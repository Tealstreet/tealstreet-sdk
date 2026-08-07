import throttle from 'lodash/throttle';

import { exchangeMetadata } from '@tealstreet/safe-cex/src/exchangeMetadata';
import { OrderBook } from '../../types';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

interface SymbolSub {
  unsub: () => void;
  throttleFn: any;
  // subIds.size IS the refcount. A separate counter desyncs the moment the same
  // subId subscribes twice — Set.add is a no-op but the counter still climbs, and
  // the matching unsubscribe then leaves it stuck above zero, so the venue
  // subscription never tears down. Clients replay subscriptions after a reconnect,
  // which makes duplicate subIds routine rather than exotic.
  subIds: Set<string>;
}

export class OrderBookSubscriptionsHandler extends BaseHandler {
  // Deduplicated subscriptions keyed by symbol.
  // In shared mode (SharedWorker / CLI server), multiple sessions can subscribe to the
  // same symbol with different subIds. Without dedup, each page reload stacks another
  // exchange listener that never gets cleaned up — causing orderbook flashing as multiple
  // throttled callbacks fire at different phases of the same data stream.
  // In dedicated worker mode this is a no-op (one session = one subId per symbol).
  private _subsBySymbol = new Map<string, SymbolSub>();

  private _sendOrderBookUpdate = (data: any, o: OrderBook) => {
    this._ctx.postMessage({ op: 'listenOrderBook', data: { ...data, orderbook: o } });
    // some ticker ws feed don't include BBO or are too slow so we inject it from OB if we can. Otherwise track price,
    // quick orders can be delayed or revert to using last price
    if (o.asks[0] && o.bids[0]) {
      const foundTicker = this._exchange.store.tickers.find((t) => t.symbol === data.symbol);
      if (foundTicker) {
        if (foundTicker.ask !== o.asks[0].price || foundTicker.bid !== o.bids[0].price) {
          this._exchange.store.updateTicker(foundTicker, {
            ask: o.asks[0].price,
            bid: o.bids[0].price,
          });
        }
      }
    }
  };

  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'subscribeOrderBook': {
        // Markets not loaded yet — buffer for later
        if (this.bufferUntilMarketsLoaded(payload)) return;

        // Skip if symbol doesn't exist on this exchange
        const markets = this._exchange.store.markets;
        if (!markets.some((m) => m.symbol === payload.data.symbol)) {
          console.warn(
            `[OrderBookSubscriptionsHandler] Symbol "${payload.data.symbol}" not found on ${this._exchange.name}, skipping subscription`,
          );
          return;
        }

        const symbol = payload.data.symbol;
        const subId = payload.data.subId;
        const existing = this._subsBySymbol.get(symbol);

        if (existing) {
          // Already subscribed to this symbol — just track the extra subscriber
          existing.subIds.add(subId);
          return;
        }

        // Create new subscription
        const data = { ...payload.data };
        const f = throttle(
          this._sendOrderBookUpdate.bind(this, data),
          exchangeMetadata[this._exchange.name].orderbookTickDelay *
            (this._exchange.options?.tuning?.dataUpdateThrottleMultiplier || 1),
          { trailing: true },
        );
        const unsub = this._exchange.listenOrderBook(symbol, f) || (() => {});

        this._subsBySymbol.set(symbol, {
          unsub,
          throttleFn: f,
          subIds: new Set([subId]),
        });
        break;
      }
      case 'unsubscribeOrderBook': {
        const subId = payload.data.subId;

        // Find which symbol this subId belongs to
        for (const [symbol, sub] of this._subsBySymbol) {
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
              this._subsBySymbol.delete(symbol);
            }
            break;
          }
        }
        break;
      }
    }
  }
}
