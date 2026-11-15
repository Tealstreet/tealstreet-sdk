import type { StoreWorkerOHLCVFrameRejection } from '../StoreWorkerPayload';

import throttle from 'lodash/throttle';

import { serializeCommandError } from '../../commandOutcome';
import { Candle, OHLCVOptions } from '../../types';
import { diagnosticConsole } from '../../utils/diagnosticRedaction';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

interface SymbolIntervalSub {
  unsub: () => void;
  throttleFn?: ReturnType<typeof throttle<(candle: Candle) => void>>;
  // subIds.size IS the refcount — see OrderBookSubscriptionsHandler for why a
  // separate counter desyncs on a duplicate subId and strands the subscription.
  subIds: Set<string>;
  lastCandleTimestamp: number;
  payloadDataBySubId: Map<string, OHLCVOptions & { subId: string }>;
}

export class OHLCVSubscriptionsHandler extends BaseHandler {
  // Deduplicated subscriptions keyed by symbol:interval.
  // In shared mode (SharedWorker / CLI server), multiple sessions can subscribe to the
  // same symbol+interval with different subIds. Without dedup, each page reload stacks
  // another exchange listener that never gets cleaned up.
  // In dedicated worker mode this is a no-op (one session = one subId per symbol+interval).
  private _subsByKey = new Map<string, SymbolIntervalSub>();
  private _disposed = false;

  private _sendCandleUpdate = (sub: SymbolIntervalSub, c: Candle) => {
    for (const [subId, data] of sub.payloadDataBySubId) {
      this._ctx.postMessage({ op: 'listenOHLCV', data: { ...data, subId, candle: c } });
    }
  };

  private _getKey(symbol: string, interval: string): string {
    return `${symbol}:${interval}`;
  }

  handle(payload: StoreWorkerPayload): void {
    if (this._disposed) return;
    switch (payload.op) {
      case 'subscribeOHLCV': {
        // Markets not loaded yet — buffer for later
        if (this.bufferUntilMarketsLoaded(payload)) return;

        // Skip if symbol doesn't exist on this exchange
        const markets = this._exchange.store.markets;
        if (!markets.some((m) => m.symbol === payload.data.symbol)) {
          diagnosticConsole.warn(
            `[OHLCVSubscriptionsHandler] Symbol "${payload.data.symbol}" not found on ${this._exchange.name}, skipping subscription${
              payload.data.reset ? ' and its forced repair' : ''
            }`,
          );
          return;
        }

        const { reset, ...data } = payload.data;
        const key = this._getKey(data.symbol, data.interval);
        const subId = data.subId;
        const existing = this._subsByKey.get(key);
        // After attaching: a forced reset also stands behind this key's other subscribers.
        const repairIfReset = () => {
          if (reset) this._exchange.repairOHLCV?.({ symbol: data.symbol, interval: data.interval });
        };

        if (existing) {
          // Already subscribed to this symbol:interval — just track the subscriber
          existing.subIds.add(subId);
          existing.payloadDataBySubId.set(subId, { ...data });
          repairIfReset();
          return;
        }

        // Create new subscription
        let unsubscribe: (() => void) | undefined;
        let stopped = false;
        const sub: SymbolIntervalSub = {
          unsub: () => {
            if (stopped) return;
            stopped = true;
            sub.throttleFn?.cancel();
            unsubscribe?.();
          },
          throttleFn: undefined,
          subIds: new Set([subId]),
          lastCandleTimestamp: 0,
          payloadDataBySubId: new Map([[subId, { ...data }]]),
        };
        const f = throttle(
          this._sendCandleUpdate.bind(this, sub),
          60 * (this._exchange.options?.tuning?.dataUpdateThrottleMultiplier || 1),
          { trailing: true },
        );
        sub.throttleFn = f;
        // Cancellation must see the subscription before a source can call back.
        this._subsByKey.set(key, sub);
        try {
          unsubscribe = this._exchange.listenOHLCV(
            data,
            (c) => {
              if (this._disposed || stopped) return;
              if (c.timestamp !== sub.lastCandleTimestamp) {
                try {
                  f?.flush?.();
                  sub.lastCandleTimestamp = c.timestamp;
                } catch (_e) {
                  // Ignored
                }
              }
              if (!this._disposed && !stopped) f(c);
            },
            (signal) => {
              if (this._disposed || stopped) return;
              f.cancel();
              for (const subId of sub.subIds) {
                const data: StoreWorkerOHLCVFrameRejection = {
                  ...signal,
                  subId,
                  error: serializeCommandError(signal.error),
                };
                this._ctx.postMessage({ op: 'listenOHLCV:rejected', data });
              }
            },
          );
        } catch (error) {
          if (this._subsByKey.get(key) === sub) this._subsByKey.delete(key);
          sub.subIds.clear();
          sub.payloadDataBySubId.clear();
          sub.unsub();
          throw error;
        }
        if (stopped) {
          try {
            unsubscribe?.();
          } catch {
            /* A cancelled registration still releases its late handle once. */
          }
        } else {
          repairIfReset();
        }
        break;
      }
      case 'unsubscribeOHLCV': {
        const subId = payload.data.subId;
        this.cancelPending(subId);

        // Find which key this subId belongs to
        for (const [key, sub] of this._subsByKey) {
          if (sub.subIds.has(subId)) {
            sub.subIds.delete(subId);
            sub.payloadDataBySubId.delete(subId);

            if (sub.subIds.size === 0) {
              // Last subscriber — actually unsubscribe from exchange
              this._subsByKey.delete(key);
              try {
                sub.unsub();
                sub.throttleFn?.cancel?.();
              } catch (_e) {
                // Ignored
              }
            }
            break;
          }
        }
        break;
      }
    }
  }
  dispose() {
    this._disposed = true;
    super.dispose();
    for (const sub of this._subsByKey.values()) {
      sub.subIds.clear();
      sub.payloadDataBySubId.clear();
      sub.throttleFn?.cancel();
      try {
        sub.unsub();
      } catch {
        /* Continue releasing other subscriptions. */
      }
    }
    this._subsByKey.clear();
  }
}
