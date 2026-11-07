import type { Candle, OHLCVOptions, OrderBook } from '../../../types';
import type { BinanceInverseExchange } from './binanceinverse.exchange';

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
import { BASE_WS_URL, ENDPOINTS } from '../../../exchanges/binance/binanceinverse.types';

type Data = Array<Record<string, any>>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class BinanceInversePublicWebsocket extends BaseWebSocket<BinanceInverseExchange> {
  messageHandlers: MessageHandlers = {
    '24hrTicker': [(d: Data) => this.handleTickerStreamEvents(d)],
    markPriceUpdate: [(d: Data) => this.handleMarkPriceStreamEvents(d)],
    bookTicker: [(d: Data) => this.handleBookTickersStreamEvents(d)],
  };

  topics: { [key: string]: number } = {};
  private nextSubscriptionId = 2;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(BASE_WS_URL[this.parent.options.testnet ? 'testnet' : 'livenet']);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      const params = this.parent.options.publicDataOnly
        ? ['!ticker@arr', '!markPrice@arr@1s']
        : ['!ticker@arr', '!markPrice@arr@1s', '!bookTicker'];
      this.send(JSON.stringify({ id: 1, method: 'SUBSCRIBE', params }));
      this.resetOrderbook();
      this.subscribe();
    }
  };

  subscribe = () => {
    for (const topic of Object.keys(this.topics)) {
      if (!Number.isFinite(this.topics[topic]) || this.topics[topic] <= 0) continue;
      this.send(JSON.stringify({ method: 'SUBSCRIBE', params: [topic], id: this.nextSubscriptionId++ }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();
    dispatchBinanceMessage(this.messageHandlers, data, (topic) => this.recordMessageHandlerStats(topic));
  };

  handleTickerStreamEvents = (events: Array<Record<string, any>>) => {
    const tickersBySymbol = this.parent.store.tickersBySymbol;
    for (const event of events) {
      const ticker = tickersBySymbol[event.s];
      if (!ticker) continue;
      this.store.updateTicker(ticker, {
        last: parseFloat(v(event, 'c')),
        percentage: parseFloat(v(event, 'P')),
        volume: parseFloat(v(event, 'v')),
        quoteVolume: parseFloat(v(event, 'q') || v(event, 'Q') || '0'),
      });
    }
  };

  handleMarkPriceStreamEvents = (events: Array<Record<string, any>>) => {
    const tickersBySymbol = this.parent.store.tickersBySymbol;
    for (const event of events) {
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

  handleBookTickersStreamEvents = (events: Array<Record<string, any>>) => {
    const tickersBySymbol = this.parent.store.tickersBySymbol;
    for (const event of events) {
      const ticker = tickersBySymbol[event.s];
      if (!ticker) continue;
      this.store.updateTicker(ticker, {
        bid: parseFloat(v(event, 'b')),
        ask: parseFloat(v(event, 'a')),
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
      if (opts.symbol === json.k?.s && json.k?.i === opts.interval) {
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
        this.send(JSON.stringify({ method: 'SUBSCRIBE', params: [topic], id: this.nextSubscriptionId++ }));
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      if (isUnsubscribed) return;
      isUnsubscribed = true;
      isActive = false;
      if (timeoutId) clearTimeout(timeoutId);

      const nextHandlers = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      if (nextHandlers.length) this.messageHandlers[handler] = nextHandlers;
      else delete this.messageHandlers[handler];

      const nextTopicCount = (this.topics[topic] || 0) - 1;
      if (nextTopicCount > 0) this.topics[topic] = nextTopicCount;
      else delete this.topics[topic];

      if (this.isConnected && nextHandlers.length === 0) {
        this.cleanupMessageHandlerStats(handler);
        this.send(JSON.stringify({ method: 'UNSUBSCRIBE', params: [topic], id: this.nextSubscriptionId++ }));
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;
    const innerId = nanoid();
    this.orderBookInnerState[innerId] = { updates: [], isSnapshotLoaded: false };

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
        const { data } = await this.parent.publicXhr.get(ENDPOINTS.ORDERBOOK, { params: { symbol, limit: 1000 } });
        if (this.isDisposed) return;
        loadBinanceOrderBookSnapshot(orderBook, data, syncState);
        const bufferedUpdates = getBufferedBinanceDepthUpdates(innerState.updates, data.lastUpdateId);
        innerState.isSnapshotLoaded = true;
        innerState.updates = [];
        for (const update of bufferedUpdates) {
          const result = applyBinanceDepthUpdate(orderBook, update, syncState, { requirePreviousFinalUpdateId: true });
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
      if (data.s !== symbol) return;
      if (!innerState.isSnapshotLoaded && innerState.updates.length === 0) {
        resetLocalOrderBook(data);
        void fetchSnapshot();
        return;
      }
      if (!innerState.isSnapshotLoaded) {
        innerState.updates.push(data);
        return;
      }
      const result = applyBinanceDepthUpdate(orderBook, data, syncState, { requirePreviousFinalUpdateId: true });
      if (result === 'gap') {
        resetLocalOrderBook(data);
        void fetchSnapshot();
        return;
      }
      if (result === 'ignored') return;
      calcOrderBookTotal(orderBook);
      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.send(JSON.stringify({ method: 'SUBSCRIBE', params: [topic], id: this.nextSubscriptionId++ }));
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      resetLocalOrderBook();
      if (timeoutId) clearTimeout(timeoutId);

      const refCount = this.topics[topic];
      if (refCount !== undefined) {
        if (refCount <= 1) delete this.topics[topic];
        else this.topics[topic] = refCount - 1;
      }

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        this.send(JSON.stringify({ method: 'UNSUBSCRIBE', params: [topic], id: this.nextSubscriptionId++ }));
      }
    };
  };
}
