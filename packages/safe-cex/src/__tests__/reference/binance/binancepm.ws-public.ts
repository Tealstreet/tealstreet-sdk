import type { Candle, OHLCVOptions, OrderBook } from '../../../types';
import type { BinancePMExchange } from './binancepm.exchange';

import { nanoid } from 'nanoid';

import { v } from '../../../utils/get-key';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import {
  applyBinanceDepthUpdate,
  getBufferedBinanceDepthUpdates,
  loadBinanceOrderBookSnapshot,
  resetBinanceOrderBookSyncState,
} from './binance.orderbook';
import { dispatchBinanceMessage } from './binance.ws-public';
import { BASE_WS_URL, ENDPOINTS } from '../../../exchanges/binance/binancepm.types';

type Data = Array<Record<string, any>>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

/**
 * Binance Portfolio Margin market-data WebSocket (`/market/ws`).
 *
 * Carries regular market data (24hrTicker, markPriceUpdate, kline). Split out
 * from the high-frequency `/public/ws` connection on 2026-04-23 when Binance
 * retired the legacy `/ws` endpoint for USDⓈ-M futures (which PM consumes).
 */
export class BinancePMMarketWebsocket extends BaseWebSocket<BinancePMExchange> {
  messageHandlers: MessageHandlers = {
    '24hrTicker': [(d: Data) => this.handleTickerStreamEvents(d)],
    markPriceUpdate: [(d: Data) => this.handleMarkPriceStreamEvents(d)],
  };

  topics: { [key: string]: number } = {};

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
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        this.topics[topic] = (this.topics[topic] || 0) + 1;

        const payload = { method: 'SUBSCRIBE', params: [topic], id: 2 };
        this.send(JSON.stringify(payload));
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];

      const refCount = this.topics[topic];
      if (refCount !== undefined) {
        if (refCount <= 1) delete this.topics[topic];
        else this.topics[topic] = refCount - 1;
      }

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = { method: 'UNSUBSCRIBE', params: [topic], id: 2 };
        this.cleanupMessageHandlerStats(handler);
        this.send(JSON.stringify(payload));
      }
    };
  };
}

/**
 * Binance Portfolio Margin public WebSocket (`/public/ws`).
 *
 * Carries high-frequency public data: bookTicker, @depth. Owns a sibling
 * `BinancePMMarketWebsocket` for regular market data on `/market/ws` (the
 * 2026-04-23 URL split). The exchange continues to interact with a single
 * `publicWebsocket` reference; this class fans out to the two underlying sockets
 * transparently.
 */
export class BinancePMPublicWebsocket extends BaseWebSocket<BinancePMExchange> {
  messageHandlers: MessageHandlers = {
    bookTicker: [(d: Data) => this.handleBookTickersStreamEvents(d)],
  };

  topics: { [key: string]: number } = {};

  marketWebsocket: BinancePMMarketWebsocket;

  constructor(parent: BinancePMExchange) {
    super(parent);
    this.marketWebsocket = new BinancePMMarketWebsocket(parent);
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet']);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);

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
