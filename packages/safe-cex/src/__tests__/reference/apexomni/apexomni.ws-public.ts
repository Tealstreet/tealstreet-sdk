import chunk from 'lodash/chunk';
import flatten from 'lodash/flatten';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { Candle, OHLCVOptions, OrderBook, PositionSide, Ticker, Writable } from '../../../types';
import { getCandleStartTime } from '../../../utils/getCandleStartTime';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { add, divide } from '../../../utils/safe-math';
import { updateOrderBook } from '../../../utils/updateOrderBook';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { ApexOmniExchange } from './apexomni.exchange';
import { BASE_WS_URL, INTERVAL } from './apexomni.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

type SubscribedTopics = {
  [id: string]: string[] | string;
};

export class ApexOmniPublicWebsocket extends BaseWebSocket<ApexOmniExchange> {
  private readonly bookResets = new Set<() => void>();
  private readonly resetBooks = () => {
    if (!this.isDisposed) for (const reset of this.bookResets) reset();
  };
  topics: SubscribedTopics = {
    // add instrument_info topics to subscribe on
    instrumentInfo: 'instrumentInfo.all',
  };
  messageHandlers: MessageHandlers = {
    instrumentInfo: [(d: Data) => this.handleInstrumentInfoEvent(d)],
    pong: [() => this.handlePongEvent()],
  };

  constructor(parent: ApexOmniExchange) {
    super(parent);
  }

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      const defaultEndpoint = BASE_WS_URL.public.linear[this.parent.options.testnet ? 'testnet' : 'livenet'];
      const endpointWithLiveTimestamp = defaultEndpoint.replace(
        /timestamp=\d+/,
        `timestamp=${virtualClock.getCurrentTimeMs()}`,
      );
      const queryJoiner = endpointWithLiveTimestamp.includes('?') ? '&' : '?';
      this.ws = this.createWebsocket(`${endpointWithLiveTimestamp}${queryJoiner}requestId=${nanoid()}`);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.resetBooks);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.resetBooks();
      this.subscribe();
      this.ping();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send(JSON.stringify({ op: 'ping' }));
    }
  };

  subscribe = () => {
    const topics = flatten(Object.values(this.topics));
    for (const chunkTopics of chunk(topics, 50)) {
      const payload = { op: 'subscribe', args: chunkTopics };
      this.send(JSON.stringify(payload));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();

    if (data.includes('"op"') && data.includes('"ping"')) {
      const control = jsonParse(data);
      if (control?.op === 'ping') {
        this.send(JSON.stringify({ op: 'pong', args: [`${virtualClock.getCurrentTimeMs()}`] }));
        return;
      }
    }

    // Extract `topic` once via targeted indexOf — see exchanges/CLAUDE.md
    // → 'Public WebSocket Hot-Path Patterns'.
    const topicIdx = data.indexOf('"topic":"');
    if (topicIdx >= 0) {
      const start = topicIdx + 9;
      const end = data.indexOf('"', start);
      if (end <= start) return;
      const fullTopic = data.slice(start, end);

      // Exact-match (e.g. orderBook200.H.BTCUSDT, candle.5.BTCUSDT)
      const exact = this.messageHandlers[fullTopic];
      // Prefix fallback (e.g. instrumentInfo matches instrumentInfo.all)
      let prefix: typeof exact | undefined;
      const dotIdx = fullTopic.indexOf('.');
      if (dotIdx > 0) {
        prefix = this.messageHandlers[fullTopic.slice(0, dotIdx)];
      }

      const hasExact = exact && exact.length > 0;
      const hasPrefix = prefix && prefix.length > 0;
      if (!hasExact && !hasPrefix) return;

      this.recordMessageHandlerStats(fullTopic);
      const json = jsonParse(data);
      if (!json) return;
      if (hasExact) for (const cb of exact!) cb(json);
      if (hasPrefix) for (const cb of prefix!) cb(json);
      return;
    }

    if (data.includes('pong')) {
      const pongHandlers = this.messageHandlers.pong;
      if (pongHandlers && pongHandlers.length > 0) {
        this.recordMessageHandlerStats('pong');
        for (const cb of pongHandlers) cb({});
      }
    }
  };

  handleInstrumentInfoEvent = (json: Record<string, any>) => {
    const tickers = json.data as ReadonlyArray<{
      s: string;
      pr: string;
      p: string;
      h: string;
      l: string;
      mp: string;
      xp: string;
      o: string;
      to: string;
      v: string;
      fr: string;
      tc: string;
    }>;

    for (const d of tickers) {
      //for (const settlementCurrency of ['USDT', 'USDC']) {
      const currSymbol = d?.s; //.replace(/(USDT|USDC)$/, settlementCurrency);

      // Use O(1) hashmap lookup instead of O(n) find
      const ticker = this.parent.store.tickersBySymbol[currSymbol] || this.parent.store.tickersById[currSymbol];

      if (ticker) {
        const update: Partial<Writable<Ticker>> = {};

        if (d.p) update.last = parseFloat(d.p);
        if (d.mp) update.mark = parseFloat(d.mp);
        if (d.xp) update.index = parseFloat(d.xp);
        if (!d.mp && d.xp) {
          update.mark = update.index;
        }

        if (d.pr) {
          update.percentage = parseFloat(d.pr) * 100;
        }

        if (d.o) {
          update.openInterest = parseFloat(d.o);
        }

        if (d.fr) {
          update.fundingRate = parseFloat(d.fr);
        }

        if (d.v) {
          update.volume = parseFloat(d.v);
        }

        if (d.to) {
          update.quoteVolume = parseFloat(d.to);
        }

        this.parent.store.updateTicker(ticker, update);

        // position pnl updates are slow. we can try and calculate it here
        const referencePrice = update.mark || update.last;
        if (referencePrice) {
          // Use O(1) hashmap lookup once per ticker. An unknown/inverse market
          // skips only this ticker's local PnL calculation, not the rest of the frame.
          const market = this.store.marketsBySymbol[d.s];
          if (!market || market.isInverse) continue;
          const matchingPositions = this.store.positions.filter((p) => p.symbol === d.s && p.contracts !== 0);
          for (const position of matchingPositions) {
            const entryValue = position.contracts * position.entryPrice;
            const currentValue = position.contracts * referencePrice;
            const directionMult = position.side === PositionSide.Long ? 1 : -1;
            const unrealizedPnl = directionMult * (currentValue - entryValue);
            this.store.updatePosition(position, { unrealizedPnl });
          }
        }
      }
      //}
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = `candle.${INTERVAL[opts.interval]}.${opts.symbol}`;
    const secondaryTopic = `recentlyTrade.H.${opts.symbol}`;

    let lastCandle: Candle | undefined = undefined;

    const parser = (json: Data) => {
      if (!json?.data || !Array.isArray(json.data) || json.data.length === 0) return;

      const candle = json.data[0];
      if (!candle || typeof candle !== 'object' || !candle.start) return;

      const timestamp = candle.start / 1000;
      if (lastCandle && lastCandle.timestamp > timestamp) {
        // prevent bleeding candles backwards
        // console.warn('Received candle with timestamp in the past, ignoring:', candle);
        return;
      }
      lastCandle = {
        timestamp,
        open: parseFloat(candle.open),
        high: parseFloat(candle.high),
        low: parseFloat(candle.low),
        close: parseFloat(candle.close),
        volume: parseFloat(candle.volume),
      };
      callback(lastCandle);
    };

    const secondaryParser = (json: Data) => {
      if (!lastCandle) return;
      if (!json?.data || !Array.isArray(json.data) || json.data.length === 0) return;

      const trade = json.data[0];
      if (!trade || typeof trade !== 'object') return;

      const { T, s: _s, p, v } = trade;
      if (T === undefined || p === undefined || v === undefined) return;

      const candleStart = getCandleStartTime(opts.interval, T / 1000);
      const price = p;
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
      lastCandle = {
        ...lastCandle,
        high: Math.max(lastCandle.high, price),
        low: Math.min(lastCandle.low, price),
        close: price,
        volume: add(lastCandle.volume, divide(v, price)),
      };
      callback(lastCandle);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];
          if (!this.parent.options.publicDataOnly) {
            this.messageHandlers[secondaryTopic] = [...(this.messageHandlers[secondaryTopic] || []), secondaryParser];
          }

          const payload = { op: 'subscribe', args: [topic] };
          this.send(JSON.stringify(payload));
          const secondaryPayload = { op: 'subscribe', args: [secondaryTopic] };
          if (!this.parent.options.publicDataOnly) {
            this.send(JSON.stringify(secondaryPayload));
          }
          //this.parent.log(`Switched to [${opts.symbol}:${opts.interval}]`);

          // store subscribed topic to re-subscribe on reconnect
          this.topics[topic] = topic;
          if (!this.parent.options.publicDataOnly) {
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

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    const topic = `orderBook200.H.${symbol}`;
    const orderBook: OrderBook = { bids: [], asks: [] };

    let sequence: number | undefined;
    const reset = (notify = false) => {
      sequence = undefined;
      const hadLevels = orderBook.bids.length > 0 || orderBook.asks.length > 0;
      orderBook.bids = [];
      orderBook.asks = [];
      if (notify || hadLevels) callback(orderBook);
    };
    this.bookResets.add(reset);

    const parseLevels = (levels: unknown): OrderBook['asks'] | undefined => {
      if (levels === undefined) return [];
      if (!Array.isArray(levels)) return undefined;
      const parsed: OrderBook['asks'] = [];
      for (const level of levels) {
        if (!Array.isArray(level) || level.length !== 2) return undefined;
        if (
          level.some(
            (value) =>
              (typeof value !== 'string' && typeof value !== 'number') ||
              (typeof value === 'string' && !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value)),
          )
        )
          return undefined;
        const price = Number(level[0]);
        const amount = Number(level[1]);
        if (!Number.isFinite(price) || price <= 0 || !Number.isFinite(amount) || amount < 0) return undefined;
        parsed.push({ price, amount, total: 0 });
      }
      return parsed;
    };
    const parser = (data: Data) => {
      const d = data.data;
      if (!d || !Number.isSafeInteger(d.u) || d.u < 0) return;
      const asks = parseLevels(d.a);
      const bids = parseLevels(d.b);
      // Validate the complete frame before touching either side or its cursor.
      if (!asks || !bids) return;
      if (data.type === 'snapshot') {
        orderBook.bids = [];
        orderBook.asks = [];
      } else if (data.type === 'delta') {
        if (sequence === undefined || d.u <= sequence) return;
        if (d.u !== sequence + 1) {
          reset(true);
          this.send(JSON.stringify({ op: 'unsubscribe', args: [topic] }));
          this.send(JSON.stringify({ op: 'subscribe', args: [topic] }));
          return;
        }
      } else return;
      sequence = d.u;
      for (const update of asks) updateOrderBook(orderBook, 'asks', update);
      for (const update of bids) updateOrderBook(orderBook, 'bids', update);
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
      this.bookResets.delete(reset);
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
