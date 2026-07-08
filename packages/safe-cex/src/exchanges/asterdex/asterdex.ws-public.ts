import type { Store } from '../../store/store.interface';
import type { Candle, OrderBook, Ticker, Writable } from '../../types';
import type { AsterDEXExchange } from './asterdex.exchange';
import type { AsterDEXStreamDepth, AsterDEXStreamTrade } from './asterdex.types';

import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { calcOrderBookTotal, sortOrderBook } from '../../utils/orderbook';
import { BaseWebSocket } from '../base.ws';
import { ASTERDEX_ENDPOINTS } from './asterdex.types';

type MessageHandlers = {
  [topic: string]: Array<(data: any) => void>;
};

type SubscribedTopics = {
  [id: string]: string[] | string;
};

export class AsterDEXPublicWebsocket extends BaseWebSocket<AsterDEXExchange> {
  private topics: SubscribedTopics = {};
  private klineCallbacks: Map<string, (candle: Candle) => void> = new Map();
  private orderBookCallbacks: Map<string, (orderBook: OrderBook) => void> = new Map();
  private subscriptionId = 1;

  messageHandlers: MessageHandlers = {
    '24hrTicker': [(d: any) => this.handleTickerUpdate(d)],
    depthUpdate: [(d: any) => this.handleDepthUpdate(d)],
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
    return `${ASTERDEX_ENDPOINTS.WS.PUBLIC}?requestId=${nanoid()}`;
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
      this.ws.send(JSON.stringify({ id: 42, method: 'LIST_SUBSCRIPTIONS' }));
    }
  };

  onMessage = (event: MessageEvent): void => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(event.data);
        if (!json) return;

        // Handle pong response
        if (json.id === 42) {
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

  private handleDepthUpdate(data: AsterDEXStreamDepth): void {
    // Parse raw orderbook data
    const rawBids = (data.b || []).map(([price, qty]) => ({
      price: parseFloat(price),
      amount: parseFloat(qty),
      total: 0,
    }));

    const rawAsks = (data.a || []).map(([price, qty]) => ({
      price: parseFloat(price),
      amount: parseFloat(qty),
      total: 0,
    }));

    // Remove any zero or negative prices first
    const cleanBids = rawBids.filter((bid) => bid.price > 0 && bid.amount > 0);
    const cleanAsks = rawAsks.filter((ask) => ask.price > 0 && ask.amount > 0);

    // Sort the clean data - CRITICAL: must be sorted correctly
    cleanBids.sort((a, b) => b.price - a.price); // Descending (highest first)
    cleanAsks.sort((a, b) => a.price - b.price); // Ascending (lowest first)

    // Find the midpoint where bid < ask is satisfied
    let validBids = [];
    let validAsks = [];

    if (cleanBids.length > 0 && cleanAsks.length > 0) {
      const bestBid = cleanBids[0].price;
      const bestAsk = cleanAsks[0].price;

      if (bestBid < bestAsk) {
        // Normal case: no crossing
        validBids = cleanBids;
        validAsks = cleanAsks;
      } else {
        // Crossed orderbook: need to filter intelligently
        // Algorithm: Remove crossing levels while preserving as much depth as possible

        // Strategy 1: Find the uncrossed region
        // Get the highest bid that's lower than at least one ask
        let maxValidBid = -Infinity;
        for (const bid of cleanBids) {
          if (cleanAsks.some((ask) => bid.price < ask.price)) {
            maxValidBid = Math.max(maxValidBid, bid.price);
            break; // Found the highest valid bid
          }
        }

        // Get the lowest ask that's higher than at least one bid
        let minValidAsk = Infinity;
        for (const ask of cleanAsks) {
          if (cleanBids.some((bid) => bid.price < ask.price)) {
            minValidAsk = Math.min(minValidAsk, ask.price);
            break; // Found the lowest valid ask
          }
        }

        if (maxValidBid !== -Infinity && minValidAsk !== Infinity) {
          // We found valid boundaries
          validBids = cleanBids.filter((bid) => bid.price <= maxValidBid);
          validAsks = cleanAsks.filter((ask) => ask.price >= minValidAsk);
        } else {
          // Strategy 2: Use midpoint splitting as fallback
          const midPrice = (bestBid + bestAsk) / 2;
          validBids = cleanBids.filter((bid) => bid.price < midPrice);
          validAsks = cleanAsks.filter((ask) => ask.price > midPrice);
        }

        // Strategy 3: If still no valid levels, create artificial spread
        if (validBids.length === 0 || validAsks.length === 0) {
          const spread = Math.max(0.01, ((bestBid + bestAsk) / 2) * 0.0001); // 0.01% minimum spread
          const artificialMid = (bestBid + bestAsk) / 2;

          // Take best levels and adjust them
          if (cleanBids.length > 0 && cleanAsks.length > 0) {
            validBids = [
              {
                price: artificialMid - spread / 2,
                amount: cleanBids[0].amount,
                total: 0,
              },
            ];
            validAsks = [
              {
                price: artificialMid + spread / 2,
                amount: cleanAsks[0].amount,
                total: 0,
              },
            ];
            console.log(`[AsterDEX] Created artificial spread for ${data.s} at mid=${artificialMid}`);
          }
        }

        // Log when we had to filter
        if (validBids.length !== cleanBids.length || validAsks.length !== cleanAsks.length) {
          console.log(`[AsterDEX] Filtered crossed orderbook for ${data.s}:`);
          console.log(`  Original: ${cleanBids.length} bids, ${cleanAsks.length} asks`);
          console.log(`  Filtered: ${validBids.length} bids, ${validAsks.length} asks`);
          console.log(`  Best raw: bid=${bestBid}, ask=${bestAsk}`);
          if (validBids.length > 0 && validAsks.length > 0) {
            console.log(`  Best valid: bid=${validBids[0].price}, ask=${validAsks[0].price}`);
          }
        }
      }
    } else {
      // One side is empty
      validBids = cleanBids;
      validAsks = cleanAsks;
    }

    // Create the final orderbook with filtered data
    const orderBook: OrderBook = {
      bids: validBids,
      asks: validAsks,
    };

    // Sort orderbook (should already be sorted, but ensure consistency)
    sortOrderBook(orderBook);

    // Double-check and enforce valid spread after sorting
    if (orderBook.bids.length > 0 && orderBook.asks.length > 0) {
      const bestBidAfterSort = orderBook.bids[0].price;
      const bestAskAfterSort = orderBook.asks[0].price;

      if (bestBidAfterSort >= bestAskAfterSort) {
        // Emergency fix: sortOrderBook might have issues or data was modified
        console.error(
          `[AsterDEX] CRITICAL: Invalid spread after sortOrderBook for ${data.s}: bid=${bestBidAfterSort}, ask=${bestAskAfterSort}`,
        );

        // Remove crossing levels
        const finalMidPrice = (bestBidAfterSort + bestAskAfterSort) / 2;
        orderBook.bids = orderBook.bids.filter((bid) => bid.price < finalMidPrice);
        orderBook.asks = orderBook.asks.filter((ask) => ask.price > finalMidPrice);

        // If we removed everything, create artificial spread
        if (orderBook.bids.length === 0 || orderBook.asks.length === 0) {
          const spread = Math.max(0.01, finalMidPrice * 0.0001); // 0.01% spread minimum
          orderBook.bids = [
            {
              price: finalMidPrice - spread / 2,
              amount: validBids[0]?.amount || 1,
              total: 0,
            },
          ];
          orderBook.asks = [
            {
              price: finalMidPrice + spread / 2,
              amount: validAsks[0]?.amount || 1,
              total: 0,
            },
          ];
          console.log(`[AsterDEX] Emergency: Created artificial spread at ${finalMidPrice} +/- ${spread / 2}`);
        }

        // Re-sort after emergency fix
        sortOrderBook(orderBook);
      }
    }

    // Calculate cumulative totals
    calcOrderBookTotal(orderBook);

    // Update ticker with best bid/ask from filtered orderbook
    // Use O(1) hashmap lookup instead of O(n) find
    const ticker = this.parent.store.tickersBySymbol[data.s];
    if (ticker && orderBook.bids.length > 0 && orderBook.asks.length > 0) {
      const bestBid = orderBook.bids[0].price;
      const bestAsk = orderBook.asks[0].price;

      // Final validation
      if (bestBid < bestAsk) {
        this.parent.store.updateTicker(ticker, {
          bid: bestBid,
          ask: bestAsk,
        });
      } else {
        // This really should never happen now
        console.error(
          `[AsterDEX] IMPOSSIBLE: Still invalid after all fixes for ${data.s}: bid=${bestBid}, ask=${bestAsk}`,
        );
        // Don't update ticker with invalid data
      }
    }

    // Notify callbacks with the filtered orderbook
    const symbol = data.s;
    const symbolLower = symbol.toLowerCase();
    const callbacks = [
      this.orderBookCallbacks.get(symbol),
      this.orderBookCallbacks.get(symbolLower),
      this.orderBookCallbacks.get(`${symbolLower}@depth`),
      this.orderBookCallbacks.get(`${symbolLower}@depth@500ms`),
    ].filter(Boolean);

    if (callbacks.length > 0) {
      // console.log(`Delivering orderbook update for ${symbol} to ${callbacks.length} callbacks`);
    }

    callbacks.forEach((callback) => callback!(orderBook));
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
    let timeoutId: NodeJS.Timeout | null = null;
    const stream = `${symbol.toLowerCase()}@depth@500ms`; // Use depth with 500ms updates

    // console.log(`listenOrderBook called for ${symbol}, stream: ${stream}`);

    // Store the callback with multiple key formats for flexible lookup
    this.orderBookCallbacks.set(symbol, callback); // Original symbol (e.g., BTCUSDT)
    this.orderBookCallbacks.set(symbol.toLowerCase(), callback); // Lowercase symbol
    this.orderBookCallbacks.set(stream, callback); // Full stream name
    this.orderBookCallbacks.set(`${symbol.toLowerCase()}@depth`, callback); // Also map basic depth stream

    // Track subscription in topics immediately for reconnection
    this.topics[stream] = [stream];
    // console.log(`Added ${stream} to topics for reconnection tracking`);

    // Subscribe function
    const doSubscribe = () => {
      if (!this.isDisposed && this.ws && this.ws.readyState === WebSocket.OPEN) {
        // console.log(`Subscribing to orderbook stream: ${stream}`);

        // Send subscription message
        const message = {
          method: 'SUBSCRIBE',
          params: [stream],
          id: this.subscriptionId++,
        };

        const messageStr = JSON.stringify(message);
        // console.log(`Sending orderbook subscription: ${messageStr}`);
        try {
          this.send(messageStr);
          return true; // Success
        } catch {
          // console.log(`Failed to send orderbook subscription`);
          return false; // Failed
        }
      }
      return false; // Not ready
    };

    // Try to subscribe with retries
    const attemptSubscribe = (retryCount = 0) => {
      const maxRetries = 50; // 5 seconds total with 100ms intervals

      // console.log(
      //   `Attempting orderbook subscription (retry ${retryCount}/${maxRetries}), WS state: ${this.ws?.readyState}, Connected: ${this.isConnected}`
      // );

      if (doSubscribe()) {
        // Success!
        // console.log(`Successfully subscribed to orderbook: ${stream}`);
      } else if (retryCount < maxRetries) {
        // Retry in 100ms
        timeoutId = setTimeout(() => attemptSubscribe(retryCount + 1), 100);
      } else {
        // console.log(`Failed to subscribe to orderbook after ${maxRetries} attempts: ${stream}`);
      }
    };

    // Start attempting to subscribe immediately
    attemptSubscribe(0);

    // Return unsubscribe function
    return () => {
      // Clear timeout if still waiting
      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      // Remove from pending if queued
      const index = this.pendingSubscriptions.indexOf(doSubscribe);
      if (index > -1) {
        this.pendingSubscriptions.splice(index, 1);
      }

      // Remove callbacks
      this.orderBookCallbacks.delete(symbol);
      this.orderBookCallbacks.delete(symbol.toLowerCase());
      this.orderBookCallbacks.delete(stream);
      this.orderBookCallbacks.delete(`${symbol.toLowerCase()}@depth`);

      // Remove from topics
      delete this.topics[stream];

      // Cleanup message handler stats
      this.cleanupMessageHandlerStats(stream);

      // Unsubscribe if connected
      if (this.isConnected && !this.isDisposed) {
        const message = {
          method: 'UNSUBSCRIBE',
          params: [stream],
          id: this.subscriptionId++,
        };

        this.send(JSON.stringify(message));
      }
    };
  };

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
    this.orderBookCallbacks.clear();
    this.topics = {};
    this.pendingSubscriptions = [];
    this.wsReady = false;
    // Call parent dispose (regular prototype method, so super works here)
    super.dispose();
  };
}
