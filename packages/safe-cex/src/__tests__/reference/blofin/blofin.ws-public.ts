import type { Candle, OHLCVOptions, OrderBook, OrderBookOrders } from '../../../types';
import type { BlofinExchange } from './blofin.exchange';

import flatten from 'lodash/flatten';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { LogSeverity } from '../../../types';
import { getCandleStartTime } from '../../../utils/getCandleStartTime';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { roundUSD } from '../../../utils/roundUSD';
import { updateOrderBook } from '../../../utils/updateOrderBook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, INTERVAL } from '../../../exchanges/blofin/blofin.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [channel: string]: Array<(json: Data) => void>;
};
type SubscribedTopics = {
  [id: string]: Array<{ channel: string; instId: string }>;
};

type SubscriptionTopic = { channel: string; instId: string };

export const BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES = 4096;

export const buildBlofinSubscribeFrames = (
  topics: SubscriptionTopic[],
  maxBytes = BLOFIN_MAX_SUBSCRIBE_FRAME_BYTES,
) => {
  const frames: string[] = [];
  let batch: SubscriptionTopic[] = [];

  for (const topic of topics) {
    const singleTopicFrame = JSON.stringify({ op: 'subscribe', args: [topic] });
    if (new TextEncoder().encode(singleTopicFrame).byteLength > maxBytes) {
      throw new Error('BloFin subscription topic exceeds the maximum WebSocket frame size');
    }

    const candidate = [...batch, topic];
    const serialized = JSON.stringify({ op: 'subscribe', args: candidate });

    if (batch.length > 0 && new TextEncoder().encode(serialized).byteLength > maxBytes) {
      frames.push(JSON.stringify({ op: 'subscribe', args: batch }));
      batch = [topic];
      continue;
    }

    batch = candidate;
  }

  if (batch.length > 0) {
    frames.push(JSON.stringify({ op: 'subscribe', args: batch }));
  }

  return frames;
};

export class BlofinPublicWebsocket extends BaseWebSocket<BlofinExchange> {
  private static readonly CONNECTION_FAILURE_NOTICE_DELAY_MS = 20_000;
  private static readonly CONNECTION_FAILURE_NOTICE =
    'BloFin live WebSocket connection failed to establish. Historical candles may still load, but live charts and order books will not update. Check your network, VPN, firewall, or browser extensions, then reload Tealstreet.';

  // BloFin's upstream has been observed to delay/drop `pong` echoes even while
  // ticker/orderbook updates keep flowing. Any inbound message on this socket
  // is sufficient proof of liveness — fall back to the 20s watchdog for truly
  // dead connections.
  protected override treatAnyMessageAsPong = true;

  private connectionFailureNoticeTimeout?: ReturnType<typeof setTimeout>;
  private connectionFailureNotified = false;

  constructor(parent: BlofinExchange) {
    super(parent);

    // BaseWebSocket.onClose is an instance arrow field, so retain that exact
    // handler before layering BloFin's user-facing connection watchdog on it.
    const baseOnClose = this.onClose;
    this.onClose = (event?: any): void => {
      baseOnClose(event);
      this.armConnectionFailureNotice();
    };
  }

  topics: SubscribedTopics = {};
  messageHandlers: MessageHandlers = {
    tickers: [(d: Data) => this.handleTickerEvents(d)],
    'funding-rate': [(d: Data) => this.handleFundingRateEvents(d)],
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.armConnectionFailureNotice();
      this.ws = this.createWebsocket(BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet']);

      this.topics.tickers = this.store.markets.map((m) => ({
        channel: 'tickers',
        instId: m.id,
      }));

      this.topics['funding-rate'] = this.store.markets.map((m) => ({
        channel: 'funding-rate',
        instId: m.id,
      }));

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

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.emitWsReconnected();
      }
    }
  };

  private onMarketsChanged = ({ added }: { added: Array<{ id: string }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;

    const newTickerTopics = added.map((m) => ({ channel: 'tickers', instId: m.id }));
    const newFundingTopics = added.map((m) => ({ channel: 'funding-rate', instId: m.id }));

    this.topics.tickers = [...(this.topics.tickers || []), ...newTickerTopics];
    this.topics['funding-rate'] = [...(this.topics['funding-rate'] || []), ...newFundingTopics];

    this.sendSubscriptionTopics([...newTickerTopics, ...newFundingTopics]);
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.send('ping');
    }
  };

  subscribe = () => {
    const topics = flatten(Object.values(this.topics));
    this.sendSubscriptionTopics(topics);
  };

  private sendSubscriptionTopics = (topics: SubscriptionTopic[]) => {
    for (const frame of buildBlofinSubscribeFrames(topics)) {
      this.send(frame);
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.markConnectionEstablished();
    this.trackLastReceived();

    if (data === 'pong') {
      this.handlePongEvent();
      return;
    }

    // Extract `channel` once via targeted indexOf instead of an
    // O(N-handlers) substring-scan loop. The prior dispatch did 3 substring
    // scans per registered handler per frame, dominating CPU on accounts with
    // many tickers + funding-rate + per-symbol books / candles subscribed.
    const channelIdx = data.indexOf('"channel":"');
    if (channelIdx < 0) return;
    const cStart = channelIdx + 11;
    const cEnd = data.indexOf('"', cStart);
    if (cEnd <= cStart) return;
    const channel = data.slice(cStart, cEnd);

    // Suppress sub/unsub acks that include `"channel"` but no real payload.
    // The prior dispatch checked `!data.includes('event":"subscribe"')`; we
    // preserve that semantic via a single targeted lookup.
    if (data.indexOf('"event":"subscribe"') >= 0) return;

    // Broadcast handlers are keyed by channel alone (tickers, funding-rate).
    const broadcast = this.messageHandlers[channel];

    // Per-instrument handlers are keyed by `${channel}.${instId}` for
    // candle*, books, books5. Extract `instId` once from the data field.
    let exact: typeof broadcast | undefined;
    const instIdx = data.indexOf('"instId":"', cEnd);
    if (instIdx >= 0) {
      const iStart = instIdx + 10;
      const iEnd = data.indexOf('"', iStart);
      if (iEnd > iStart) {
        exact = this.messageHandlers[`${channel}.${data.slice(iStart, iEnd)}`];
      }
    }

    const hasBroadcast = broadcast && broadcast.length > 0;
    const hasExact = exact && exact.length > 0;
    if (!hasBroadcast && !hasExact) return;

    const json = jsonParse(data);
    if (!json) return;
    if (hasBroadcast) {
      for (const cb of broadcast!) cb(json);
    }
    if (hasExact) {
      for (const cb of exact!) cb(json);
    }
  };

  private armConnectionFailureNotice = () => {
    if (this.isDisposed || this.connectionFailureNotified || this.connectionFailureNoticeTimeout) return;

    this.connectionFailureNoticeTimeout = setTimeout(() => {
      this.connectionFailureNoticeTimeout = undefined;
      if (this.isDisposed || this.connectionFailureNotified) return;

      this.connectionFailureNotified = true;
      this.parent.log(BlofinPublicWebsocket.CONNECTION_FAILURE_NOTICE, LogSeverity.Warning);
      // Force the warning through while the exchange appears disconnected;
      // ordinary exchange errors are intentionally hidden during reconnects.
      this.parent.emitter.emit('warning', BlofinPublicWebsocket.CONNECTION_FAILURE_NOTICE, { forceToast: true });
    }, BlofinPublicWebsocket.CONNECTION_FAILURE_NOTICE_DELAY_MS);
  };

  private markConnectionEstablished = () => {
    if (this.connectionFailureNoticeTimeout) {
      clearTimeout(this.connectionFailureNoticeTimeout);
      this.connectionFailureNoticeTimeout = undefined;
    }
    this.connectionFailureNotified = false;
  };

  handleTickerEvents = ({ data: [update] }: Data) => {
    const open = parseFloat(update.open24h);
    const last = parseFloat(update.last);
    const percentage = roundUSD(((last - open) / open) * 100);

    this.store.updateTicker(
      { id: update.instId },
      {
        bid: parseFloat(update.bidPrice),
        ask: parseFloat(update.askPrice),
        last,
        mark: last,
        index: last,
        openInterest: 0,
        percentage,
        volume: parseFloat(update.volCurrency24h),
        quoteVolume: parseFloat(update.vol24h),
      },
    );
  };

  handleFundingRateEvents = ({ data: [update] }: Data) => {
    this.store.updateTicker(
      { id: update.instId },
      { fundingRate: parseFloat(update.fundingRate), fundingTime: parseFloat(update.fundingTime) },
    );
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    if (!this.store.loaded.markets) {
      timeoutId = setTimeout(() => this.listenOHLCV(opts, callback), 100);

      return () => {
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      };
    }

    const market = this.store.markets.find((m) => m.symbol === opts.symbol);
    if (!market) return () => {};

    const candleChannel = `candle${INTERVAL[opts.interval]}`;
    const handler = `${candleChannel}.${market.id}`;
    const topic = {
      channel: candleChannel,
      instId: market.id,
    };

    const secondaryHandler = `books5.${market.id}`;
    const secondaryTopic = { channel: 'books5', instId: market.id };

    let lastCandle: Candle | null = null;

    type BooksSnapshotWithTs = {
      ts: string | number;
      bids: Array<[string, string]>;
      asks: Array<[string, string]>;
    };
    const parseBookSnapshot = (data: Data): BooksSnapshotWithTs | null => {
      const payload = data?.data;
      if (Array.isArray(payload)) {
        const first = payload[0];
        if (first && first.ts && Array.isArray(first.bids) && Array.isArray(first.asks)) {
          return first as BooksSnapshotWithTs;
        }
        return null;
      }
      if (payload && payload.ts && Array.isArray(payload.bids) && Array.isArray(payload.asks)) {
        return payload as BooksSnapshotWithTs;
      }
      return null;
    };

    const parseCandleUpdate = (data: Data): Candle | null => {
      const rawData = data?.data;
      let candleRow: any = null;

      if (Array.isArray(rawData)) {
        candleRow = rawData[0];
      } else if (rawData && typeof rawData === 'object') {
        candleRow = rawData;
      }

      if (!candleRow) {
        return null;
      }

      // Standard BloFin shape: [ts, open, high, low, close, ..., ..., volume]
      if (Array.isArray(candleRow)) {
        const timestamp = Number.parseInt(String(candleRow[0]), 10) / 1000;
        const open = Number.parseFloat(String(candleRow[1]));
        const high = Number.parseFloat(String(candleRow[2]));
        const low = Number.parseFloat(String(candleRow[3]));
        const close = Number.parseFloat(String(candleRow[4]));
        const volume = Number.parseFloat(String(candleRow[7] ?? candleRow[5] ?? candleRow[6] ?? 0));

        const nextCandle: Candle = { timestamp, open, high, low, close, volume };
        const hasInvalidNumber = Object.values(nextCandle).some((value) => !Number.isFinite(value));
        return hasInvalidNumber ? null : nextCandle;
      }

      // Defensive object-form fallback in case channel shape drifts.
      const timestampRaw = candleRow.ts ?? candleRow.timestamp ?? candleRow.t ?? candleRow.time;
      const timestamp = Number.parseInt(String(timestampRaw ?? ''), 10);
      const open = Number.parseFloat(String(candleRow.open ?? candleRow.o ?? ''));
      const high = Number.parseFloat(String(candleRow.high ?? candleRow.h ?? ''));
      const low = Number.parseFloat(String(candleRow.low ?? candleRow.l ?? ''));
      const close = Number.parseFloat(String(candleRow.close ?? candleRow.c ?? ''));
      const volume = Number.parseFloat(String(candleRow.volume ?? candleRow.vol ?? candleRow.v ?? 0));

      if (!Number.isFinite(timestamp) || timestamp <= 0) {
        return null;
      }

      const nextCandle: Candle = {
        timestamp: timestamp > 1_000_000_000_000 ? timestamp / 1000 : timestamp,
        open,
        high,
        low,
        close,
        volume,
      };

      const hasInvalidNumber = Object.values(nextCandle).some((value) => !Number.isFinite(value));
      return hasInvalidNumber ? null : nextCandle;
    };

    const parser = (data: Data) => {
      const nextCandle = parseCandleUpdate(data);
      if (!nextCandle) return;

      const isSameCandle =
        !!lastCandle &&
        lastCandle.timestamp === nextCandle.timestamp &&
        lastCandle.open === nextCandle.open &&
        lastCandle.high === nextCandle.high &&
        lastCandle.low === nextCandle.low &&
        lastCandle.close === nextCandle.close &&
        lastCandle.volume === nextCandle.volume;
      if (isSameCandle) {
        return;
      }

      lastCandle = nextCandle;
      callback(lastCandle);
    };

    // use orderbook for mid market chart to speed up appearance of chart
    const secondaryParser = (data: Data) => {
      // BloFin can emit books5 payload as either data object or data array.
      const snapshot = parseBookSnapshot(data);
      if (!snapshot) {
        this.parent.log(
          `BloFin: Invalid orderbook snapshot shape. Received: ${JSON.stringify(data)}`,
          LogSeverity.Warning,
        );
        return;
      }
      const timestamp = Number.parseInt(String(snapshot.ts), 10);
      const candleStart = getCandleStartTime(opts.interval, timestamp / 1000);
      const topBid = parseFloat(snapshot.bids?.[0]?.[0]);
      const topAsk = parseFloat(snapshot.asks?.[0]?.[0]);
      if (!Number.isFinite(topBid) || !Number.isFinite(topAsk) || !lastCandle || !Number.isFinite(lastCandle.close)) {
        return;
      }

      if (lastCandle.timestamp !== candleStart) {
        return;
      }

      const newClose = [topBid, lastCandle.close, topAsk].sort((a, b) => a - b)[1]; // median of top bid/ask/last
      if (newClose === lastCandle.close) return;

      lastCandle = {
        ...lastCandle,
        close: newClose,
        high: Math.max(lastCandle.high, newClose),
        low: Math.min(lastCandle.low, newClose),
      };

      callback(lastCandle);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
          this.send(JSON.stringify({ op: 'subscribe', args: [topic] }));
          // store the topic so we can unsubscribe later
          this.topics[handler] = [topic];

          this.messageHandlers[secondaryHandler] = [...(this.messageHandlers[secondaryHandler] || []), secondaryParser];
          this.send(JSON.stringify({ op: 'subscribe', args: [secondaryTopic] }));
          // store the topic so we can unsubscribe later
          this.topics[secondaryHandler] = [secondaryTopic];
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      if (!this.messageHandlers[handler].length) {
        delete this.topics[handler];
        this.cleanupMessageHandlerStats(handler);
      }

      this.messageHandlers[secondaryHandler] = [
        ...(this.messageHandlers[secondaryHandler] || []).filter((f) => f !== secondaryParser),
      ];
      if (!this.messageHandlers[secondaryHandler].length) {
        delete this.topics[secondaryHandler];
        this.cleanupMessageHandlerStats(secondaryHandler);
      }

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = { op: 'unsubscribe', args: [topic] };
        this.send(JSON.stringify(payload));
      }

      if (this.isConnected && !this.messageHandlers[secondaryHandler].length) {
        const payload = { op: 'unsubscribe', args: [secondaryTopic] };
        this.send(JSON.stringify(payload));
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    if (!this.store.loaded.markets) {
      timeoutId = setTimeout(() => this.listenOrderBook(symbol, callback), 100);

      return () => {
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      };
    }

    const market = this.store.markets.find((m) => m.symbol === symbol);
    if (!market) return () => {};

    const orderBook: OrderBook = { bids: [], asks: [] };

    const handler = `books.${market.id}`;
    const topic = { channel: 'books', instId: market.id };

    const secondaryHandler = `books5.${market.id}`;
    const secondaryTopic = { channel: 'books5', instId: market.id };
    type BooksSnapshot = {
      bids: Array<[string, string]>;
      asks: Array<[string, string]>;
      prevSeqId?: string | number;
      seqId?: string | number;
    };
    const parseBookSnapshot = (data: Data): BooksSnapshot | null => {
      const payload = data?.data;
      if (Array.isArray(payload)) {
        const first = payload[0];
        if (first && Array.isArray(first.bids) && Array.isArray(first.asks)) {
          return first as BooksSnapshot;
        }
        return null;
      }
      if (payload && Array.isArray(payload.bids) && Array.isArray(payload.asks)) {
        return payload as BooksSnapshot;
      }
      return null;
    };
    let lastBookSeqId: string | undefined;
    let hasBookSnapshot = false;
    let bookResubscribePending = false;

    const requestFreshBookSnapshot = () => {
      if (!bookResubscribePending && this.isConnected && !this.isDisposed) {
        this.send(JSON.stringify({ op: 'unsubscribe', args: [topic] }));
        this.send(JSON.stringify({ op: 'subscribe', args: [topic] }));
      }
      bookResubscribePending = true;
      hasBookSnapshot = false;
      lastBookSeqId = undefined;
      orderBook.bids = [];
      orderBook.asks = [];
    };

    const parser = (data: Data) => {
      if (data.action === 'snapshot') {
        const snapshot = parseBookSnapshot(data);
        if (!snapshot) return;

        lastBookSeqId = snapshot.seqId === undefined ? undefined : String(snapshot.seqId);
        hasBookSnapshot = true;
        bookResubscribePending = false;

        // Linear push into a fresh array for each side. The prior reduce
        // pattern was quadratic — `[...acc, x]` allocated and copied on
        // every level (~200×) per snapshot side.
        const newBids: OrderBookOrders[] = [];
        const sBids = snapshot.bids;
        for (let i = 0; i < sBids.length; i++) {
          const [price, amount] = sBids[i];
          if (parseFloat(amount) === 0) continue;
          newBids.push({
            price: parseFloat(price),
            amount: parseFloat(amount),
            total: 0,
          });
        }
        orderBook.bids = newBids;

        const newAsks: OrderBookOrders[] = [];
        const sAsks = snapshot.asks;
        for (let i = 0; i < sAsks.length; i++) {
          const [price, amount] = sAsks[i];
          if (parseFloat(amount) === 0) continue;
          newAsks.push({
            price: parseFloat(price),
            amount: parseFloat(amount),
            total: 0,
          });
        }
        orderBook.asks = newAsks;
      }

      if (data.action === 'update') {
        const update = parseBookSnapshot(data);
        if (!update) return;

        const prevSeqId = update.prevSeqId === undefined ? undefined : String(update.prevSeqId);
        const seqId = update.seqId === undefined ? undefined : String(update.seqId);
        if (!hasBookSnapshot || !lastBookSeqId || !prevSeqId || !seqId || prevSeqId !== lastBookSeqId) {
          requestFreshBookSnapshot();
          return;
        }

        const uBids = update.bids;
        for (let i = 0; i < uBids.length; i++) {
          const [rPrice, rAmount] = uBids[i];
          updateOrderBook(orderBook, 'bids', {
            price: parseFloat(rPrice),
            amount: parseFloat(rAmount),
            total: 0,
          });
        }
        const uAsks = update.asks;
        for (let i = 0; i < uAsks.length; i++) {
          const [rPrice, rAmount] = uAsks[i];
          updateOrderBook(orderBook, 'asks', {
            price: parseFloat(rPrice),
            amount: parseFloat(rAmount),
            total: 0,
          });
        }
        lastBookSeqId = seqId;
      }

      // Use O(1) hashmap lookup instead of O(n) find
      const ticker = this.store.tickersBySymbol[market.symbol];
      const lastPrice = ticker?.last;

      if (lastPrice) {
        orderBook.asks = orderBook.asks.filter((a) => a.price >= lastPrice);
        orderBook.bids = orderBook.bids.filter((b) => b.price <= lastPrice);
      }

      calcOrderBookTotal(orderBook);
      callback(orderBook);
    };

    const secondaryParser = (data: Data) => {
      // always snapshot — top-N levels (books5 channel)
      const snapshot = parseBookSnapshot(data);
      if (!snapshot) return;

      // Replace prices covered by the snapshot, preserving only deeper levels.
      // Replacing by array index can duplicate prices when the top of book moves.
      for (const side of ['bids', 'asks'] as const) {
        const levels = snapshot[side]
          .map(([price, amount]) => ({ price: parseFloat(price), amount: parseFloat(amount), total: 0 }))
          .filter((level) => level.amount !== 0);
        const boundary = levels[levels.length - 1]?.price;
        const deeper =
          levels.length < 5 || boundary === undefined
            ? []
            : orderBook[side].filter((level) => (side === 'bids' ? level.price < boundary : level.price > boundary));
        orderBook[side] = [...levels, ...deeper];
      }

      calcOrderBookTotal(orderBook);
      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
          const payload = JSON.stringify({ op: 'subscribe', args: [topic] });
          this.send(payload);

          // store subscribed topic to re-subscribe on reconnect
          this.topics[handler] = [topic];
          this.messageHandlers[secondaryHandler] = [...(this.messageHandlers[secondaryHandler] || []), secondaryParser];

          const secondaryPayload = JSON.stringify({ op: 'subscribe', args: [secondaryTopic] });
          this.send(secondaryPayload);

          // store subscribed topic to re-subscribe on reconnect
          this.topics[secondaryHandler] = [secondaryTopic];
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];

      if (!this.messageHandlers[handler].length) {
        delete this.topics[handler];
        this.cleanupMessageHandlerStats(handler);
      }

      this.messageHandlers[secondaryHandler] = [
        ...(this.messageHandlers[secondaryHandler] || []).filter((f) => f !== secondaryParser),
      ];

      if (!this.messageHandlers[secondaryHandler].length) {
        delete this.topics[secondaryHandler];
        this.cleanupMessageHandlerStats(secondaryHandler);
      }

      orderBook.bids = [];
      orderBook.asks = [];

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (this.isConnected && !this.messageHandlers[handler].length) {
        const payload = { op: 'unsubscribe', args: [topic] };
        this.send(JSON.stringify(payload));
      }

      if (this.isConnected && !this.messageHandlers[secondaryHandler].length) {
        const payload = { op: 'unsubscribe', args: [secondaryTopic] };
        this.send(JSON.stringify(payload));
      }
    };
  };

  override dispose() {
    if (this.connectionFailureNoticeTimeout) {
      clearTimeout(this.connectionFailureNoticeTimeout);
      this.connectionFailureNoticeTimeout = undefined;
    }
    this.parent.emitter.off('markets:changed', this.onMarketsChanged);
    super.dispose();
  }
}
