import type { BybitSpotExchange } from './bybitspot.exchange';

import chunk from 'lodash/chunk';
import flatten from 'lodash/flatten';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { sleep } from '@tealstreet/sleep/src/sleep';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { resolveTransportRouting } from '../../../transport/registry';
import { WsDirectFallback } from '../../../utils/wsDirectFallback';
import {
  Candle,
  OHLCVOptions,
  OHLCVSOptions,
  OrderBook,
  Ticker,
  Timeframe,
  Writable,
} from '../../../types';
import { createLiveCandleStart } from '../../../utils/calendarCandles';
import { inverseObj } from '../../../utils/inverseObj';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { updateOrderBookByPrice } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { getBybitWebSocketOptions } from '../../../exchanges/bybit/bybit.site';
import { INTERVAL } from '../../../exchanges/bybit/bybit.types';
import { BASE_WS_URL } from './bybitspot.types';

// Module-level cache. `inverseObj` runs `Object.entries` + a quadratic
// spread-reduce; doing it per kline frame inside the listenOHLCVs
// dispatcher allocated a fresh map every call.
const INTERVAL_INVERSE = inverseObj(INTERVAL) as Record<string, Timeframe>;
// Length of the leading "kline." segment in topics.
const KLINE_TOPIC_PREFIX_LEN = 6;

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

type SubscribedTopics = {
  [id: string]: string[] | string;
};

export class BybitPublicWebsocket extends BaseWebSocket<BybitSpotExchange> {
  private directFallback = new WsDirectFallback();

  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {
    tickers: [(d: Data) => this.handleTickersEvent(d)],
    pong: [() => this.handlePongEvent()],
  };
  private handleActiveTickerSymbolsChanged = () => {
    if (this.isDisposed) {
      return;
    }

    const prevTopics = new Set(this.getCurrentTickerTopics());
    const nextTopics = this.syncTickerTopics();
    const topicsToSubscribe = nextTopics.filter((topic) => !prevTopics.has(topic));

    if (this.isConnected && topicsToSubscribe.length) {
      for (const chunkTopics of chunk(topicsToSubscribe, 10)) {
        const payload = { op: 'subscribe', args: chunkTopics };
        this.send(JSON.stringify(payload));
      }
    }
  };

  constructor(parent: BybitSpotExchange) {
    super(parent);
    this.parent.on('activeTickerSymbols:changed', this.handleActiveTickerSymbolsChanged);
  }

  private getCurrentTickerTopics() {
    const current = this.topics.tickers;
    if (!current) return [];
    return Array.isArray(current) ? current : [current];
  }

  private getDesiredTickerTopics() {
    return this.parent.getTickerSymbolsForPublicStream().map((symbol) => `tickers.${symbol}`);
  }

  private syncTickerTopics() {
    const merged = [...new Set([...this.getCurrentTickerTopics(), ...this.getDesiredTickerTopics()])];
    this.topics.tickers = merged;
    return merged;
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      const route = resolveTransportRouting(this.parent.name, this.parent.options).wsPublic;
      const directUrl = BASE_WS_URL.public['spot'][this.parent.options.testnet ? 'testnet' : 'livenet'];
      const baseUrl = this.directFallback.resolveUrl(route.tunnelled && route.host ? route.host : directUrl, route.fallback);
      if (baseUrl === route.fallback) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet bybit public spot proxy');
        } catch (_e) {}
      }
      const webSocketOptions = this.parent.isDemo ? undefined : getBybitWebSocketOptions(this.parent.options);
      this.ws = this.createWebsocket(
        baseUrl + '?requestId=' + nanoid(),
        undefined,
        webSocketOptions,
      );

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.syncTickerTopics();

      this.subscribe();
      this.ping();

      // Subscribe to new markets dynamically when market list is refreshed
      this.parent.emitter.off('markets:changed', this.onMarketsChanged);
      this.parent.emitter.on('markets:changed', this.onMarketsChanged);

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  private onMarketsChanged = async ({ added }: { added: Array<{ symbol: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    const newTopics = added.map((m) => `tickers.${m.symbol}`);
    this.topics.tickers = [...this.getCurrentTickerTopics(), ...newTopics];

    for (const chunkTopics of chunk(newTopics, 10)) {
      this.send(JSON.stringify({ op: 'subscribe', args: chunkTopics }));
      await sleep(5);
    }
  };

  override dispose() {
    this.parent.off('activeTickerSymbols:changed', this.handleActiveTickerSymbolsChanged);
    super.dispose();
  }

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ op: 'ping' }));
    }
  };

  subscribe = async () => {
    const topics = flatten(Object.values(this.topics));

    // need to back off here or bybit closes
    for (const chunkTopics of chunk(topics, 10)) {
      const payload = { op: 'subscribe', args: chunkTopics };
      this.send(JSON.stringify(payload));
      await sleep(10);
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();
    this.directFallback.onMessage();

    // Extract the "topic" field once via targeted indexOf instead of
    // O(N) substring scans per registered handler. Bybit V5 frames always
    // include `"topic":"<full topic>"` for data messages.
    const topicIdx = data.indexOf('"topic":"');
    if (topicIdx >= 0) {
      const start = topicIdx + 9;
      const end = data.indexOf('"', start);
      if (end <= start) return;
      const fullTopic = data.slice(start, end);

      // Exact-match handler (e.g. "orderbook.200.BTCUSDT", "kline.1.BTCUSDT")
      const exact = this.messageHandlers[fullTopic];

      // Prefix handler (e.g. "tickers" matches "tickers.BTCUSDT", "kline" matches "kline.1.BTCUSDT")
      let prefix: typeof exact | undefined;
      const dotIdx = fullTopic.indexOf('.');
      if (dotIdx > 0) {
        prefix = this.messageHandlers[fullTopic.slice(0, dotIdx)];
      }

      const hasExact = exact && exact.length > 0;
      const hasPrefix = prefix && prefix.length > 0;
      if (!hasExact && !hasPrefix) return;

      // Record stats under the key that actually matched — prefix key when
      // only a prefix handler fired. See linear ws-public for rationale.
      this.recordMessageHandlerStats(hasExact ? fullTopic : fullTopic.slice(0, dotIdx));
      const json = jsonParse(data);
      if (!json) return;
      // Indexed for-loop — see linear ws-public for rationale.
      if (hasExact) {
        const exactArr = exact!;
        for (let i = 0; i < exactArr.length; i++) exactArr[i](json);
      }
      if (hasPrefix) {
        const prefixArr = prefix!;
        for (let i = 0; i < prefixArr.length; i++) prefixArr[i](json);
      }
      return;
    }

    // No `topic` field — pong / sub ack / op echo. Dispatch only if a pong
    // handler is registered.
    if (data.includes('pong')) {
      const pongHandlers = this.messageHandlers.pong;
      if (pongHandlers && pongHandlers.length > 0) {
        this.recordMessageHandlerStats('pong');
        for (let i = 0; i < pongHandlers.length; i++) pongHandlers[i]({});
      }
    }
  };

  handleTickersEvent = (json: Record<string, any>) => {
    const d = json.data;
    if (!d?.symbol) return;
    // O(1) lookup. Skip parseFloat work and update allocation when an
    // exchange pushes a ticker for a symbol we don't track.
    const ticker = this.parent.store.tickersBySymbol[d.symbol];
    if (!ticker) return;

    const update: Partial<Writable<Ticker>> = {};

    if (d.bid1Price) update.bid = parseFloat(d.bid1Price);
    if (d.ask1Price) update.ask = parseFloat(d.ask1Price);
    if (d.lastPrice) update.last = parseFloat(d.lastPrice);
    if (d.markPrice) update.mark = parseFloat(d.markPrice);
    if (d.indexPrice) update.index = parseFloat(d.indexPrice);

    if (d.price24hPcnt) {
      update.percentage = parseFloat(d.price24hPcnt) * 100;
    }

    if (d.openInterest) {
      update.openInterest = parseFloat(d.openInterest);
    }

    if (d.fundingRate) {
      update.fundingRate = 0;
    }

    // if (d.nextFundingTime) {
    //   update.fundingTime = parseFloat(d.nextFundingTime);
    // }

    if (d.volume24h) {
      update.volume = parseFloat(d.volume24h);
    }

    if (d.turnover24h) {
      update.quoteVolume = parseFloat(d.turnover24h);
    }

    this.parent.store.updateTicker(ticker, update);

  };

  listenOHLCVs = (
    opts: OHLCVSOptions,
    callback: (candle: Candle & { symbol: string; interval: Timeframe }) => void,
  ) => {
    const topics: string[] = [];

    for (const symbol of opts.symbols) {
      const topic = `kline.${INTERVAL[opts.interval]}.${symbol}`;
      topics.push(topic);
      this.topics[topic] = topic;
    }

    this.messageHandlers['kline'] = [
      ({ data: [candle], topic }: Data) => {
        // Topic format: "kline.{interval}.{symbol}". Slice without
        // allocating a 3-element array via `split`.
        const secondDot = topic.indexOf('.', KLINE_TOPIC_PREFIX_LEN);
        const interval = topic.slice(KLINE_TOPIC_PREFIX_LEN, secondDot);
        const symbol = topic.slice(secondDot + 1);
        const lastCandle = {
          timestamp: candle.start / 1000,
          open: parseFloat(candle.open),
          high: parseFloat(candle.high),
          low: parseFloat(candle.low),
          close: parseFloat(candle.close),
          volume: parseFloat(candle.volume),
          symbol,
          interval: INTERVAL_INVERSE[interval],
        };
        callback(lastCandle);
      },
    ];

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          for (const chunkTopics of chunk(topics, 10)) {
            const payload = { op: 'subscribe', args: chunkTopics };
            this.send(JSON.stringify(payload));
          }
        }
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = `kline.${INTERVAL[opts.interval]}.${opts.symbol}`;
    const secondaryTopic = `publicTrade.${opts.symbol}`;

    // Subscription-invariant: built once instead of recomputed per trade.
    // Monday weeks / calendar months for `1w` / `1M`, like the kline it extends.
    const candleStartOf = createLiveCandleStart(opts.interval);

    let lastCandle: Candle | undefined = undefined;

    const parser = ({ data: [candle] }: Data) => {
      const timestamp = candle.start / 1000;
      if (lastCandle && lastCandle.timestamp > timestamp) {
        // prevent bleeding candles backwards
        // console.warn('Received candle with timestamp in the past, ignoring:', candle);
        return;
      }
      const high = parseFloat(candle.high);
      const low = parseFloat(candle.low);
      const close = parseFloat(candle.close);
      const volume = parseFloat(candle.volume);
      if (!lastCandle || lastCandle.timestamp < timestamp) {
        lastCandle = {
          timestamp,
          open: parseFloat(candle.open),
          high,
          low,
          close,
          volume,
        };
      } else {
        lastCandle = {
          ...lastCandle,
          high: lastCandle.high > high ? lastCandle.high : high,
          low: lastCandle.low < low ? lastCandle.low : low,
          close,
          volume,
        };
      }
      callback(lastCandle);
    };

    const secondaryParser = ({ data }: Data) => {
      const trade = data[0];
      const T = trade.T;
      const p = trade.p;
      const v = trade.v;

      // T is in ms.
      const candleStart = candleStartOf(T / 1000);
      if (!lastCandle) return;
      // Numeric `price` once. Previous code aliased the raw string `p` as
      // `price`, relying on Math.max/min coercion — left fields briefly as
      // strings and skipped any chance to skip-update on equal values.
      const price = parseFloat(p);
      if (lastCandle.timestamp > candleStart) {
        // prevent bleeding candles backwards
        console.warn('Received trade with timestamp in the past, ignoring:', T);
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
      // Plain `+` over `safe-math.add` — chart-volume accumulation doesn't
      // need decimal-aware rounding.
      lastCandle = {
        ...lastCandle,
        high: lastCandle.high > price ? lastCandle.high : price,
        low: lastCandle.low < price ? lastCandle.low : price,
        close: price,
        volume: lastCandle.volume + v / price,
      };
      callback(lastCandle);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];
          const payload = { op: 'subscribe', args: [topic] };
          this.send(JSON.stringify(payload));
          this.topics[topic] = topic;

          if (!this.parent.options.publicDataOnly) {
            this.messageHandlers[secondaryTopic] = [...(this.messageHandlers[secondaryTopic] || []), secondaryParser];
            const secondaryPayload = { op: 'subscribe', args: [secondaryTopic] };
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
        const payload = { op: 'unsubscribe', args: [topic] };
        this.send(JSON.stringify(payload));
      }

      if (this.isConnected && !this.messageHandlers[secondaryTopic].length) {
        const payload = { op: 'unsubscribe', args: [secondaryTopic] };
        this.cleanupMessageHandlerStats(topic);
        this.cleanupMessageHandlerStats(secondaryTopic);
        this.send(JSON.stringify(payload));
      }
    };
  };

  listenOHLCVOld = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = `kline.${INTERVAL[opts.interval]}.${opts.symbol}`;

    const parser = ({ data: [candle] }: Data) => {
      callback({
        timestamp: candle.start / 1000,
        open: parseFloat(candle.open),
        high: parseFloat(candle.high),
        low: parseFloat(candle.low),
        close: parseFloat(candle.close),
        volume: parseFloat(candle.volume),
      });
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];

          const payload = { op: 'subscribe', args: [topic] };
          this.send(JSON.stringify(payload));
          //this.parent.log(`Switched to [${opts.symbol}:${opts.interval}]`);

          // store subscribed topic to re-subscribe on reconnect
          this.topics[topic] = topic;
        }
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];

      if (!this.messageHandlers[topic].length) {
        delete this.topics[topic];
      }

      if (this.isConnected && !this.messageHandlers[topic].length) {
        const payload = { op: 'unsubscribe', args: [topic] };
        this.send(JSON.stringify(payload));
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    const topic = `orderbook.200.${symbol}`;
    const orderBook: OrderBook = { bids: [], asks: [] };
    let lastUpdateId = 0;
    let isResyncing = false;

    const resync = () => {
      if (isResyncing) return;
      isResyncing = true;
      lastUpdateId = 0;
      orderBook.bids = [];
      orderBook.asks = [];

      // Re-subscribe to get a fresh snapshot
      this.send(JSON.stringify({ op: 'unsubscribe', args: [topic] }));
      setTimeout(() => {
        if (!this.isDisposed) {
          this.send(JSON.stringify({ op: 'subscribe', args: [topic] }));
        }
        isResyncing = false;
      }, 100);
    };

    const parser = (data: Data) => {
      const d = data.data;

      if (data.type === 'snapshot') {
        orderBook.bids = [];
        orderBook.asks = [];
        lastUpdateId = d?.u || 0;

        const asks = d?.a;
        if (asks) {
          for (let i = 0; i < asks.length; i++) {
            const o = asks[i];
            orderBook.asks.push({
              price: parseFloat(o[0]),
              amount: parseFloat(o[1]),
              total: 0,
            });
          }
        }

        const bids = d?.b;
        if (bids) {
          for (let i = 0; i < bids.length; i++) {
            const o = bids[i];
            orderBook.bids.push({
              price: parseFloat(o[0]),
              amount: parseFloat(o[1]),
              total: 0,
            });
          }
        }
      }

      if (data.type === 'delta') {
        const updateId = d?.u || 0;

        // Detect sequence gap — resync if we missed any deltas
        if (lastUpdateId > 0 && updateId > lastUpdateId + 1) {
          resync();
          return;
        }

        lastUpdateId = updateId;

        const asks = d?.a;
        if (asks) {
          for (let i = 0; i < asks.length; i++) {
            const o = asks[i];
            updateOrderBookByPrice(orderBook, 'asks', parseFloat(o[0]), parseFloat(o[1]));
          }
        }

        const bids = d?.b;
        if (bids) {
          for (let i = 0; i < bids.length; i++) {
            const o = bids[i];
            updateOrderBookByPrice(orderBook, 'bids', parseFloat(o[0]), parseFloat(o[1]));
          }
        }
      }

      calcOrderBookTotal(orderBook);

      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];

          const payload = { op: 'subscribe', args: [topic] };
          this.send(JSON.stringify(payload));

          // store subscribed topic to re-subscribe on reconnect
          this.topics[topic] = topic;
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

      this.cleanupMessageHandlerStats(topic);

      orderBook.asks = [];
      orderBook.bids = [];

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (this.isConnected && !this.messageHandlers[topic].length) {
        const payload = { op: 'unsubscribe', args: [topic] };
        this.send(JSON.stringify(payload));
      }
    };
  };
}
