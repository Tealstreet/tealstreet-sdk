import type { Store } from '../../../store/store.interface';
import type { Candle, OrderBook, Ticker, Writable } from '../../../types';
import type { AsterDEXExchange } from './asterdex.exchange';
import type { AsterDEXStreamDepth, AsterDEXStreamTrade } from '../../../exchanges/asterdex/asterdex.types';

import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { calcOrderBookTotal } from '../../../utils/orderbook';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { ASTERDEX_ENDPOINTS } from '../../../exchanges/asterdex/asterdex.types';

type MessageHandlers = {
  [topic: string]: Array<(data: any) => void>;
};

type SubscribedTopics = {
  [id: string]: string[] | string;
};

type OrderBookSubscription = {
  callbacks: Map<number, (orderBook: OrderBook) => void>;
  retryTimer: NodeJS.Timeout | null;
};

const PARTIAL_DEPTH_STREAM = /@depth(?:5|10|20)(?:@(100|250|500)ms)?$/;

export class AsterDEXPublicWebsocket extends BaseWebSocket<AsterDEXExchange> {
  private static readonly PING_REQUEST_ID = -1;
  private topics: SubscribedTopics = {};
  private klineCallbacks: Map<string, (candle: Candle) => void> = new Map();
  private orderBookSubscriptions: Map<string, OrderBookSubscription> = new Map();
  private subscriptionId = 1;
  private orderBookListenerId = 1;

  messageHandlers: MessageHandlers = {
    '24hrTicker': [(d: any) => this.handleTickerUpdate(d)],
    aggTrade: [(d: any) => this.handleTradeUpdate(d)],
    markPriceUpdate: [(d: any) => this.handleMarkPriceUpdate(d)],
    kline: [(d: any) => this.handleKlineUpdate(d)],
  };

  constructor(parent: AsterDEXExchange, _store: Store) {
    super(parent);

    // BaseWebSocket.onClose is an instance arrow-function field (not a
    // prototype method), so retain the base instance handler and wrap it.
    const baseOnClose = this.onClose;
    this.onClose = (event?: any): void => {
      // console.log('AsterDEX public WebSocket connection closed');

      // Reset initialization state on close but keep topics for reconnection
      this.wsReady = false;
      this.pendingSubscriptions = [];
      // DON'T clear topics here - we need them for reconnection!

      baseOnClose(event);
    };
  }

  get connectionURL(): string {
    // Use the combined stream endpoint
    const baseUrl = this.parent.options.testnet
      ? ASTERDEX_ENDPOINTS.WS.PUBLIC.TESTNET
      : ASTERDEX_ENDPOINTS.WS.PUBLIC.PRODUCTION;
    return `${baseUrl}?requestId=${nanoid()}`;
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      // console.log(`Connecting to AsterDEX public WebSocket: ${this.connectionURL}`);

      // Reset state when reconnecting
      this.wsReady = false;

      try {
        this.ws = this.createWebsocket(this.connectionURL);

        this.ws.addEventListener('open', this.onOpen);
        this.ws.addEventListener('message', this.onMessage);
        this.ws.addEventListener('close', this.onClose);
        this.ws.addEventListener('error', () => {
          // console.log(`WebSocket error occurred`);
          this.onClose();
        });
      } catch {
        // console.log(`Failed to create WebSocket`);
      }
    }
  };

  private pendingSubscriptions: Array<() => void> = [];
  private wsReady = false;

  onOpen = () => {
    if (!this.isDisposed) {
      // console.log('AsterDEX public WebSocket connected');

      // Send initial ping to establish pong tracking
      this.ping();

      // Process subscriptions asynchronously to not block
      (async () => {
        try {
          // Subscribe to aggregate streams only to avoid overwhelming the WebSocket
          // Individual ticker subscriptions were causing disconnections
          const aggStreams = [
            '!ticker@arr', // All tickers at once (replaces individual ticker streams)
            '!markPrice@arr@1s', // All mark prices
          ];

          // Subscribe to aggregate streams
          // console.log('Subscribing to aggregate streams');
          await this.subscribe(aggStreams);

          if (ENABLE_RECONNECTING_WEBSOCKET) {
            this.emitWsReconnected();
          }

          // A connection opening will resubscribe every active topic below, so
          // cancel per-topic wait retries to avoid duplicate SUBSCRIBEs.
          this.clearOrderBookSubscriptionRetries();

          // Resubscribe to all streams from topics (including depth, kline, etc)
          const allStreams = Object.keys(this.topics).filter(
            (topic) => !topic.startsWith('!') && topic.includes('@') && Array.isArray(this.topics[topic]),
          );

          if (allStreams.length > 0) {
            // console.log(`Resubscribing to ${allStreams.length} streams from topics: ${allStreams.join(', ')}`);
            // Subscribe to all streams in one batch
            await this.subscribe(allStreams).catch(() => {
              // console.log(`Failed to resubscribe to streams`);
            });
          }
        } catch {
          // console.log(`Error in onOpen`);
        } finally {
          // Mark as initialized
          this.wsReady = true;
        }
      })();
    }
  };

  private async subscribe(streams: string[]): Promise<void> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      // console.log('WebSocket not ready for subscription');
      return;
    }

    const message = {
      method: 'SUBSCRIBE',
      params: streams,
      id: this.subscriptionId++,
    };

    const messageStr = JSON.stringify(message);
    // console.log(`Sending subscription: ${messageStr}`);
    this.send(messageStr);

    // Track subscribed topics
    // For depth and kline streams, we store the full stream name as the key
    streams.forEach((stream) => {
      // Store the full stream as a key for proper resubscription
      if (stream.includes('@depth') || stream.includes('@kline')) {
        this.topics[stream] = [stream];
      } else {
        // For other streams, use the old logic
        const [symbol, type] = stream.split('@');
        if (!this.topics[type]) {
          this.topics[type] = [];
        }
        if (Array.isArray(this.topics[type])) {
          (this.topics[type] as string[]).push(symbol);
        }
      }
    });
  }

  private async unsubscribe(streams: string[]): Promise<void> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return;
    }

    const message = {
      method: 'UNSUBSCRIBE',
      params: streams,
      id: this.subscriptionId++,
    };

    this.send(JSON.stringify(message));
  }

  ping = (): void => {
    // AsterDEX (like Binance) supports application-level ping via LIST_SUBSCRIPTIONS
    if (!this.isDisposed && this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.pingAt = performance.now();
      this.ws.send(JSON.stringify({ id: AsterDEXPublicWebsocket.PING_REQUEST_ID, method: 'LIST_SUBSCRIPTIONS' }));
    }
  };

  onMessage = (event: MessageEvent): void => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(event.data);
        if (!json) return;

        // Handle pong response
        if (json.id === AsterDEXPublicWebsocket.PING_REQUEST_ID) {
          this.handlePongEvent();
          return;
        }

        // Handle subscription response
        if (json.id !== undefined && json.result === null) {
          return;
        }

        // Handle error response
        if (json.error) {
          // console.log(`WebSocket error: ${JSON.stringify(json.error)}`);
          return;
        }

        // Handle stream data with array payload (like !markPrice@arr or !ticker@arr)
        if (json.stream && json.data) {
          // Check if it's an array stream (markPrice or ticker)
          if (json.stream === '!markPrice@arr@1s' || json.stream === '!markPrice@arr') {
            // Handle array of mark price updates — for-loop avoids the
            // per-event closure allocation of forEach on a 600-entry array.
            if (Array.isArray(json.data)) {
              for (let i = 0; i < json.data.length; i++) {
                this.handleMarkPriceUpdate(json.data[i]);
              }
            }
          } else if (json.stream === '!ticker@arr') {
            // Handle array of ticker updates
            if (Array.isArray(json.data)) {
              for (let i = 0; i < json.data.length; i++) {
                this.handleTickerUpdate(json.data[i]);
              }
            }
          } else {
            // Handle other stream data
            this.handleStreamData(json.stream, json.data);
          }
        } else if (Array.isArray(json)) {
          // Handle batch updates (legacy format)
          for (let i = 0; i < json.length; i++) {
            this.handleEventData(json[i]);
          }
        } else if (json.e) {
          // Handle single event
          this.handleEventData(json);
        }
      } catch {
        // console.log(`Failed to process message`);
      }
    }
  };

  private handleStreamData(stream: string, data: any): void {
    const [symbol, streamType] = stream.split('@');

    // Map stream type to event type
    const eventTypeMap: Record<string, string> = {
      ticker: '24hrTicker',
      depth: 'depthUpdate',
      depth20: 'depthUpdate', // Handle depth20 specifically
      depth10: 'depthUpdate', // Handle depth10 specifically
      depth5: 'depthUpdate', // Handle depth5 specifically
      aggTrade: 'aggTrade',
      markPrice: 'markPriceUpdate',
      kline: 'kline',
    };

    const eventType = streamType.split('_')[0]; // Handle kline_1m, etc
    const mappedType = eventTypeMap[eventType];

    // Aster's bare @depth stream is incremental. Only its explicitly named
    // partial-depth streams are snapshots that can be handed to consumers as
    // a complete book.
    if (mappedType === 'depthUpdate') {
      if (PARTIAL_DEPTH_STREAM.test(stream)) {
        if (!data.s) data.s = symbol.toUpperCase();
        this.handleDepthSnapshot(data);
      }
      return;
    }

    if (mappedType && this.messageHandlers[mappedType]) {
      // Add symbol to data if not present
      if (!data.s) {
        data.s = symbol.toUpperCase();
      }

      this.recordMessageHandlerStats(mappedType);
      this.messageHandlers[mappedType].forEach((handler) => handler(data));
    } else if (eventType === 'kline') {
      // console.log(`No handler found for kline event`);
    }
  }

  private handleEventData(data: any): void {
    const eventType = data.e;

    if (eventType && this.messageHandlers[eventType]) {
      this.recordMessageHandlerStats(eventType);
      this.messageHandlers[eventType].forEach((handler) => handler(data));
    }
  }

  private handleTickerUpdate(data: any): void {
    // Use O(1) hashmap lookup instead of O(n) find
    const ticker = this.parent.store.tickersBySymbol[data.s];
    if (!ticker) return;

    const update: Partial<Writable<Ticker>> = {};

    // 24hr ticker data doesn't include bid/ask - those come from orderbook depth
    // Only update the fields that are actually in the ticker stream
    if (data.c) update.last = parseFloat(data.c); // Last price
    if (data.P) update.percentage = parseFloat(data.P); // Price change percent
    if (data.v) update.volume = parseFloat(data.v); // Volume
    if (data.q) update.quoteVolume = parseFloat(data.q); // Quote volume

    this.parent.store.updateTicker(ticker, update);
    this.parent.emitter.emit('ticker', data.s, ticker);
  }

  private handleDepthSnapshot(data: AsterDEXStreamDepth): void {
    const parseLevels = (levels: unknown) => {
      if (!Array.isArray(levels)) return undefined;
      const parsed = [] as OrderBook['bids'];
      for (let i = 0; i < levels.length; i++) {
        const level = levels[i];
        if (
          !Array.isArray(level) ||
          level.length !== 2 ||
          typeof level[0] !== 'string' ||
          typeof level[1] !== 'string' ||
          level[0].trim() === '' ||
          level[1].trim() === ''
        ) {
          return undefined;
        }
        const [rawPrice, rawAmount] = level;
        // Number() rejects malformed prefixes (for example "100junk") that
        // parseFloat() would silently accept as authoritative market data. The
        // documented partial-book payload is a pair of strings, never coerced
        // booleans, arrays, objects, or numeric JSON values.
        const price = Number(rawPrice);
        const amount = Number(rawAmount);
        // Zero quantities are valid only for diff updates. A partial snapshot
        // containing one is not authoritative, so discard the whole frame.
        if (!Number.isFinite(price) || !Number.isFinite(amount) || price <= 0 || amount <= 0) return undefined;
        parsed.push({ price, amount, total: 0 });
      }
      return parsed;
    };

    const bids = parseLevels(data.b || []);
    const asks = parseLevels(data.a || []);
    if (!bids || !asks || bids.length === 0 || asks.length === 0) return;

    bids.sort((a, b) => b.price - a.price);
    asks.sort((a, b) => a.price - b.price);

    // Do not manufacture a spread or trim levels. A malformed/crossed
    // snapshot is unsafe market data and must not replace the last good book.
    if (bids[0].price >= asks[0].price) return;

    const orderBook: OrderBook = { bids, asks };

    // Calculate cumulative totals
    calcOrderBookTotal(orderBook);

    // Update ticker with best bid/ask from the authoritative snapshot.
    const ticker = this.parent.store.tickersBySymbol[data.s];
    if (ticker) {
      this.parent.store.updateTicker(ticker, {
        bid: orderBook.bids[0].price,
        ask: orderBook.asks[0].price,
      });
    }

    const subscription = this.orderBookSubscriptions.get(`${data.s.toLowerCase()}@depth20@100ms`);
    if (!subscription) return;

    // Callbacks are consumer-owned. Give each listener its own authoritative
    // snapshot so a mutation in one callback cannot corrupt another listener.
    for (const [listenerId, callback] of Array.from(subscription.callbacks)) {
      if (!subscription.callbacks.has(listenerId)) continue;
      callback({
        bids: orderBook.bids.map((level) => ({ ...level })),
        asks: orderBook.asks.map((level) => ({ ...level })),
      });
    }
  }

  private handleTradeUpdate(data: AsterDEXStreamTrade): void {
    // Use O(1) hashmap lookup instead of O(n) find
    const ticker = this.parent.store.tickersBySymbol[data.s];
    if (ticker) {
      this.parent.store.updateTicker(ticker, {
        last: parseFloat(data.p),
      });
    }

    this.parent.emitter.emit('trade', {
      symbol: data.s,
      price: parseFloat(data.p),
      qty: parseFloat(data.q),
      timestamp: data.T,
      side: data.m ? 'SELL' : 'BUY',
    });
  }

  private handleMarkPriceUpdate(data: any): void {
    // Handle both stream format and array format
    // Use O(1) hashmap lookup instead of O(n) find
    const ticker = this.parent.store.tickersBySymbol[data.s];
    if (!ticker) return;

    const update: Partial<Writable<Ticker>> = {};

    if (data.p) update.mark = parseFloat(data.p);
    if (data.i) update.index = parseFloat(data.i);
    if (data.r !== undefined && data.r !== null && data.r !== '') {
      update.fundingRate = parseFloat(data.r);
    }
    if (data.T) update.fundingTime = data.T;

    this.parent.store.updateTicker(ticker, update);
    this.parent.emitter.emit('markPrice', data.s, ticker);
  }

  private handleKlineUpdate(data: any): void {
    if (!data.k) return;

    const kline = data.k;
    // Ensure symbol is lowercase for stream matching
    const symbol = (data.s || kline.s || '').toLowerCase();
    const stream = `${symbol}@kline_${kline.i}`;
    const callback = this.klineCallbacks.get(stream);

    if (callback) {
      const candle: Candle = {
        timestamp: kline.t,
        open: parseFloat(kline.o),
        high: parseFloat(kline.h),
        low: parseFloat(kline.l),
        close: parseFloat(kline.c),
        volume: parseFloat(kline.v),
      };

      callback(candle);
    }
  }

  // Public method for orderbook subscription (called by exchange class)
  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void): (() => void) => {
    const stream = `${symbol.toLowerCase()}@depth20@100ms`;
    const listenerId = this.orderBookListenerId++;
    const existingSubscription = this.orderBookSubscriptions.get(stream);

    if (existingSubscription) {
      existingSubscription.callbacks.set(listenerId, callback);
      return () => this.unsubscribeOrderBookListener(stream, listenerId);
    }

    const subscription: OrderBookSubscription = {
      callbacks: new Map([[listenerId, callback]]),
      retryTimer: null,
    };
    this.orderBookSubscriptions.set(stream, subscription);

    // Track subscription in topics immediately for reconnection
    this.topics[stream] = [stream];
    this.subscribeOrderBook(stream, subscription);

    return () => this.unsubscribeOrderBookListener(stream, listenerId);
  };

  private subscribeOrderBook(stream: string, subscription: OrderBookSubscription, retryCount = 0): void {
    if (this.orderBookSubscriptions.get(stream) !== subscription || subscription.callbacks.size === 0) return;

    if (!this.isDisposed && this.ws && this.ws.readyState === WebSocket.OPEN) {
      try {
        this.send(
          JSON.stringify({
            method: 'SUBSCRIBE',
            params: [stream],
            id: this.subscriptionId++,
          }),
        );
        return;
      } catch {
        // Retry below while listeners remain registered.
      }
    }

    if (retryCount < 50) {
      subscription.retryTimer = setTimeout(() => this.subscribeOrderBook(stream, subscription, retryCount + 1), 100);
    }
  }

  private unsubscribeOrderBookListener(stream: string, listenerId: number): void {
    const subscription = this.orderBookSubscriptions.get(stream);
    if (!subscription || !subscription.callbacks.delete(listenerId)) return;
    if (subscription.callbacks.size > 0) return;

    if (subscription.retryTimer) {
      clearTimeout(subscription.retryTimer);
    }
    this.orderBookSubscriptions.delete(stream);
    delete this.topics[stream];
    this.cleanupMessageHandlerStats(stream);

    if (!this.isDisposed) {
      void this.unsubscribe([stream]);
    }
  }

  private clearOrderBookSubscriptionRetries(): void {
    for (const subscription of this.orderBookSubscriptions.values()) {
      if (subscription.retryTimer) {
        clearTimeout(subscription.retryTimer);
        subscription.retryTimer = null;
      }
    }
  }

  // Public methods for subscribing to specific data streams
  subscribeToKline(stream: string, callback: (candle: Candle) => void): void {
    let timeoutId: NodeJS.Timeout | null = null;
    this.klineCallbacks.set(stream, callback);

    // Subscribe function
    const doSubscribe = () => {
      if (!this.isDisposed) {
        // Track subscription in topics
        this.topics[stream] = [stream];

        // Send subscription message
        const message = {
          method: 'SUBSCRIBE',
          params: [stream],
          id: this.subscriptionId++,
        };

        this.send(JSON.stringify(message));
      }
    };

    // Wait for connection and initialization
    const waitForConnectedAndSubscribe = (retryCount = 0) => {
      if (this.isConnected && this.wsReady) {
        // Connection is ready, subscribe immediately
        doSubscribe();
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      } else if (this.isConnected && !this.wsReady) {
        // Connected but still initializing, queue the subscription
        this.pendingSubscriptions.push(doSubscribe);
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      } else {
        // Not connected yet, retry with backoff
        const delay = Math.min(100 * Math.pow(1.5, Math.floor(retryCount / 10)), 2000);
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(retryCount + 1), delay);
      }
    };

    waitForConnectedAndSubscribe(0);
  }

  unsubscribeFromKline(stream: string): void {
    this.klineCallbacks.delete(stream);
    delete this.topics[stream];
    this.cleanupMessageHandlerStats(stream);
    this.unsubscribe([stream]).catch(() => {
      // console.log(`Failed to unsubscribe from kline`);
    });
  }

  dispose = (): void => {
    this.klineCallbacks.clear();
    this.clearOrderBookSubscriptionRetries();
    this.orderBookSubscriptions.clear();
    this.topics = {};
    this.pendingSubscriptions = [];
    this.wsReady = false;
    // Call parent dispose (regular prototype method, so super works here)
    super.dispose();
  };
}
