import { MessageHandlersStats } from '../../exchanges/types';
import { StoreData, Writable } from '../../types';
import { ExchangeDependentPostprocessor } from './ExchangeDependentPostprocessor';

const websocketHasStats = (
  ws: { isInitialized: boolean; messageHandlersStats: MessageHandlersStats } | undefined,
): boolean => {
  if (!ws) return false;
  if (ws.isInitialized) return true;
  for (const _k in ws.messageHandlersStats) return true;
  return false;
};

export class MessageHandlerStatsPostprocessor extends ExchangeDependentPostprocessor {
  process(store: Writable<StoreData>): void {
    if (!store.messageHandlerStats) {
      store.messageHandlerStats = {};
    }

    // Hot path: don't deep-clone the stats Maps. `postMessage` already
    // structuredClones the entire payload at the worker → main boundary,
    // so the intermediate worker-side clone was duplicating that work for
    // every WS type on every throttled pipeline pass. The window between
    // this assignment and the structuredClone in `postMessage` is purely
    // synchronous (no WS frame can mutate `messageHandlersStats`
    // mid-microtask), so aliasing the live reference is safe.
    //
    // Also: replaced the `new Map([...])` allocation with direct
    // `if`-style checks, and the `Object.keys(...).length > 0` empty
    // check with a `for...in` early-break (avoids allocating a 600-key
    // array on Bybit linear's stats just to ask "is anything here?").
    const ws = this._exchange;

    if (websocketHasStats(ws.publicWebsocket as any)) {
      store.messageHandlerStats.public = ws.publicWebsocket!.messageHandlersStats;
    }
    if (websocketHasStats(ws.businessWebsocket as any)) {
      store.messageHandlerStats.business = ws.businessWebsocket!.messageHandlersStats;
    }
    if (websocketHasStats(ws.futuresWebsocket as any)) {
      store.messageHandlerStats.futures = ws.futuresWebsocket!.messageHandlersStats;
    }
    if (websocketHasStats(ws.privateWebsocket as any)) {
      store.messageHandlerStats.private = ws.privateWebsocket!.messageHandlersStats;
    }

    // BybitInverse maps publicInverseWebsocket → public
    if (ws.publicInverseWebsocket) {
      store.messageHandlerStats.public = ws.publicInverseWebsocket.messageHandlersStats;
    }

    // bitunix-style secondary public WS — merge into the primary's view.
    // Object spread is the cheapest way to combine; secondary fields
    // first so primary keys win on conflict (preserves prior semantics).
    if (ws.publicWebsocketSecondary && store.messageHandlerStats.public) {
      store.messageHandlerStats.public = {
        ...ws.publicWebsocketSecondary.messageHandlersStats,
        ...store.messageHandlerStats.public,
      };
    }
  }
}
