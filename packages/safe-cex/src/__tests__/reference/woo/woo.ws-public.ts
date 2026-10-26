import type { Candle, OHLCVOptions, OrderBook } from '../../../types';
import type { WOOXExchange } from './woo.exchange';

import flatten from 'lodash/flatten';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { getCandleStartTime } from '../../../utils/getCandleStartTime';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { add } from '../../../utils/safe-math';
import { updateOrderBookByPrice } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ENDPOINTS } from './woo.types';
import { normalizeSymbol, reverseSymbol } from './woo.utils';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};
type SubscribedTopics = {
  [id: string]: string;
};

export class WooPublicWebsocket extends BaseWebSocket<WOOXExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePingEvent()],
    pong: [() => this.handlePongEvent()],
    tickers: [({ data }: Data) => this.handleTickersStreamEvents(data)],
    bbos: [({ data }: Data) => this.handleBBOStreamEvents(data)],
    markprices: [({ data }: Data) => this.handleMarkPricesStreamEvents(data)],
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const baseURL = BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet'];

      this.ws = this.createWebsocket(`${baseURL}${this.parent.options.applicationId}`);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.ping();
      this.send(JSON.stringify({ event: 'subscribe', topic: 'tickers' }));
      this.send(JSON.stringify({ event: 'subscribe', topic: 'bbos' }));
      this.send(JSON.stringify({ event: 'subscribe', topic: 'markprices' }));
      this.resetOrderbook();
      this.subscribe();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  subscribe = () => {
    const topics = flatten(Object.values(this.topics));
    for (const topic of topics) {
      const payload = { event: 'subscribe', topic };
      this.send(JSON.stringify(payload));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      const json = jsonParse<Data>(data);
      if (!json) return;

      let handlerKey = '';
      if (typeof json.event === 'string') {
        // Legacy WOO control frames use event only for ping/pong and
        // subscribe/unsubscribe acknowledgements. Control always wins over a
        // stray payload topic so acknowledgements cannot reach data handlers.
        if (json.event !== 'ping' && json.event !== 'pong') return;
        handlerKey = json.event;
      } else if (typeof json.topic === 'string') {
        handlerKey = json.topic;
      }

      const handlers = this.messageHandlers[handlerKey];
      if (!handlers?.length) return;

      this.recordMessageHandlerStats(handlerKey);
      for (const callback of handlers) {
        callback(json);
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ event: 'ping' }));
    }
  };

  handlePingEvent = () => {
    this.send(JSON.stringify({ event: 'pong' }));
  };

  handleTickersStreamEvents = (data: Array<Record<string, any>>) => {
    data.forEach((row) => {
      if (row.symbol.startsWith('PERP_')) {
        const symbol = normalizeSymbol(row.symbol);
        const ticker = this.parent.store.tickersBySymbol[symbol];

        if (ticker) {
          this.store.updateTicker(ticker, {
            last: row.close,
            quoteVolume: row.amount,
            volume: row.volume,
          });
        }
      }
    });
  };

  handleBBOStreamEvents = (data: Array<Record<string, any>>) => {
    data.forEach((row) => {
      if (row.symbol.startsWith('PERP_')) {
        const symbol = normalizeSymbol(row.symbol);
        const ticker = this.parent.store.tickersBySymbol[symbol];

        if (ticker) {
          this.store.updateTicker(ticker, {
            bid: row.bid,
            ask: row.ask,
          });
        }
      }
    });
  };

  handleMarkPricesStreamEvents = (data: Array<Record<string, any>>) => {
    data.forEach((row) => {
      if (row.symbol.startsWith('PERP_')) {
        const symbol = normalizeSymbol(row.symbol);
        const ticker = this.parent.store.tickersBySymbol[symbol];

        if (ticker) {
          this.store.updateTicker(ticker, {
            mark: row.price,
          });
        }
      }
    });
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = `${reverseSymbol(opts.symbol)}@kline_${opts.interval}`;
    const secondaryTopic = `${reverseSymbol(opts.symbol)}@trades`;
    let lastCandle: Candle | undefined = undefined;

    const parser = (json: Data) => {
      const timestamp = json.data.startTime / 1000;

      if (lastCandle && lastCandle.timestamp > timestamp) {
        // prevent bleeding candles backwards
        // console.warn('Received candle with timestamp in the past, ignoring:', json);
        return;
      }

      if (!lastCandle || lastCandle.timestamp < timestamp) {
        lastCandle = {
          timestamp,
          open: json.data.open,
          high: json.data.high,
          low: json.data.low,
          close: json.data.close,
          volume: json.data.volume,
        };
      } else {
        lastCandle = {
          ...lastCandle,
          high: Math.max(lastCandle.high, parseFloat(json.data.high)),
          low: Math.min(lastCandle.low, parseFloat(json.data.low)),
          close: json.data.close,
          volume: json.data.volume,
        };
      }

      callback(lastCandle);
    };

    // {
    //   "topic": "PERP_BTC_USDT@trades",
    //   "ts": 1737684439784,
    //   "data": [
    //   {
    //     "s": "PERP_BTC_USDT",
    //     "p": 103284,
    //     "a": 0.058,
    //     "b": "BUY",
    //     "c": 0
    //   }
    // ]
    // }
    const secondaryParser = (json: Data) => {
      const timestamp = json.ts / 1000;
      const data = json.data[0];
      if (!data) return;
      const price = parseFloat(data.p);
      const amount = parseFloat(data.a);

      const candleStart = getCandleStartTime(opts.interval, timestamp);

      const isSeconds = opts.interval.endsWith('s');
      if (isSeconds && (!lastCandle || lastCandle.timestamp !== candleStart)) {
        lastCandle = {
          timestamp: candleStart,
          open: lastCandle?.close || price,
          high: price,
          low: price,
          close: price,
          volume: 0,
        };
      }

      if (!lastCandle) return;

      if (lastCandle.timestamp > candleStart) {
        // prevent bleeding candles backwards
        console.warn('Received trade with timestamp in the past, ignoring:', json);
        return;
      }

      if (lastCandle.timestamp < candleStart) {
        lastCandle = {
          timestamp: candleStart,
          open: lastCandle.close,
          high: lastCandle.close,
          low: lastCandle.close,
          close: lastCandle.close,
          volume: 0,
        };
      }

      lastCandle = {
        ...lastCandle,
        high: Math.max(lastCandle.high, price),
        low: Math.min(lastCandle.low, price),
        close: price,
        volume: add(lastCandle.volume, amount),
      };
      callback(lastCandle);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];
          const payload = { event: 'subscribe', topic };
          this.send(JSON.stringify(payload));
          this.topics[topic] = topic;

          if (!this.parent.options.publicDataOnly) {
            this.messageHandlers[secondaryTopic] = [...(this.messageHandlers[secondaryTopic] || []), secondaryParser];
            const secondaryPayload = { event: 'subscribe', topic: secondaryTopic };
            this.send(JSON.stringify(secondaryPayload));
            this.topics[secondaryTopic] = secondaryTopic;
          }
        }
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];
      this.messageHandlers[secondaryTopic] = [
        ...(this.messageHandlers[secondaryTopic] || []).filter((f) => f !== secondaryParser),
      ];

      if (!this.messageHandlers[topic].length) {
        delete this.topics[topic];
        delete this.topics[secondaryTopic];
      }

      if (this.isConnected && !this.messageHandlers[topic].length) {
        const payload = { event: 'unsubscribe', topic };
        this.cleanupMessageHandlerStats(topic);
        this.send(JSON.stringify(payload));
      }

      if (this.isConnected && !this.messageHandlers[secondaryTopic].length) {
        const payload = { event: 'unsubscribe', topic: secondaryTopic };
        this.cleanupMessageHandlerStats(secondaryTopic);
        this.send(JSON.stringify(payload));
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;
    let snapshotRetryId: NodeJS.Timeout | null = null;
    let isSnapshotLoading = false;
    const isOrderBookDelta = (update: Data) => Array.isArray(update?.data?.bids) && Array.isArray(update?.data?.asks);

    const topic = `${reverseSymbol(symbol)}@orderbookupdate`;
    const orderBook: OrderBook = { bids: [], asks: [] };
    const innerId = nanoid();

    if (!this.orderBookInnerState[innerId]) {
      this.orderBookInnerState[innerId] = {
        updates: [],
        isSnapshotLoaded: false,
      };
    }

    const innerState = this.orderBookInnerState[innerId];

    const fetchSnapshot = async () => {
      if (this.isDisposed || innerState.isSnapshotLoaded || isSnapshotLoading) return;

      isSnapshotLoading = true;
      try {
        const { data } = await this.parent.xhr.get(`${ENDPOINTS.ORDERBOOK}/${reverseSymbol(symbol)}`);

        if (!this.isDisposed) {
          orderBook.bids = data.bids.map((row: Record<string, any>) => ({
            price: row.price,
            amount: row.quantity,
            total: 0,
          }));

          orderBook.asks = data.asks.map((row: Record<string, any>) => ({
            price: row.price,
            amount: row.quantity,
            total: 0,
          }));

          // drop events where timestamp is older than the snapshot
          innerState.updates = innerState.updates.filter((update: Record<string, any>) => {
            if (!isOrderBookDelta(update)) return false;
            const updateTs = Number(update.ts);
            return Number.isFinite(updateTs) ? updateTs > data.timestamp : true;
          });

          // apply all updates
          innerState.updates.forEach((update: Record<string, any>) => {
            if (isOrderBookDelta(update)) {
              this.processOrderBookUpdate(orderBook, update);
            }
          });

          calcOrderBookTotal(orderBook);

          innerState.isSnapshotLoaded = true;
          innerState.updates = [];

          callback(orderBook);
        }
      } catch (_error) {
        // Retry snapshot fetch in case of transient API/network issues.
        if (!this.isDisposed && !innerState.isSnapshotLoaded && !snapshotRetryId) {
          snapshotRetryId = setTimeout(() => {
            snapshotRetryId = null;
            void fetchSnapshot();
          }, 1000);
        }
      } finally {
        isSnapshotLoading = false;
      }
    };

    const parser = (json: Data) => {
      if (!isOrderBookDelta(json)) {
        if (!innerState.isSnapshotLoaded) {
          void fetchSnapshot();
        }
        return;
      }

      if (!innerState.isSnapshotLoaded) {
        innerState.updates.push(json);
        void fetchSnapshot();
        return;
      }

      // do updates
      this.processOrderBookUpdate(orderBook, json);
      calcOrderBookTotal(orderBook);

      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        // 1. subscribe to the topic
        // 2. request snapshot immediately
        // 3. store all incoming updates in an array
        // 4. when the snapshot is received, apply all updates and send the order book to the callback
        // 5. then on each update, apply it to the order book and send it to the callback
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];
          this.topics[topic] = topic;

          const payload = { event: 'subscribe', topic };
          this.send(JSON.stringify(payload));

          // Request snapshot immediately so illiquid markets still produce an initial orderbook.
          void fetchSnapshot();
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];

      if (!this.messageHandlers[topic].length) {
        delete this.topics[topic];
      }

      orderBook.asks = [];
      orderBook.bids = [];
      innerState.updates = [];
      innerState.isSnapshotLoaded = false;

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (snapshotRetryId) {
        clearTimeout(snapshotRetryId);
        snapshotRetryId = null;
      }

      if (this.isConnected && !this.messageHandlers[topic].length) {
        const payload = { event: 'unsubscribe', topic };
        this.cleanupMessageHandlerStats(topic);
        this.send(JSON.stringify(payload));
      }
    };
  };

  private processOrderBookUpdate = (orderBook: OrderBook, update: Record<string, any>) => {
    const bids = update.data.bids;
    if (Array.isArray(bids)) {
      for (let i = 0; i < bids.length; i++) {
        updateOrderBookByPrice(orderBook, 'bids', bids[i][0], bids[i][1]);
      }
    }

    const asks = update.data.asks;
    if (Array.isArray(asks)) {
      for (let i = 0; i < asks.length; i++) {
        updateOrderBookByPrice(orderBook, 'asks', asks[i][0], asks[i][1]);
      }
    }
  };
}
