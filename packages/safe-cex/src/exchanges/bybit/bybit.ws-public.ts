import type { BybitExchange } from './bybit.exchange';

import chunk from 'lodash/chunk';
import flatten from 'lodash/flatten';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { sleep } from '@tealstreet/sleep/src/sleep';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { resolveTransportRouting } from '../../transport/registry';
import { WsDirectFallback } from '../../utils/wsDirectFallback';
import {
  Candle,
  LogSeverity,
  OHLCVOptions,
  OHLCVSOptions,
  OrderBook,
  OrderSide,
  Ticker,
  Timeframe,
  Writable,
} from '../../types';
import { createLiveCandleStart } from '../../utils/calendarCandles';
import { inverseObj } from '../../utils/inverseObj';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { updateOrderBookByPrice } from '../../utils/updateOrderBook';
import { BaseWebSocket } from '../base.ws';
import { getBybitWebSocketOptions } from './bybit.site';
import { BASE_WS_URL, INTERVAL } from './bybit.types';

// Module-level cache. `inverseObj` runs `Object.entries` + a quadratic
// spread-reduce; doing it per kline frame inside the listenOHLCVs
// dispatcher allocated a fresh map every call.
const INTERVAL_INVERSE = inverseObj(INTERVAL) as Record<string, Timeframe>;
// Length of the leading "kline." segment in topics like
// "kline.{interval}.{symbol}". Used to slice without `topic.split('.')`.
const KLINE_TOPIC_PREFIX_LEN = 6;
const OHLCV_NO_DATA_TIMEOUT_MS = 25_000;
const OHLCV_RECOVERY_COOLDOWN_MS = 10_000;
// Bybit livenet closes the socket if too many topics arrive in a single
// subscribe frame, so initial ticker subscribes and reconnect resubscribes
// stay at 10. Bulk kline subscribes are tolerated at a higher ceiling on
// Bybit (we use `publicKlineSubscribeChunkSize` on the parent exchange so
// Hyro can shrink its own cap without affecting ordinary Bybit).
const PUBLIC_TICKER_SUBSCRIBE_CHUNK_SIZE = 10;

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

type SubscribedTopics = {
  [id: string]: string[] | string;
};

export class BybitPublicWebsocket extends BaseWebSocket<BybitExchange> {
  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {
    tickers: [(d: Data) => this.handleTickersEvent(d)],
    pong: [() => this.handlePongEvent()],
  };

  private category: 'linear' | 'inverse';

  private directFallback = new WsDirectFallback();
  private ohlcvNoDataTimeouts: Record<string, NodeJS.Timeout | undefined> = {};
  private lastOhlcvRecoveryAtByTopic: Record<string, number> = {};

  constructor(parent: BybitExchange, category: 'linear' | 'inverse' = 'linear') {
    super(parent);
    this.category = category;
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      const network = this.parent.options.testnet ? 'testnet' : 'livenet';
      // The inverse stream has its own proxy hosts, which `bybitinverse` names.
      const routeName = this.category === 'inverse' ? 'bybitinverse' : this.parent.name;
      const route = resolveTransportRouting(routeName, this.parent.options).wsPublic;
      const directUrl =
        this.parent.getBybitPublicWsUrl?.(this.category, network) ?? BASE_WS_URL.public[this.category][network];
      const baseUrl = this.directFallback.resolveUrl(route.tunnelled && route.host ? route.host : directUrl, route.fallback);
      if (baseUrl === route.fallback) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet bybit public linear proxy');
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
      // add instrument_info topics to subscribe on
      this.topics.tickers = this.parent.store.markets
        .filter((m) => {
          if (m.option) return false;
          if (this.category === 'linear') {
            return m.quote === 'USDT' || m.quote === 'USDC';
          } else {
            return m.quote !== 'USDT' && m.quote !== 'USDC';
          }
        })
        .map((m) => `tickers.${m.symbol}`);

      this.subscribe();
      this.ping();
      this.armExistingOhlcvNoDataTimeouts();

      // Subscribe to new markets dynamically when market list is refreshed
      this.parent.emitter.off('markets:changed', this.onMarketsChanged);
      this.parent.emitter.on('markets:changed', this.onMarketsChanged);

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  private clearOhlcvNoDataTimeout = (topic: string) => {
    const timeoutId = this.ohlcvNoDataTimeouts[topic];
    if (!timeoutId) return;
    clearTimeout(timeoutId);
    delete this.ohlcvNoDataTimeouts[topic];
  };

  private armOhlcvNoDataTimeout = (topic: string, secondaryTopic?: string) => {
    this.clearOhlcvNoDataTimeout(topic);
    if (!this.messageHandlers[topic]?.length) return;

    this.ohlcvNoDataTimeouts[topic] = setTimeout(() => {
      delete this.ohlcvNoDataTimeouts[topic];
      if (this.isDisposed || !this.messageHandlers[topic]?.length) return;
      this.maybeRecoverOhlcvStreamFromTopics(
        topic,
        secondaryTopic,
        `no OHLCV messages after ${OHLCV_NO_DATA_TIMEOUT_MS}ms`,
      );
      this.armOhlcvNoDataTimeout(topic, secondaryTopic);
    }, OHLCV_NO_DATA_TIMEOUT_MS);
  };

  private armExistingOhlcvNoDataTimeouts = () => {
    for (const topic of Object.keys(this.messageHandlers)) {
      if (!topic.startsWith('kline.') || !this.messageHandlers[topic]?.length) continue;
      const symbol = topic.slice(topic.lastIndexOf('.') + 1);
      this.armOhlcvNoDataTimeout(topic, `publicTrade.${symbol}`);
    }
  };

  maybeRecoverOhlcvStream = (opts: Pick<OHLCVOptions, 'symbol' | 'interval'>, reason = 'stale OHLCV stream') => {
    const topic = `kline.${INTERVAL[opts.interval]}.${opts.symbol}`;
    this.maybeRecoverOhlcvStreamFromTopics(topic, `publicTrade.${opts.symbol}`, reason);
  };

  private maybeRecoverOhlcvStreamFromTopics = (topic: string, secondaryTopic: string | undefined, reason: string) => {
    if (!this.messageHandlers[topic]?.length) return;

    const now = Date.now();
    const lastRecoveryAt = this.lastOhlcvRecoveryAtByTopic[topic] || 0;
    if (now - lastRecoveryAt < OHLCV_RECOVERY_COOLDOWN_MS) return;
    this.lastOhlcvRecoveryAtByTopic[topic] = now;

    const activeTopics = [topic];
    if (secondaryTopic && this.messageHandlers[secondaryTopic]?.length) {
      activeTopics.push(secondaryTopic);
    }
    for (const activeTopic of activeTopics) {
      this.topics[activeTopic] = activeTopic;
    }

    if (!this.isConnected) {
      this.parent.log(`[Bybit] Public WS OHLCV recovery reconnecting ${topic} (${reason}).`, LogSeverity.Warning);
      this.reconnect();
      return;
    }

    this.parent.log(`[Bybit] Public WS OHLCV recovery re-subscribing ${topic} (${reason}).`, LogSeverity.Warning);
    this.sendOhlcvDiagnosticPayload({ op: 'unsubscribe', args: activeTopics });
    this.sendOhlcvDiagnosticPayload({ op: 'subscribe', args: activeTopics });
  };

  private sendOhlcvDiagnosticPayload = (payload: { op: string; args: string[] }) => {
    const message = JSON.stringify(payload);
    this.send(message);
    this.emitWsDiagnosticSend(message);
  };

  private emitOhlcvDiagnosticReceive = (data: string) => {
    this.emitWsDiagnosticReceive(data, {
      allowChannel: (json) => {
        if (!json || typeof json !== 'object' || Array.isArray(json)) return false;
        const frame = json as Record<string, unknown>;
        const topic = typeof frame.topic === 'string' ? frame.topic : undefined;
        if (topic?.startsWith('kline.') || topic?.startsWith('publicTrade.')) return true;
        return frame.op === 'subscribe' || frame.op === 'unsubscribe';
      },
    });
  };

  private onMarketsChanged = async ({ added }: { added: Array<{ symbol: string; quote: string; option?: unknown }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    const filtered = added.filter((m) => {
      if (m.option) return false;
      if (this.category === 'linear') return m.quote === 'USDT' || m.quote === 'USDC';
      return m.quote !== 'USDT' && m.quote !== 'USDC';
    });

    if (filtered.length === 0) return;

    const newTopics = filtered.map((m) => `tickers.${m.symbol}`);
    this.topics.tickers = [...(this.topics.tickers || []), ...newTopics];

    for (const chunkTopics of chunk(newTopics, PUBLIC_TICKER_SUBSCRIBE_CHUNK_SIZE)) {
      this.send(JSON.stringify({ op: 'subscribe', args: chunkTopics }));
      await sleep(5);
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ op: 'ping' }));
    }
  };

  subscribe = async () => {
    const topics = flatten(Object.values(this.topics));

    // Need to back off here or Bybit closes. Hyro's simulator also rejects
    // oversized kline batches ("at most 32 topics per connection"); the base
    // 10-topic ticker chunk is already comfortably below that limit, so this
    // path is safe for both venues.
    for (const chunkTopics of chunk(topics, PUBLIC_TICKER_SUBSCRIBE_CHUNK_SIZE)) {
      const payload = { op: 'subscribe', args: chunkTopics };
      this.send(JSON.stringify(payload));
      await sleep(10);
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();
    this.directFallback.onMessage();
    this.emitOhlcvDiagnosticReceive(data);

    // Extract the "topic" field once via targeted indexOf instead of
    // O(N) substring scans per registered handler. Bybit V5 frames always
    // include `"topic":"<full topic>"` for data messages.
    const topicIdx = data.indexOf('"topic":"');
    if (topicIdx >= 0) {
      const start = topicIdx + 9;
      const end = data.indexOf('"', start);
      if (end <= start) return;
      const fullTopic = data.slice(start, end);

      // Exact-match handler (e.g. "orderbook.1000.BTCUSDT", "kline.1.BTCUSDT")
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

      // Record stats under the key that actually matched. When only a
      // prefix handler fired (e.g. the single 'tickers' handler covering
      // every tickers.<symbol> frame), recording the full per-symbol wire
      // topic would grow messageHandlersStats by one entry per subscribed
      // symbol (~600 on linear) and put every ticker under
      // reconnect-banner tracking.
      this.recordMessageHandlerStats(hasExact ? fullTopic : fullTopic.slice(0, dotIdx));
      const json = jsonParse(data);
      if (!json) return;
      // Indexed for-loop instead of `for...of` over the handler array
      // — `for...of` on Arrays creates an iterator (allocation + protocol
      // overhead). Hot-path frames usually have a single-element handler
      // array; the iterator wrapper cost is real even with V8 optimizations.
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

    // O(1) lookup. Skips parseFloat work and update allocation when an
    // exchange pushes a ticker for a symbol we don't track, and avoids
    // allocating `{ id }` for the store call on the hit path.
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
      update.fundingRate = parseFloat(d.fundingRate);
    }

    if (d.nextFundingTime) {
      update.fundingTime = parseFloat(d.nextFundingTime);
    }

    // Inverse frames carry volume24h in USD contracts and turnover24h in the coin, the reverse of linear.
    const [baseVolume, quoteVolume] =
      this.category === 'inverse' ? [d.turnover24h, d.volume24h] : [d.volume24h, d.turnover24h];

    if (baseVolume) {
      update.volume = parseFloat(baseVolume);
    }

    if (quoteVolume) {
      update.quoteVolume = parseFloat(quoteVolume);
    }

    this.parent.store.updateTicker(ticker, update);
  };

  listenOHLCVs = (
    opts: OHLCVSOptions,
    callback: (candle: Candle & { symbol: string; interval: Timeframe }) => void,
  ) => {
    if (this.parent.supportsTradeDerivedCandles === false) {
      this.parent.validateCandleInterval(opts.interval);
    }
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

    // Fall back to the Bybit-livenet default if the parent predates the
    // Hyro-vs-Bybit chunk split (keeps the ws class usable from test doubles
    // that mock the parent exchange).
    const chunkSize = this.parent.publicKlineSubscribeChunkSize ?? 50;
    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          for (const chunkTopics of chunk(topics, chunkSize)) {
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
    const tradeDerived = this.parent.supportsTradeDerivedCandles !== false;
    if (!tradeDerived) this.parent.validateCandleInterval(opts.interval);

    // Subscription-invariant: built once instead of recomputed per trade. The
    // epoch floor for intraday intervals; Monday weeks / calendar months for
    // `1w` / `1M`, matching the `kline.W` / `kline.M` bar it extends (an epoch
    // floor would start a Thursday bar mid-week).
    const candleStartOf = createLiveCandleStart(opts.interval);
    const isSeconds = opts.interval.endsWith('s');

    let lastCandle: Candle | undefined = undefined;

    const parser = ({ data: [candle] }: Data) => {
      const timestamp = candle.start / 1000;
      if (lastCandle && lastCandle.timestamp > timestamp) {
        // prevent bleeding candles backwards
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
      this.armOhlcvNoDataTimeout(topic, secondaryTopic);
      callback(lastCandle);
    };

    const secondaryParser = ({ data }: Data) => {
      // Hot path: Bybit publicTrade frames carry many fields we don't use
      // (s, L, i, BT, RPI...). Avoid the rest-spread allocation by reading
      // by index/key directly.
      const trade = data[0];
      const T = trade.T;
      const p = trade.p;
      const v = trade.v;
      const S = trade.S;

      // T is always in ms here (Bybit V5 publicTrade timestamp).
      const candleStart = candleStartOf(T / 1000);
      const price = parseFloat(p);
      if (isSeconds && (!lastCandle || lastCandle.timestamp !== candleStart)) {
        const seed = lastCandle?.close ?? price;
        lastCandle = {
          timestamp: candleStart,
          open: seed,
          high: seed,
          low: seed,
          close: seed,
          volume: 0,
        };
      }

      if (!lastCandle) return;
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
      // need decimal-aware rounding, and `add()` runs String(num) +
      // afterDecimal twice per call.
      lastCandle = {
        ...lastCandle,
        high: lastCandle.high > price ? lastCandle.high : price,
        low: lastCandle.low < price ? lastCandle.low : price,
        close: price,
        volume: lastCandle.volume + v / price,
        lastSide: S === 'Sell' ? OrderSide.Sell : OrderSide.Buy,
      };
      this.armOhlcvNoDataTimeout(topic, secondaryTopic);
      callback(lastCandle);
    };

    let isActive = true;
    let subscribeRetryTimeout: NodeJS.Timeout | undefined;

    const waitForConnectedAndSubscribe = () => {
      if (!isActive || this.isDisposed) return;
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];
          const payload = { op: 'subscribe', args: [topic] };
          this.sendOhlcvDiagnosticPayload(payload);
          this.topics[topic] = topic;

          if (tradeDerived) {
            this.messageHandlers[secondaryTopic] = [...(this.messageHandlers[secondaryTopic] || []), secondaryParser];
            const secondaryPayload = { op: 'subscribe', args: [secondaryTopic] };
            this.sendOhlcvDiagnosticPayload(secondaryPayload);
            this.topics[secondaryTopic] = secondaryTopic;
          }
          this.armOhlcvNoDataTimeout(topic, secondaryTopic);
        }
      } else {
        subscribeRetryTimeout = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      isActive = false;
      if (subscribeRetryTimeout) {
        clearTimeout(subscribeRetryTimeout);
        subscribeRetryTimeout = undefined;
      }
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];
      if (tradeDerived) {
        this.messageHandlers[secondaryTopic] = [
          ...(this.messageHandlers[secondaryTopic] || []).filter((f) => f !== secondaryParser),
        ];
      }

      if (!this.messageHandlers[topic].length) {
        delete this.topics[topic];
        this.clearOhlcvNoDataTimeout(topic);
        delete this.lastOhlcvRecoveryAtByTopic[topic];
      }

      if (tradeDerived && !this.messageHandlers[secondaryTopic].length) {
        delete this.topics[secondaryTopic];
      }

      if (this.isConnected && !this.messageHandlers[topic].length) {
        const payload = { op: 'unsubscribe', args: [topic] };
        this.cleanupMessageHandlerStats(topic);
        this.sendOhlcvDiagnosticPayload(payload);
      }

      if (tradeDerived && this.isConnected && !this.messageHandlers[secondaryTopic].length) {
        const payload = { op: 'unsubscribe', args: [secondaryTopic] };
        this.cleanupMessageHandlerStats(secondaryTopic);
        this.sendOhlcvDiagnosticPayload(payload);
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

    const topic = `orderbook.1000.${symbol}`;
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
