import type { Candle, OHLCVOptions, OrderBook } from '../../../types';
import type { BinanceExchange } from './binance.exchange';

import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { v } from '../../../utils/get-key';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import {
  applyBinanceDepthUpdate,
  getBufferedBinanceDepthUpdates,
  loadBinanceOrderBookSnapshot,
  resetBinanceOrderBookSyncState,
} from './binance.orderbook';
import { BASE_WS_URL, ENDPOINTS } from '../../../exchanges/binance/binance.types';

type Data = Array<Record<string, any>>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

/**
 * Shared O(1) dispatch for Binance public-WS frames.
 *
 * The previous dispatch did `Object.entries(messageHandlers)` per frame, then
 * for each registered handler key took its leftmost dot-segment and ran
 * `data.includes(\`e":"\${leftmost}\`)` — an O(N) substring scan over the
 * (multi-KB) message body per registered handler. With many depthUpdate /
 * kline subscriptions plus broadcast handlers this dominated CPU.
 *
 * Replacement: extract the `e` field once via targeted indexOf, then dispatch:
 *   1) broadcast handler keyed by event type alone (e.g. \`24hrTicker\`)
 *   2) per-symbol handler keyed by \`\${event}.\${symbol}\` (e.g. depthUpdate.BTCUSDT)
 *   3) prefix iteration as a fallback for multi-segment keys whose middle
 *      segments live inside the JSON body (e.g. kline.{interval}.{symbol},
 *      where the interval comes from \`k.i\`). Only walked when neither (1)
 *      nor (2) matched, and only checks keys starting with \`\${event}.\`.
 */
export const dispatchBinanceMessage = (
  messageHandlers: MessageHandlers,
  data: string,
  recordStats: (topic: string) => void,
): void => {
  // Extract the `"e":"<eventType>"` field once.
  const eIdx = data.indexOf('"e":"');
  if (eIdx < 0) return;
  const eStart = eIdx + 5;
  const eEnd = data.indexOf('"', eStart);
  if (eEnd <= eStart) return;
  const eventType = data.slice(eStart, eEnd);

  // (1) Broadcast handler (24hrTicker, markPriceUpdate, bookTicker, …)
  const broadcast = messageHandlers[eventType];

  // (2) Per-symbol handler — try `${eventType}.${symbol}` if the frame has a
  //     top-level "s":"<symbol>" field (depthUpdate, bookTicker variants).
  let exact: Array<(json: Data) => void> | undefined;
  const sIdx = data.indexOf('"s":"', eEnd);
  if (sIdx >= 0) {
    const sStart = sIdx + 5;
    const sEnd = data.indexOf('"', sStart);
    if (sEnd > sStart) {
      exact = messageHandlers[`${eventType}.${data.slice(sStart, sEnd)}`];
    }
  }

  // (3) Prefix fallback — only used when (1) and (2) miss. Covers
  //     kline.{interval}.{symbol} keys: interval lives in `k.i` so we cannot
  //     construct the exact key from a substring scan; iterate the (small)
  //     set of `${eventType}.*` handlers instead. Each parser internally
  //     filters by `k.s` / `k.i`, matching the prior dispatch contract.
  let prefixCalls: Array<Array<(json: Data) => void>> | undefined;
  if (!broadcast?.length && !exact?.length) {
    const prefix = `${eventType}.`;
    for (const key in messageHandlers) {
      if (key.length > prefix.length && key.startsWith(prefix)) {
        const arr = messageHandlers[key];
        if (arr.length) {
          if (!prefixCalls) prefixCalls = [];
          prefixCalls.push(arr);
        }
      }
    }
  }

  if (!broadcast?.length && !exact?.length && (!prefixCalls || prefixCalls.length === 0)) return;

  recordStats(eventType);
  const json = jsonParse(data);
  if (!json) return;
  const events = Array.isArray(json) ? json : [json];
  if (broadcast?.length) {
    for (const cb of broadcast) cb(events);
  }
  if (exact?.length) {
    for (const cb of exact) cb(events);
  }
  if (prefixCalls) {
    for (const handlers of prefixCalls) {
      for (const cb of handlers) cb(events);
    }
  }
};

/**
 * Binance USDⓈ-M Futures market-data WebSocket (`/market/ws`).
 *
 * Carries regular market data: 24hrTicker, markPriceUpdate, kline, aggTrade, etc.
 * Split out from the high-frequency `/public/ws` (bookTicker, depth) on 2026-04-23
 * when Binance retired the legacy `/ws` endpoint.
 */
export class BinanceMarketWebsocket extends BaseWebSocket<BinanceExchange> {
  messageHandlers: MessageHandlers = {
    '24hrTicker': [(d: Data) => this.handleTickerStreamEvents(d)],
    markPriceUpdate: [(d: Data) => this.handleMarkPriceStreamEvents(d)],
  };

  topics: { [key: string]: number } = {};
  private nextSubscriptionId = 2;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(BASE_WS_URL.market[this.parent.options.testnet ? 'testnet' : 'livenet']);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      const payload = {
        id: 1,
        method: 'SUBSCRIBE',
        params: ['!ticker@arr', '!markPrice@arr@1s'],
      };

      this.send(JSON.stringify(payload));

      this.subscribe();
    }
  };

  // Re-subscribes to any on-demand kline streams after reconnect
  subscribe = () => {
    let id = 10;
    for (const topic of Object.keys(this.topics)) {
      if (!Number.isFinite(this.topics[topic]) || this.topics[topic] <= 0) continue;
      this.send(JSON.stringify({ method: 'SUBSCRIBE', params: [topic], id: id++ }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();
    dispatchBinanceMessage(this.messageHandlers, data, this.recordHandlerStats);
  };

  private recordHandlerStats = (topic: string) => this.recordMessageHandlerStats(topic);

  handleTickerStreamEvents = (events: Array<Record<string, any>>) => {
    // Plain for-loop avoids the per-event closure allocation of forEach.
    // `!ticker@arr` arrives once/sec with all ~600 tickers in a single frame.
    const tickersBySymbol = this.parent.store.tickersBySymbol;
    for (let i = 0; i < events.length; i++) {
      const event = events[i];
      const ticker = tickersBySymbol[event.s];
      if (!ticker) continue;
      this.store.updateTicker(ticker, {
        last: parseFloat(v(event, 'c')),
        percentage: parseFloat(v(event, 'P')),
        volume: parseFloat(v(event, 'v')),
        quoteVolume: parseFloat(v(event, 'q')),
      });
    }
  };

  handleMarkPriceStreamEvents = (events: Array<Record<string, any>>) => {
    const tickersBySymbol = this.parent.store.tickersBySymbol;
    for (let i = 0; i < events.length; i++) {
      const event = events[i];
      const ticker = tickersBySymbol[event.s];
      if (!ticker) continue;
      this.store.updateTicker(ticker, {
        mark: parseFloat(v(event, 'p')),
        index: parseFloat(v(event, 'i')),
        fundingRate: parseFloat(v(event, 'r')),
        fundingTime: parseFloat(v(event, 'T')),
      });
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = `${opts.symbol.toLowerCase()}@kline_${opts.interval}`;
    const handler = `kline.${opts.interval}.${opts.symbol}`;
    let timeoutId: NodeJS.Timeout | null = null;
    let isActive = true;
    let isUnsubscribed = false;

    const parser = ([json]: Data) => {
      if (opts.symbol === json.k.s && json.k.i === opts.interval) {
        callback({
          timestamp: json.k.t / 1000,
          open: parseFloat(json.k.o),
          high: parseFloat(json.k.h),
          low: parseFloat(json.k.l),
          close: parseFloat(json.k.c),
          volume: parseFloat(json.k.v),
        });
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (!isActive || this.isDisposed) return;

      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        this.topics[topic] = (this.topics[topic] || 0) + 1;

        const payload = { method: 'SUBSCRIBE', params: [topic], id: this.nextSubscriptionId++ };
        this.send(JSON.stringify(payload));
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      if (isUnsubscribed) return;
      isUnsubscribed = true;
      isActive = false;

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      const nextHandlers = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      if (nextHandlers.length) {
        this.messageHandlers[handler] = nextHandlers;
      } else {
        delete this.messageHandlers[handler];
      }

      const nextTopicCount = (this.topics[topic] || 0) - 1;
      if (nextTopicCount > 0) {
        this.topics[topic] = nextTopicCount;
      } else {
        delete this.topics[topic];
      }

      if (this.isConnected && nextHandlers.length === 0) {
        const payload = { method: 'UNSUBSCRIBE', params: [topic], id: this.nextSubscriptionId++ };
        this.cleanupMessageHandlerStats(handler);
        this.send(JSON.stringify(payload));
      }
    };
  };
}

/**
 * Binance USDⓈ-M Futures public WebSocket (`/public/ws`).
 *
 * Carries high-frequency public data: bookTicker, @depth (and rpiDepth).
 * Owns a sibling `BinanceMarketWebsocket` connection for the regular market-data
 * channels (ticker, markPrice, kline) that live on `/market/ws` after the
 * 2026-04-23 URL split. The exchange continues to interact with a single
 * `publicWebsocket` reference; this class fans out to the two underlying sockets
 * transparently.
 */
export class BinancePublicWebsocket extends BaseWebSocket<BinanceExchange> {
  messageHandlers: MessageHandlers = {
    bookTicker: [(d: Data) => this.handleBookTickersStreamEvents(d)],
  };

  topics: { [key: string]: number } = {};

  marketWebsocket: BinanceMarketWebsocket;

  constructor(parent: BinanceExchange) {
    super(parent);
    this.marketWebsocket = new BinanceMarketWebsocket(parent);
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet']);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);

      // Bring the sibling /market/ws connection up alongside /public/ws.
      // Reconnects on either socket are handled independently by BaseWebSocket.
      this.marketWebsocket.connectAndSubscribe();
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      // bookTicker is high-frequency and only needed when we actually trade.
      // Skip it for public-data-only mode to reduce ingress.
      // TODO: maybe something better here...
      const params = this.parent.options.publicDataOnly ? [] : ['!bookTicker'];

      if (params.length > 0) {
        const payload = { id: 1, method: 'SUBSCRIBE', params };
        this.send(JSON.stringify(payload));
      }

      this.resetOrderbook();
      this.subscribe();
    }
  };

  // Re-subscribes to any on-demand depth streams after reconnect
  subscribe = () => {
    let id = 10;
    for (const topic of Object.keys(this.topics)) {
      if (!Number.isFinite(this.topics[topic]) || this.topics[topic] <= 0) continue;
      this.send(JSON.stringify({ method: 'SUBSCRIBE', params: [topic], id: id++ }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();
    dispatchBinanceMessage(this.messageHandlers, data, this.recordHandlerStats);
  };

  private recordHandlerStats = (topic: string) => this.recordMessageHandlerStats(topic);

  handleBookTickersStreamEvents = (events: Array<Record<string, any>>) => {
    const tickersBySymbol = this.parent.store.tickersBySymbol;
    for (let i = 0; i < events.length; i++) {
      const event = events[i];
      const ticker = tickersBySymbol[event.s];
      if (!ticker) continue;
      this.store.updateTicker(ticker, {
        bid: parseFloat(v(event, 'b')),
        ask: parseFloat(v(event, 'a')),
      });
    }
  };

  // Kline streams now live on /market/ws — forward to the sibling socket.
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.marketWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    const innerId = nanoid();

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    const topic = `${symbol.toLowerCase()}@depth`;
    const handler = `depthUpdate.${symbol}`;
    const orderBook: OrderBook = { bids: [], asks: [] };
    const innerState = this.orderBookInnerState[innerId];
    const syncState = {};
    let isFetchingSnapshot = false;

    const resetLocalOrderBook = (initialUpdate?: Record<string, any>) => {
      orderBook.asks = [];
      orderBook.bids = [];
      innerState.updates = initialUpdate ? [initialUpdate] : [];
      innerState.isSnapshotLoaded = false;
      resetBinanceOrderBookSyncState(syncState);
    };

    const fetchSnapshot = async () => {
      if (isFetchingSnapshot) return;
      isFetchingSnapshot = true;

      try {
        const { data } = await this.parent.publicXhr.get(ENDPOINTS.ORDERBOOK, {
          params: { symbol, limit: 1000 },
        });

        if (this.isDisposed) {
          return;
        }

        loadBinanceOrderBookSnapshot(orderBook, data, syncState);

        const bufferedUpdates = getBufferedBinanceDepthUpdates(innerState.updates, data.lastUpdateId);
        innerState.isSnapshotLoaded = true;
        innerState.updates = [];

        for (const update of bufferedUpdates) {
          const result = applyBinanceDepthUpdate(orderBook, update, syncState, {
            requirePreviousFinalUpdateId: true,
          });
          if (result === 'gap') {
            resetLocalOrderBook(update);
            setTimeout(() => void fetchSnapshot(), 0);
            return;
          }
        }

        calcOrderBookTotal(orderBook);

        callback(orderBook);
      } finally {
        isFetchingSnapshot = false;
      }
    };

    const parser = ([data]: Data) => {
      // incorrect symbol, we don't take account
      if (data.s !== symbol) return;

      // first update, request snapshot
      if (!innerState.isSnapshotLoaded && innerState.updates.length === 0) {
        resetLocalOrderBook(data);
        void fetchSnapshot();
        return;
      }

      // more updates, but snapshot is not loaded yet
      if (!innerState.isSnapshotLoaded) {
        innerState.updates.push(data);
        return;
      }

      // snapshot is loaded, apply updates and callback
      const result = applyBinanceDepthUpdate(orderBook, data, syncState, {
        requirePreviousFinalUpdateId: true,
      });
      if (result === 'gap') {
        resetLocalOrderBook(data);
        void fetchSnapshot();
        return;
      }
      if (result === 'ignored') {
        return;
      }
      calcOrderBookTotal(orderBook);

      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        // 1. subscribe to the topic
        // 2. wait for the first message and send request to snapshot
        // 3. store all incoming updates in an array
        // 4. when the snapshot is received, apply all updates and send the order book to the callback
        // 5. then on each update, apply it to the order book and send it to the callback
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        this.topics[topic] = (this.topics[topic] || 0) + 1;

        const payload = { method: 'SUBSCRIBE', params: [topic], id: 3 };
        this.send(JSON.stringify(payload));
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      resetLocalOrderBook();

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      const refCount = this.topics[topic];
      if (refCount !== undefined) {
        if (refCount <= 1) delete this.topics[topic];
        else this.topics[topic] = refCount - 1;
      }

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = { method: 'UNSUBSCRIBE', params: [topic], id: 3 };
        this.cleanupMessageHandlerStats(handler);
        this.send(JSON.stringify(payload));
      }
    };
  };

  dispose() {
    this.marketWebsocket.dispose();
    super.dispose();
  }
}
