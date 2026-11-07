import type { BitrueExchange } from './bitrue.exchange';

import dayjs from 'dayjs';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Candle, OHLCVOptions, OrderBook, Ticker, Timeframe } from '../../../types';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { readBlob } from '../../../utils/readBlob';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, INTERVAL, ORDER_BOOK_WS_UPDATE, TICKER_WS_UPDATE } from './bitrue.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

type SubscribedTopics = {
  [id: string]: number;
};

export class BitruePublicWebsocket extends BaseWebSocket<BitrueExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {};
  private tickerSymbols = new Set<string>();

  resolveRequests: Record<string, ((data: any) => void)[]> = {};

  private resolveMarketSymbol = (symbol: string) => {
    if (this.parent.store.marketsBySymbol[symbol]) {
      return symbol;
    }
    const normalized = symbol
      .replace(/^E[-_]/i, '')
      .replace(/[-_/]/g, '')
      .toUpperCase();
    const found = this.parent.store.markets.find((m) => {
      const candidate = m.symbol
        .replace(/^E[-_]/i, '')
        .replace(/[-_/]/g, '')
        .toUpperCase();
      return candidate === normalized;
    });
    return found?.symbol || symbol;
  };

  convertToWsSymbol = (symbol: string) => {
    // Supports both Bitrue internal symbols (E-BTC-USDT) and canonical symbols (BTCUSDT).
    const normalized = symbol
      .replace(/^E[-_]/i, '')
      .replace(/[-_/]/g, '')
      .toLowerCase();
    return `e_${normalized}`;
  };

  private ensureTickerSubscription = (symbol: string) => {
    if (this.tickerSymbols.has(symbol)) return;

    const topic = `market_${this.convertToWsSymbol(symbol)}_ticker`;
    this.tickerSymbols.add(symbol);
    this.topics[topic] = 1;
    this.messageHandlers[topic] = [(json: Data) => this.handleTickersEvent(json, symbol)];
  };

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      this.parent.store.markets.forEach((m) => {
        this.ensureTickerSubscription(m.symbol);
      });

      this.ws = this.createWebsocket(BASE_WS_URL.public + '?requestId=' + nanoid());

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.subscribe();
      this.ping();

      // Subscribe to new markets dynamically when market list is refreshed
      this.parent.emitter.off('markets:changed', this.onMarketsChanged);
      this.parent.emitter.on('markets:changed', this.onMarketsChanged);
    }
  };

  private onMarketsChanged = ({ added }: { added: Array<{ symbol: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    for (const m of added) {
      this.ensureTickerSubscription(m.symbol);
      const topic = `market_${this.convertToWsSymbol(m.symbol)}_ticker`;

      if (this.isConnected) {
        const payload = { event: 'sub', params: { channel: topic, cb_id: nanoid() } };
        this.send(JSON.stringify(payload));
      }
    }
  };

  // reverse uno ping pong
  handlePingEvent = (json?: Data) => {
    this.pingAt = performance.now();
    this.send('pong');
    if (json?.ping !== undefined) {
      this.send(JSON.stringify({ pong: json.ping }));
    }
    this.handlePongEvent(20_000);
  };

  // noop bc reversed
  ping = () => {};

  subscribe = () => {
    // {"event":"sub","params":{"channel":"market_e_chrusdt_ticker","cb_id":"e_chrusdt"}}
    Object.keys(this.topics).forEach((topic) => {
      const payload = { event: 'sub', params: { channel: topic, cb_id: nanoid() } };
      this.send(JSON.stringify(payload));
    });
  };

  formatKlineTopic = (symbol: string, interval: Timeframe) => {
    return `market_${this.convertToWsSymbol(symbol)}_kline_${INTERVAL[interval]}`;
  };

  mapKline = (json: Data, symbol?: string) => {
    const multiplier = symbol ? Number(this.parent.store.marketsBySymbol[this.resolveMarketSymbol(symbol)]?.info?.multiplier || 1) : 1;
    const rawCandles: {
      amount: number;
      close: number;
      ds: string;
      high: number;
      id: number;
      low: number;
      open: number;
      tradeId: number;
      vol: number;
    }[] = json as any;
    return rawCandles.map((candle) => ({
      timestamp:
        dayjs(candle.ds)
          // candles are returned in SG timezone
          .utcOffset(8 * 60, true)
          .valueOf() / 1000,
      open: candle.open,
      high: candle.high,
      low: candle.low,
      close: candle.close,
      volume: candle.vol * multiplier,
      info: candle,
    }));
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = this.formatKlineTopic(opts.symbol, opts.interval);
    const handler = topic;

    const parser = (json: Data) => {
      const candle = this.mapKline([json], opts.symbol);
      candle.forEach(callback);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];

        this.topics[topic] = (this.topics[topic] || 0) + 1;

        this.send(JSON.stringify({ event: 'sub', params: { channel: topic, cb_id: nanoid() } }));
        //this.parent.log(`Switched to [${opts.symbol}:${opts.interval}]`);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];

      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler].length) {
        this.cleanupMessageHandlerStats(handler);
        this.send(JSON.stringify({ event: 'unsub', params: { channel: topic, cb_id: nanoid() } }));
      }
    };
  };

  onMessage = async ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      let strData = data;
      if (data instanceof Blob) {
        strData = await readBlob(data);
        if (this.isDisposed) return;
      }
      const json = jsonParse(strData);
      if (!json) {
        if (strData) {
          console.log('Unknown data', strData);
        }
        return;
      }
      // {"event_rep":"rep","channel":"market_e_wifusdt_kline_30min","data":[{"amount":10423.6281,"close":2.8494,"ds":"2024-06-08
      if (json.ping !== undefined || json.event === 'ping') {
        this.handlePingEvent(json);
      } else if (json.event_rep === 'rep') {
        if (this.resolveRequests[json.channel]) {
          this.resolveRequests[json.channel].forEach((resolve) => resolve(json));
          this.resolveRequests[json.channel] = [];
        }
      } else {
        // console.log(json);
        const handlers = this.messageHandlers[json.channel];
        if (handlers) {
          handlers.forEach((handler) => handler(json.tick || json.data));
        }
      }
    }
  };

  handleTickersEvent = (_json: Record<string, any>, symbol: string) => {
    this.recordMessageHandlerStats('ticker');
    const json: TICKER_WS_UPDATE = _json as any;
    const resolvedSymbol = this.resolveMarketSymbol(symbol);
    const last = +json.close;
    const multiplier = Number(this.parent.store.marketsBySymbol[resolvedSymbol]?.info?.multiplier || 1);

    const ticker: Partial<Ticker> = {
      id: resolvedSymbol,
      symbol: resolvedSymbol,
      last,
      mark: last,
      percentage: +json.rose * 100,
      volume: +json.vol * multiplier,
      quoteVolume: +json.amount * multiplier,
      info: json,
    };
    this.parent.store.updateTicker({ id: resolvedSymbol }, ticker);
  };

  makeRequest = (topic: string): any => {
    return new Promise((resolve, reject) => {
      const rejectTimeout = setTimeout(() => {
        this.resolveRequests[topic] = [];
        reject(new Error('Request timeout'));
      }, 5000);
      const resolveHandler = (data: any) => {
        clearTimeout(rejectTimeout);
        resolve(data.data);
      };
      if (!this.resolveRequests[topic]) {
        this.resolveRequests[topic] = [];
      }
      this.resolveRequests[topic].push(resolveHandler);
      this.send(JSON.stringify({ event: 'req', params: { channel: topic, cb_id: nanoid() } }));
    });
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;
    let fallbackSubscribeTimeoutId: NodeJS.Timeout | null = null;
    const resolvedSymbol = this.resolveMarketSymbol(symbol);
    let receivedAnyUpdate = false;
    const primaryTopic = `market_${this.convertToWsSymbol(resolvedSymbol)}_simple_depth_step0`;
    const fallbackTopic = `market_${this.convertToWsSymbol(resolvedSymbol)}_depth_step0`;
    const subscribedTopics = new Set<string>();
    const parserByTopic: Record<string, (json: Data) => void> = {};
    const orderBook: OrderBook = { bids: [], asks: [] };

    const parser = (topic: string, _data: Data) => {
      receivedAnyUpdate = true;
      this.recordMessageHandlerStats(topic);
      const data = _data as ORDER_BOOK_WS_UPDATE;
      const buys = (data.buys || (data as any).bids || []) as Array<[number, number]>;
      const asks = (data.asks || []) as Array<[number, number]>;
      const multiplier = Number(this.parent.store.marketsBySymbol[resolvedSymbol]?.info?.multiplier || 1);
      orderBook.bids = buys.map(([price, amount]) => ({
        price: price,
        amount: amount * multiplier,
        total: 0,
      }));

      orderBook.asks = asks.map(([price, amount]) => ({
        price: price,
        amount: amount * multiplier,
        total: 0,
      }));

      calcOrderBookTotal(orderBook);
      const tickerUpdate: Partial<Ticker> = {
        bid: orderBook.bids[0]?.price,
        ask: orderBook.asks[0]?.price,
      };
      this.parent.store.updateTicker({ id: resolvedSymbol }, tickerUpdate);

      callback(orderBook);
    };

    const subscribeTopic = (topic: string) => {
      const handler = (json: Data) => parser(topic, json);
      parserByTopic[topic] = handler;
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), handler];
      this.topics[topic] = (this.topics[topic] || 0) + 1;
      subscribedTopics.add(topic);
      this.send(JSON.stringify({ event: 'sub', params: { channel: topic, cb_id: nanoid() } }));
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        subscribeTopic(primaryTopic);
        fallbackSubscribeTimeoutId = setTimeout(() => {
          if (!receivedAnyUpdate && this.isConnected && !subscribedTopics.has(fallbackTopic)) {
            subscribeTopic(fallbackTopic);
          }
        }, 3000);

        // clean this up so that last fallbacks work and dont catch on stale bid / asks for touch prices
        const tickerUpdate: Partial<Ticker> = {
          bid: 0,
          ask: 0,
        };
        this.parent.store.updateTicker({ id: resolvedSymbol }, tickerUpdate);
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      subscribedTopics.forEach((topic) => {
        const handler = parserByTopic[topic];
        this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== handler)];
        delete parserByTopic[topic];
      });
      orderBook.asks = [];
      orderBook.bids = [];

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
      if (fallbackSubscribeTimeoutId) {
        clearTimeout(fallbackSubscribeTimeoutId);
        fallbackSubscribeTimeoutId = null;
      }

      subscribedTopics.forEach((topic) => {
        this.topics[topic] = Math.max((this.topics[topic] || 1) - 1, 0);
        if (this.isConnected && !this.messageHandlers[topic]?.length) {
          this.cleanupMessageHandlerStats(topic);
          this.send(JSON.stringify({ event: 'unsub', params: { channel: topic, cb_id: nanoid() } }));
        }
      });
    };
  };
}
