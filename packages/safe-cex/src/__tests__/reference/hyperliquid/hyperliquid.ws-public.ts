import flatten from 'lodash/flatten';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Candle, LogSeverity, OHLCVOptions, OrderBook, Ticker, Writable } from '../../../types';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { HyperliquidExchange } from './hyperliquid.exchange';
import { decodeHyperliquidFastAssetCtxs } from './hyperliquid.fast-asset-ctxs';
import { AllMids, HyperliquidCandle, WsBbo, WsBook, WSS_URLS } from '../../../exchanges/hyperliquid/hyperliquid.types';
import {
  formatHyperliquidWsDiagnostic,
  shouldEmitHyperliquidWsReceiveDiagnostic,
  shouldEmitHyperliquidWsSendDiagnostic,
} from './hyperliquid.ws-diagnostics';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: any) => void>;
};

type SubscribedTopics = {
  [id: string]: number;
};

export class HyperliquidPublicWebSocket extends BaseWebSocket<HyperliquidExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {};

  // Track active subscriptions to prevent duplicates (key = subscription JSON)
  private activeSubscriptions: Set<string> = new Set();

  // Independent ping loop
  private hlPingTimeoutId?: ReturnType<typeof setTimeout>;

  constructor(parent: HyperliquidExchange) {
    super(parent);
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION']);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.parent.log('Public WebSocket connection established', LogSeverity.Debug);
      this.trackLastReceived();
      this.resetOrderbook();
      // Clear subscription tracking on new connection (reconnect needs fresh subscriptions)
      this.activeSubscriptions.clear();
      this.subscribePublicTopics();
      this.startPingLoop();
    }
  };

  private startPingLoop = () => {
    // Clear any existing timeout
    this.stopPingLoop();
    // Don't send ping immediately on connect - wait for first scheduled ping
    // Sending ping before server responds to subscription can cause issues
    this.schedulePing();
  };

  // Ping interval: 15 seconds (CCXT uses 20s, our previous 1s was too aggressive)
  private static readonly PING_INTERVAL_MS = 15_000;

  private schedulePing = () => {
    this.hlPingTimeoutId = setTimeout(() => {
      if (!this.isDisposed && this.isConnected) {
        this.ping();
        this.schedulePing();
      }
    }, HyperliquidPublicWebSocket.PING_INTERVAL_MS);
  };

  private stopPingLoop = () => {
    if (this.hlPingTimeoutId) {
      clearTimeout(this.hlPingTimeoutId);
      this.hlPingTimeoutId = undefined;
    }
  };

  subscribePublicTopics = () => {
    // Keep allMids as the stable full-snapshot fallback, and overlay the
    // compressed snapshot+diff fastAssetCtxs stream for fresh mark/mid prices.
    this.sendWithDiagnostics(
      JSON.stringify({
        method: 'subscribe',
        subscription: {
          type: 'allMids',
        },
      }),
    );
    this.sendWithDiagnostics(
      JSON.stringify({
        method: 'subscribe',
        subscription: {
          type: 'fastAssetCtxs',
        },
      }),
    );

    // Subscribe to user-requested topics (candles, orderbooks)
    this.subscribeUserTopics();
  };

  /**
   * Send a subscription request only if not already subscribed
   * Returns true if subscription was sent, false if already subscribed
   */
  private sendSubscription = (subscription: Record<string, unknown>): boolean => {
    const key = JSON.stringify(subscription);
    if (this.activeSubscriptions.has(key)) {
      return false;
    }
    this.activeSubscriptions.add(key);
    this.sendWithDiagnostics(JSON.stringify({ method: 'subscribe', subscription }));
    return true;
  };

  private sendUnsubscription = (subscription: Record<string, unknown>): void => {
    this.activeSubscriptions.delete(JSON.stringify(subscription));
    this.sendWithDiagnostics(JSON.stringify({ method: 'unsubscribe', subscription }));
  };

  subscribeUserTopics = () => {
    const topics = flatten(Object.keys(this.topics));
    for (const topic of topics) {
      const [id, symbol, interval] = topic.split('.');

      if (id === 'candle') {
        this.sendSubscription({
          type: 'candle',
          coin: symbol,
          interval: interval,
        });
      } else if (id === 'l2Book') {
        this.sendSubscription({
          type: 'l2Book',
          coin: symbol,
        });
        this.sendSubscription({
          type: 'bbo',
          coin: symbol,
        });
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithDiagnostics(JSON.stringify({ method: 'ping' }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        this.emitReceiveDiagnostics(data, json);
        // Debug: log subscription responses
        if (json?.channel === 'subscriptionResponse' || json?.channel === 'SubscriptionResponse') {
          this.parent.log(`Subscription response: ${JSON.stringify(json)}`, LogSeverity.Debug);
        }

        // Per-symbol channels are recorded under their exact handler key at the
        // dispatch site instead (`bbo.{coin}`, `candle.{s}.{i}`). Recording the
        // bare channel here would key every symbol to one entry that matches no
        // `messageHandlers` key, so `reconcileMessageHandlerStats` could never
        // see it go idle and the topic would be reported as a live feed forever.
        // l2Book/error/subscriptionResponse are not stats-tracked at all.
        if (
          json?.channel !== 'l2Book' &&
          json?.channel !== 'bbo' &&
          json?.channel !== 'candle' &&
          json?.channel !== 'error' &&
          json?.channel !== 'subscriptionResponse' &&
          json?.channel !== 'SubscriptionResponse'
        ) {
          this.recordMessageHandlerStats(json?.channel);
        }
        // Deliberate hack: bump 'pong' stats on every frame, not just real
        // pongs. Hyperliquid's pong cadence is unreliable enough that the
        // staleness watchdog (which gates on `now - pong.lastMessageTimestamp`)
        // would flag the socket as dead and trigger reconnects even when
        // tickers/orderbooks are flowing normally. Treating any inbound frame
        // as a heartbeat keeps the connection alive. Don't "fix" this without
        // also tightening the watchdog logic.
        this.recordMessageHandlerStats('pong');

        switch (json?.channel) {
          case 'error':
            this.parent.log(`WebSocket error: ${JSON.stringify(json)}`, LogSeverity.Error);
            break;
          case 'pong':
            // Just record latency, don't schedule next ping (handled by ping loop)
            if (this.pingAt) {
              const diff = performance.now() - this.pingAt;
              this.store.update({ latency: Math.round(diff / 2) });
            }
            break;
          case 'allMids':
            this.handleAllMids(json?.data?.mids || {});
            break;
          case 'fastAssetCtxs':
            this.handleFastAssetCtxs(json?.data);
            break;
          case 'candle': {
            // Hyperliquid candle subscriptions are per (coin, interval) and a
            // single frame is always for one (coin, interval) — sample the
            // first entry to construct the exact handler key, then look up
            // directly. The prior Object.keys.filter walked every registered
            // handler key and called every candle parser; each parser then
            // re-filtered internally by `entry.s === symbol && entry.i === i`.
            const candleData = json?.data;
            const sample = Array.isArray(candleData) ? candleData[0] : candleData;
            if (!sample?.s || !sample?.i) break;
            const handlers = this.messageHandlers[`candle.${sample.s}.${sample.i}`];
            if (handlers) {
              const payload = candleData || [];
              // Indexed for-loop — `for...of` over an Array allocates an
              // iterator object per dispatch; not free at WS frame rate.
              for (let i = 0; i < handlers.length; i++) handlers[i](payload);
            }
            break;
          }
          case 'l2Book': {
            const coin = json?.data?.coin;
            if (!coin) break;

            // Direct Map lookup — the handler key is the exact full topic.
            // The prior Object.keys + filter walked all registered handler
            // keys to find the single match, paying O(N_handlers) for what
            // is a single-element lookup.
            const handlers = this.messageHandlers[`l2Book.${coin}`];
            if (handlers) {
              const payload = json?.data || [];
              // Individual try/catch per handler to prevent one crashed
              // handler from blocking subsequent handlers. Indexed for-loop
              // avoids iterator allocation per frame.
              for (let i = 0; i < handlers.length; i++) {
                try {
                  handlers[i](payload);
                } catch (_e) {}
              }
            }
            break;
          }
          case 'bbo': {
            const coin = json?.data?.coin;
            if (!coin) break;

            const bboTopic = `bbo.${coin}`;
            const handlers = this.messageHandlers[bboTopic];
            if (handlers) {
              this.recordMessageHandlerStats(bboTopic);
              const payload = json?.data;
              for (let i = 0; i < handlers.length; i++) {
                try {
                  handlers[i](payload);
                } catch (_e) {}
              }
            }
            break;
          }
        }
      } catch (_e) {}
    }
  };

  private sendWithDiagnostics = (data: string) => {
    this.send(data);
    if (shouldEmitHyperliquidWsSendDiagnostic(data)) {
      this.emitWsDiagnosticSend(data, {
        format: (message) => formatHyperliquidWsDiagnostic('public', message),
      });
    }
  };

  private emitReceiveDiagnostics = (data: unknown, json: unknown) => {
    if (shouldEmitHyperliquidWsReceiveDiagnostic('public', json)) {
      this.emitWsDiagnosticReceive(data, {
        format: (message) => formatHyperliquidWsDiagnostic('public', message),
      });
    }
  };

  // Reusable two-field update buffer for `handleAllMids`. allMids
  // arrives every heartbeat with ~600 entries; allocating per-entry
  // burns ~600 short-lived objects per frame. The store's
  // `updateTicker` iterates this buffer and writes fields onto the
  // ticker without retaining the reference, so reusing one buffer is
  // safe. `allMids` IS Hyperliquid's published `midPx` — populate
  // both `last` and `midPrice` so the cli `$mid` resolver reads the
  // canonical mid rather than the impact-derived `(bid+ask)/2`.
  private _midsUpdateBuf: Partial<Writable<Ticker>> = { last: 0, midPrice: 0 };

  handleAllMids = (data: AllMids) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const tickersById = this.parent.store.tickersById;
    // Skip Set allocation when there are no positions — the Set is
    // only consumed by `syncBalanceFromLivePositions` to filter which
    // positions need re-cumulating, and that function bails fast when
    // positions is empty anyway.
    const hasPositions = this.parent.store.positions.length > 0;
    const updatedSymbols: Set<string> | undefined = hasPositions ? new Set<string>() : undefined;
    const updateBuf = this._midsUpdateBuf;
    // for...in avoids the Object.entries tuple-array allocation. allMids
    // arrives once per heartbeat with ~600+ entries, so dropping the
    // entries array saves ~600 short-lived 2-tuples per frame.
    for (const tickerPrefix in data) {
      const ticker = tickersById[tickerPrefix];
      if (!ticker) continue;
      const last = parseFloat(data[tickerPrefix]);
      updateBuf.last = last;
      updateBuf.midPrice = last;
      this.parent.store.updateTicker(ticker, updateBuf);
      if (updatedSymbols) updatedSymbols.add(ticker.symbol);
    }

    if (updatedSymbols && updatedSymbols.size > 0) {
      this.parent.syncBalanceFromLivePositions(updatedSymbols);
    }
  };

  private _fastMarkUpdateBuf: Partial<Writable<Ticker>> = { mark: 0 };
  private _fastMidUpdateBuf: Partial<Writable<Ticker>> = { last: 0, midPrice: 0 };
  private _fastMarkAndMidUpdateBuf: Partial<Writable<Ticker>> = { mark: 0, last: 0, midPrice: 0 };

  handleFastAssetCtxs = (payload: string) => {
    const data = decodeHyperliquidFastAssetCtxs(payload);
    const tickersById = this.parent.store.tickersById;
    const hasPositions = this.parent.store.positions.length > 0;
    const updatedSymbols: Set<string> | undefined = hasPositions ? new Set<string>() : undefined;

    for (const coin in data) {
      const ticker = tickersById[coin];
      if (!ticker) continue;

      const context = data[coin];
      const mark = Number(context?.markPx);
      const mid = Number(context?.midPx);
      const hasMark = Number.isFinite(mark) && mark > 0;
      const hasMid = Number.isFinite(mid) && mid > 0;

      if (hasMark && hasMid) {
        const update = this._fastMarkAndMidUpdateBuf;
        update.mark = mark;
        update.last = mid;
        update.midPrice = mid;
        this.parent.store.updateTicker(ticker, update);
        if (updatedSymbols) updatedSymbols.add(ticker.symbol);
      } else if (hasMark) {
        const update = this._fastMarkUpdateBuf;
        update.mark = mark;
        this.parent.store.updateTicker(ticker, update);
      } else if (hasMid) {
        const update = this._fastMidUpdateBuf;
        update.last = mid;
        update.midPrice = mid;
        this.parent.store.updateTicker(ticker, update);
        if (updatedSymbols) updatedSymbols.add(ticker.symbol);
      }
    }

    if (updatedSymbols && updatedSymbols.size > 0) {
      this.parent.syncBalanceFromLivePositions(updatedSymbols);
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    // Use market.id for the coin parameter (API requires "ETH", "BTC", etc.)
    const market = this.parent.store.markets.find((m) => m.symbol === opts.symbol);
    if (!market) {
      console.error(`[Hyperliquid] listenOHLCV: Market not found for symbol "${opts.symbol}", skipping subscription`);
      return () => {};
    }
    const symbol = market.id;
    const topic = `candle.${symbol}.${opts.interval}`;
    const handler = `candle.${symbol}.${opts.interval}`;

    const parser = (data: HyperliquidCandle[]) => {
      // Indexed for-loop over for...of (no iterator allocation per
      // candle frame). The (s, i) check is defensive — the dispatcher
      // already routes by exact `candle.{s}.{i}` key based on the
      // sample's first entry, but a heterogeneous frame would otherwise
      // pass mismatched entries through.
      const entries = Array.isArray(data) ? data : [data];
      // Hoisted out of the loop: the dispatcher already matched at least
      // one entry by sampling the first one, so any frame reaching this
      // parser is "active". Avoids per-matched-entry stats writes on
      // multi-entry frames.
      this.recordMessageHandlerStats(topic);
      for (let i = 0; i < entries.length; i++) {
        const entry = entries[i];
        if (symbol === entry.s && entry.i === opts.interval) {
          callback({
            timestamp: entry.t / 1000,
            open: parseFloat(entry.o),
            high: parseFloat(entry.h),
            low: parseFloat(entry.l),
            close: parseFloat(entry.c),
            volume: parseFloat(entry.v),
          });
        }
      }
    };

    let waitingTimeout: ReturnType<typeof setTimeout> | undefined;
    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;

        // Use sendSubscription to avoid duplicates
        this.sendSubscription({
          type: 'candle',
          coin: symbol,
          interval: opts.interval,
        });
      } else {
        waitingTimeout = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      // Cancel any pending waitForConnectedAndSubscribe polling
      if (waitingTimeout) {
        clearTimeout(waitingTimeout);
        waitingTimeout = undefined;
      }

      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        this.cleanupMessageHandlerStats(handler);
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const market = this.parent.store.markets.find((market) => market.symbol === symbol);
    if (!market) {
      this.parent.emitAndLogError(
        `Market with symbol ${symbol} not found. Available markets: ${this.parent.store.markets.length}`,
        'listenOrderBook',
      );
      throw new Error(`Market with symbol ${symbol} not found`);
    }
    // Use market.id for WebSocket subscription (API requires lowercase, no -PERP suffix)
    const subSymbol = market.id;
    const pricePrecision = market.precision.price;
    if (!pricePrecision) {
      this.parent.emitAndLogError(`Market with symbol ${symbol} has no price precision`, 'listenOrderBook');
      throw new Error(`Market with symbol ${symbol} has no price precision`);
    }

    const topic = `l2Book.${subSymbol}`;
    const handler = `l2Book.${subSymbol}`;
    const bboHandler = `bbo.${subSymbol}`;

    const innerId = nanoid();

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    let depthBook: OrderBook | undefined;
    let depthTime = 0;
    let latestBbo: WsBbo | undefined;

    const emitCombinedBook = () => {
      if (!depthBook || !this.orderBookInnerState[innerId]?.isSnapshotLoaded) return;

      const orderBook: OrderBook = {
        bids: depthBook.bids.map((level) => ({ ...level, total: 0 })),
        asks: depthBook.asks.map((level) => ({ ...level, total: 0 })),
      };

      // The standard l2Book feed carries the UI's 20-level depth, while bbo
      // updates on every block where the best price/size changes. Overlay only
      // a BBO at least as new as the depth snapshot and discard stale levels
      // that would otherwise appear better than the current best.
      if (latestBbo && latestBbo.time >= depthTime) {
        const overlaySide = (
          levels: OrderBook['bids'],
          best: WsBbo['bbo'][number],
          isBid: boolean,
        ): OrderBook['bids'] => {
          if (!best) return [];
          const bestPrice = Number(best.px);
          const bestAmount = Number(best.sz);
          if (!Number.isFinite(bestPrice) || !Number.isFinite(bestAmount)) return levels;

          const combined: OrderBook['bids'] = [{ price: bestPrice, amount: bestAmount, total: 0 }];
          for (let i = 0; i < levels.length && combined.length < 20; i++) {
            const level = levels[i];
            if (isBid ? level.price < bestPrice : level.price > bestPrice) {
              combined.push({ ...level, total: 0 });
            }
          }
          return combined;
        };

        orderBook.bids = overlaySide(orderBook.bids, latestBbo.bbo[0], true);
        orderBook.asks = overlaySide(orderBook.asks, latestBbo.bbo[1], false);
      }

      if (orderBook.asks.length !== 0 || orderBook.bids.length !== 0) {
        calcOrderBookTotal(orderBook);
        callback(orderBook);
      }
    };

    const parser = (json: Data) => {
      const event = json as WsBook;

      const bidPrice1 = event.levels[0][0]?.px;
      const bidPrice2 = event.levels[0][1]?.px;
      if (!bidPrice1 || !bidPrice2) {
        this.parent.log(`Order book for ${subSymbol} is empty`);
        return;
      }

      const orderBook: OrderBook = {
        bids: [],
        asks: [],
      };

      // bids — plain for-loop with direct property access avoids the
      // per-level destructuring closure of forEach({ px, sz }).
      const bidLevels = event.levels[0];
      for (let i = 0; i < bidLevels.length; i++) {
        const lvl = bidLevels[i];
        orderBook.bids.push({
          price: parseFloat(lvl.px),
          amount: parseFloat(lvl.sz),
          total: 0,
        });
      }

      // asks
      const askLevels = event.levels[1];
      for (let i = 0; i < askLevels.length; i++) {
        const lvl = askLevels[i];
        orderBook.asks.push({
          price: parseFloat(lvl.px),
          amount: parseFloat(lvl.sz),
          total: 0,
        });
      }

      // Guard against stale parser from cleaned-up subscription
      if (this.orderBookInnerState[innerId]) {
        this.orderBookInnerState[innerId].isSnapshotLoaded = true;
      }

      // Hyperliquid l2Book frames arrive pre-sorted: levels[0] is bids
      // descending and levels[1] is asks ascending. Keep the full standard
      // depth snapshot and overlay the fresher BBO before emitting.
      depthBook = orderBook;
      depthTime = Number(event.time) || 0;
      emitCombinedBook();
    };

    const bboParser = (json: Data) => {
      const event = json as WsBbo;
      if (!Array.isArray(event.bbo) || !Number.isFinite(Number(event.time))) return;
      if (latestBbo && event.time < latestBbo.time) return;
      latestBbo = event;
      emitCombinedBook();
    };

    let waitingTimeout: ReturnType<typeof setTimeout> | undefined;
    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        const shouldSubscribe = !this.messageHandlers[handler]?.length;
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.messageHandlers[bboHandler] = [...(this.messageHandlers[bboHandler] || []), bboParser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;

        if (shouldSubscribe) {
          this.sendSubscription({ type: 'l2Book', coin: subSymbol });
          this.sendSubscription({ type: 'bbo', coin: subSymbol });
        }
      } else {
        waitingTimeout = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      // Cancel any pending waitForConnectedAndSubscribe polling to prevent
      // orphaned parsers from being added after cleanup
      if (waitingTimeout) {
        clearTimeout(waitingTimeout);
        waitingTimeout = undefined;
      }

      // Clean up orderbook inner state to prevent memory leak
      delete this.orderBookInnerState[innerId];

      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.messageHandlers[bboHandler] = [...(this.messageHandlers[bboHandler] || []).filter((f) => f !== bboParser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        this.sendUnsubscription({ type: 'l2Book', coin: subSymbol });
        this.sendUnsubscription({ type: 'bbo', coin: subSymbol });
      }

      // No bespoke bbo stats cleanup here any more. Stats are keyed by the exact
      // `bbo.{coin}` handler key, and the handler array above is emptied rather
      // than deleted, so `reconcileMessageHandlerStats` in base.ws.ts zeroes it
      // on the next walk. bbo remains the only orderbook liveness signal on this
      // venue, since l2Book is not stats-tracked.
    };
  };

  dispose() {
    // Stop ping loop
    this.stopPingLoop();

    // Call parent dispose
    super.dispose();
  }
}
