import throttle from 'lodash/throttle';

import { Candle } from '../../types';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

interface SymbolIntervalSub {
  unsub: () => void;
  throttleFn: any;
  // subIds.size IS the refcount — see OrderBookSubscriptionsHandler for why a
  // separate counter desyncs on a duplicate subId and strands the subscription.
  subIds: Set<string>;
  lastCandleTimestamp: number;
  lastSentCandle?: Candle;
}

const areCandlesEqual = (left: Candle | undefined, right: Candle | undefined): boolean => {
  if (!left || !right) return false;

  return (
    left.timestamp === right.timestamp &&
    left.open === right.open &&
    left.high === right.high &&
    left.low === right.low &&
    left.close === right.close &&
    left.volume === right.volume &&
    left.lastSide === right.lastSide
  );
};

export class OHLCVSubscriptionsHandler extends BaseHandler {
  // Deduplicated subscriptions keyed by symbol:interval.
  // In shared mode (SharedWorker / CLI server), multiple sessions can subscribe to the
  // same symbol+interval with different subIds. Without dedup, each page reload stacks
  // another exchange listener that never gets cleaned up.
  // In dedicated worker mode this is a no-op (one session = one subId per symbol+interval).
  private _subsByKey = new Map<string, SymbolIntervalSub>();

  private _sendCandleUpdate = (data: any, c: Candle) => {
    this._ctx.postMessage({ op: 'listenOHLCV', data: { ...data, candle: c } });
  };

  private _getKey(symbol: string, interval: string): string {
    return `${symbol}:${interval}`;
  }

  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'subscribeOHLCV': {
        // Markets not loaded yet — buffer for later
        if (this.bufferUntilMarketsLoaded(payload)) return;

        // Skip if symbol doesn't exist on this exchange
        const markets = this._exchange.store.markets;
        if (!markets.some((m) => m.symbol === payload.data.symbol)) {
          console.warn(
            `[OHLCVSubscriptionsHandler] Symbol "${payload.data.symbol}" not found on ${this._exchange.name}, skipping subscription`,
          );
          return;
        }

        const key = this._getKey(payload.data.symbol, payload.data.interval);
        const subId = payload.data.subId;
        const existing = this._subsByKey.get(key);

        if (existing) {
          // Already subscribed to this symbol:interval — just track the subscriber
          existing.subIds.add(subId);
          return;
        }

        // Create new subscription
        const data = { ...payload.data };
        const f = throttle(
          this._sendCandleUpdate.bind(this, data),
          60 * (this._exchange.options?.tuning?.dataUpdateThrottleMultiplier || 1),
          { trailing: true },
        );

        const sub: SymbolIntervalSub = {
          unsub: () => {},
          throttleFn: f,
          subIds: new Set([subId]),
          lastCandleTimestamp: 0,
          lastSentCandle: undefined,
        };

        const ohlcvSub = this._exchange.listenOHLCV(payload.data, (c) => {
          if (c.timestamp !== sub.lastCandleTimestamp) {
            try {
              f?.flush?.();
              sub.lastCandleTimestamp = c.timestamp;
            } catch (_e) {
              // Ignored
            }
          }
          if (areCandlesEqual(c, sub.lastSentCandle)) {
            return;
          }
          sub.lastSentCandle = { ...c };
          f(c);
        });
        sub.unsub = ohlcvSub || (() => {});

        this._subsByKey.set(key, sub);
        break;
      }
      case 'unsubscribeOHLCV': {
        const subId = payload.data.subId;

        // Find which key this subId belongs to
        for (const [key, sub] of this._subsByKey) {
          if (sub.subIds.has(subId)) {
            sub.subIds.delete(subId);

            if (sub.subIds.size === 0) {
              // Last subscriber — actually unsubscribe from exchange
              try {
                sub.unsub();
                sub.throttleFn?.cancel?.();
              } catch (_e) {
                // Ignored
              }
              this._subsByKey.delete(key);
            }
            break;
          }
        }
        break;
      }
    }
  }
}
