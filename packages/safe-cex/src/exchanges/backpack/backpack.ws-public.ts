import throttle from 'lodash/throttle';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Candle, OHLCVOptions, OrderBook, Ticker } from '../../types';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { updateOrderBook } from '../../utils/updateOrderBook';
import { BaseWebSocket } from '../base.ws';
import { BackpackExchange } from './backpack.exchange';
import {
  BackpackWsBookTickerUpdate,
  BackpackWsDepthUpdate,
  BackpackWsKlineUpdate,
  BackpackWsTickerUpdate,
  BackpackWsTradeUpdate,
  ENDPOINTS,
  WSS_URLS,
} from './backpack.types';

// State for orderbook snapshot + delta handling
interface OrderBookState {
  updates: BackpackWsDepthUpdate[];
  isSnapshotLoaded: boolean;
  lastUpdateId: number;
}

export class BackpackPublicWebSocket extends BaseWebSocket<BackpackExchange> {
  private subscriptions: Set<string> = new Set();
  private topics: Record<string, number> = {};
  private orderBookState: Record<string, OrderBookState> = {};
  // Store snapshot fetch functions to refetch on reconnect
  private orderBookSnapshotFetchers: Record<string, () => Promise<void>> = {};

  constructor(parent: BackpackExchange) {
    super(parent);
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const url = WSS_URLS[this.parent.options.testnet ? 'TESTNET' : 'PRODUCTION'];
      this.ws = this.createWebsocket(url);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.resetAndRefetchOrderBooks();
      this.resubscribe();
      this.ping();
    }
  };

  /**
   * Ping implementation for Backpack WebSocket
   * Send a ping message to keep the connection alive and measure latency.
   * Note: Backpack responds to PING with error code 4002 (parse error),
   * which we use as a pong response to measure latency.
   */
  ping = () => {
    if (!this.isDisposed && this.isConnected) {
      this.pingAt = performance.now();
      // Send ping message - Backpack responds with error 4002, which we use as pong
      this.send(JSON.stringify({ method: 'PING' }));
    }
  };

  /**
   * Reset orderbook state and refetch snapshots on reconnect
   */
  private resetAndRefetchOrderBooks = () => {
    // Reset all orderbook states to force re-buffering of updates
    Object.keys(this.orderBookState).forEach((marketId) => {
      this.orderBookState[marketId] = {
        updates: [],
        isSnapshotLoaded: false,
        lastUpdateId: 0,
      };
    });

    // Refetch snapshots for all active orderbook subscriptions
    Object.keys(this.orderBookSnapshotFetchers).forEach((marketId) => {
      const fetcher = this.orderBookSnapshotFetchers[marketId];
      if (fetcher) {
        fetcher();
      }
    });
  };

  private resubscribe = () => {
    if (this.subscriptions.size > 0) {
      this.send(
        JSON.stringify({
          method: 'SUBSCRIBE',
          params: Array.from(this.subscriptions),
        }),
      );
    }
  };

  subscribeToOrderBook = (symbol: string) => {
    const stream = `depth.${symbol}`;
    if (!this.subscriptions.has(stream)) {
      this.subscriptions.add(stream);
      this.send(
        JSON.stringify({
          method: 'SUBSCRIBE',
          params: [stream],
        }),
      );
    }
  };

  subscribeToTicker = (symbol: string) => {
    const stream = `ticker.${symbol}`;
    if (!this.subscriptions.has(stream)) {
      this.subscriptions.add(stream);
      this.send(
        JSON.stringify({
          method: 'SUBSCRIBE',
          params: [stream],
        }),
      );
    }
  };

  subscribeToTrades = (symbol: string) => {
    const stream = `trade.${symbol}`;
    if (!this.subscriptions.has(stream)) {
      this.subscriptions.add(stream);
      this.send(
        JSON.stringify({
          method: 'SUBSCRIBE',
          params: [stream],
        }),
      );
    }
  };

  subscribeToBookTicker = (symbol: string) => {
    const stream = `bookTicker.${symbol}`;
    if (!this.subscriptions.has(stream)) {
      this.subscriptions.add(stream);
      this.send(
        JSON.stringify({
          method: 'SUBSCRIBE',
          params: [stream],
        }),
      );
    }
  };

  subscribeToKline = (symbol: string, interval: string) => {
    const stream = `kline.${interval}.${symbol}`;
    if (!this.subscriptions.has(stream)) {
      this.subscriptions.add(stream);
      this.send(
        JSON.stringify({
          method: 'SUBSCRIBE',
          params: [stream],
        }),
      );
    }
  };

  unsubscribeFromKline = (symbol: string, interval: string) => {
    const stream = `kline.${interval}.${symbol}`;
    this.unsubscribe([stream]);
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      try {
        const json = jsonParse(data);
        if (!json) return;

        // Handle subscription confirmations
        if (json.result === null && json.id) {
          return; // Subscription confirmation
        }

        // Handle error responses
        if (json.error) {
          // Parse errors (code 4002) come from our PING messages - use as pong response
          if (json.error.code === 4002) {
            this.handlePongEvent(10_000);
            return;
          }
          this.parent.emitAndLogError(`WebSocket error: ${JSON.stringify(json.error)}`);
          return;
        }

        // Messages come wrapped: {"data": {...}, "stream": "..."}
        const eventData = json.data || json;
        const stream = json.stream as string | undefined;
        if (!eventData.e) return;

        const eventType = eventData.e;

        switch (eventType) {
          case 'depth':
            this.handleDepthUpdate(eventData as BackpackWsDepthUpdate);
            break;

          case 'ticker':
            this.handleTickerUpdate(eventData as BackpackWsTickerUpdate);
            break;

          case 'trade':
            this.handleTradeUpdate(eventData as BackpackWsTradeUpdate);
            break;

          case 'bookTicker':
            this.handleBookTickerUpdate(eventData as BackpackWsBookTickerUpdate);
            break;

          case 'kline':
            this.handleKlineUpdate(eventData as BackpackWsKlineUpdate, stream);
            break;

          default:
            // console.log('Unhandled event type:', eventType, eventData);
            break;
        }
      } catch (error) {
        this.parent.emitAndLogError(`Error parsing WebSocket message: ${error}`);
      }
    }
  };

  private handleDepthUpdate = throttle(
    (data: BackpackWsDepthUpdate) => {
      try {
        // Backpack market id == symbol (e.g. BTC_USDC_PERP), so a single
        // tickersBySymbol hashmap lookup replaces the prior O(N) markets.find
        // + O(N) tickers.find pair.
        const ticker = this.store.tickersBySymbol[data.s];

        // Call registered orderbook handlers
        const topic = `depth.${data.s}`;
        const handlers = this.messageHandlers[topic];
        if (handlers && handlers.length > 0) {
          this.recordMessageHandlerStats(topic);
          handlers.forEach((handler) => handler(data));
        }

        // Update best bid/ask in ticker if available and valid
        if (ticker && (data.b.length > 0 || data.a.length > 0)) {
          const bid = data.b.length > 0 ? parseFloat(data.b[0][0]) : NaN;
          const ask = data.a.length > 0 ? parseFloat(data.a[0][0]) : NaN;

          const update: Partial<Ticker> = {
            ...(!Number.isNaN(bid) && { bid }),
            ...(!Number.isNaN(ask) && { ask }),
          };

          if (Object.keys(update).length > 0) {
            this.store.updateTicker(ticker, update);
          }
        }
      } catch (error) {
        this.parent.emitAndLogError(`Error handling depth update: ${error}`);
      }
    },
    100,
    { leading: true, trailing: true },
  );

  private handleTickerUpdate = throttle(
    (data: BackpackWsTickerUpdate) => {
      try {
        const ticker = this.store.tickersBySymbol[data.s];
        if (!ticker) return;

        // Only include fields that parse to valid numbers to avoid overwriting good data with NaN
        const last = parseFloat(data.c);
        const percentage = parseFloat(data.P);
        const volume = parseFloat(data.v);
        const quoteVolume = parseFloat(data.q);

        const update: Partial<Ticker> = {
          ...(!Number.isNaN(last) && { last }),
          ...(!Number.isNaN(percentage) && { percentage: percentage * 100 }),
          ...(!Number.isNaN(volume) && { volume }),
          ...(!Number.isNaN(quoteVolume) && { quoteVolume }),
        };

        if (Object.keys(update).length > 0) {
          this.store.updateTicker(ticker, update);
        }
      } catch (error) {
        this.parent.emitAndLogError(`Error handling ticker update: ${error}`);
      }
    },
    1000,
    { leading: true, trailing: true },
  );

  private handleTradeUpdate = (data: BackpackWsTradeUpdate) => {
    try {
      const ticker = this.store.tickersBySymbol[data.s];
      if (!ticker) return;

      // Update last price from trade, only if valid
      const last = parseFloat(data.p);
      if (!Number.isNaN(last)) {
        this.store.updateTicker(ticker, { last });
      }
    } catch (error) {
      this.parent.emitAndLogError(`Error handling trade update: ${error}`);
    }
  };

  private handleBookTickerUpdate = (data: BackpackWsBookTickerUpdate) => {
    try {
      const existingTicker = this.store.tickersBySymbol[data.s];
      if (!existingTicker) return;

      const bid = parseFloat(data.b);
      const ask = parseFloat(data.a);

      const update: Partial<Ticker> = {
        ...(!Number.isNaN(bid) && { bid }),
        ...(!Number.isNaN(ask) && { ask }),
      };

      if (Object.keys(update).length > 0) {
        this.store.updateTicker(existingTicker, update);
      }
    } catch (error) {
      this.parent.emitAndLogError(`Error handling book ticker update: ${error}`);
    }
  };

  private handleKlineUpdate = (data: BackpackWsKlineUpdate, stream?: string) => {
    try {
      // Stream format: "kline.1m.BTC_USDC_PERP" - use it directly as topic
      // Fall back to constructing from data if stream not available
      const topic = stream || `kline.${data.i}.${data.s}`;
      const handlers = this.messageHandlers[topic];

      if (handlers && handlers.length > 0) {
        // Skip updates with null values (sometimes sent on candle close)
        if (data.o === null || data.c === null) return;

        this.recordMessageHandlerStats(topic);
        handlers.forEach((handler) => handler(data));
      }
    } catch (error) {
      this.parent.emitAndLogError(`Error handling kline update: ${error}`);
    }
  };

  /**
   * Listen to OHLCV candles for a specific market and interval
   */
  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const market = this.parent.store.markets.find((m) => m.symbol === opts.symbol);
    if (!market) {
      this.parent.emitAndLogError(`Market with symbol ${opts.symbol} not found`);
      return () => {};
    }

    const marketId = market.id;
    const interval = opts.interval;
    const topic = `kline.${interval}.${marketId}`;
    const handler = topic;

    const parser = (data: BackpackWsKlineUpdate) => {
      // Only check symbol - interval is already filtered by the topic subscription
      // Skip updates with null values (sometimes sent on candle close)
      if (
        data.s !== marketId ||
        data.o === null ||
        data.h === null ||
        data.l === null ||
        data.c === null ||
        data.v === null
      ) {
        return;
      }

      callback({
        // Parse ISO date string to timestamp (e.g., "2025-12-15T19:02:00" -> ms)
        timestamp: new Date(data.t + 'Z').getTime(),
        open: parseFloat(data.o),
        high: parseFloat(data.h),
        low: parseFloat(data.l),
        close: parseFloat(data.c),
        volume: parseFloat(data.v),
      });
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.subscribeToKline(marketId, interval);
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        this.unsubscribeFromKline(marketId, interval);
      }
    };
  };

  /**
   * Listen to orderbook updates for a specific market
   * Fetches initial snapshot via REST, then applies WebSocket delta updates
   */
  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    const market = this.parent.store.markets.find((m) => m.symbol === symbol);
    if (!market) {
      this.parent.emitAndLogError(`Market with symbol ${symbol} not found`);
      return () => {};
    }

    const marketId = market.id;
    const topic = `depth.${marketId}`;
    const handler = topic;

    // Initialize state for this subscription
    if (!this.orderBookState[marketId]) {
      this.orderBookState[marketId] = {
        updates: [],
        isSnapshotLoaded: false,
        lastUpdateId: 0,
      };
    }

    // Maintain orderbook state for this subscription
    const orderBook: OrderBook = { bids: [], asks: [] };

    // Fetch initial snapshot from REST API
    const fetchSnapshot = async () => {
      try {
        const { data } = await this.parent.xhr.get(ENDPOINTS.DEPTH, {
          params: { symbol: marketId },
        });

        if (this.isDisposed) return;

        // Populate orderbook from snapshot
        orderBook.bids = data.bids.map(([price, amount]: [string, string]) => ({
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        }));

        orderBook.asks = data.asks.map(([price, amount]: [string, string]) => ({
          price: parseFloat(price),
          amount: parseFloat(amount),
          total: 0,
        }));

        // Get lastUpdateId from snapshot (if available)
        const lastUpdateId = data.lastUpdateId || 0;

        // Drop any buffered updates that are older than the snapshot
        this.orderBookState[marketId].updates = this.orderBookState[marketId].updates.filter(
          (update) => (update.u || 0) > lastUpdateId,
        );

        // Apply any buffered updates that came while loading snapshot
        this.orderBookState[marketId].updates.forEach((update) => {
          this.applyOrderBookUpdate(orderBook, update);
        });

        calcOrderBookTotal(orderBook);

        this.orderBookState[marketId].isSnapshotLoaded = true;
        this.orderBookState[marketId].lastUpdateId = lastUpdateId;
        this.orderBookState[marketId].updates = [];

        callback(orderBook);
      } catch (error) {
        this.parent.emitAndLogError(`Error fetching orderbook snapshot: ${error}`);
      }
    };

    const parser = (data: BackpackWsDepthUpdate) => {
      if (data.s !== marketId) return;

      // If snapshot not loaded yet, buffer the update
      if (!this.orderBookState[marketId].isSnapshotLoaded) {
        this.orderBookState[marketId].updates.push(data);
        return;
      }

      // Apply the update to the orderbook
      this.applyOrderBookUpdate(orderBook, data);

      calcOrderBookTotal(orderBook);

      callback(orderBook);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []), parser];
        this.topics[topic] = (this.topics[topic] || 0) + 1;
        this.subscribeToOrderBook(marketId);

        // Store the fetch function for reconnection handling
        this.orderBookSnapshotFetchers[marketId] = fetchSnapshot;

        // Fetch snapshot after subscribing to avoid missing updates
        fetchSnapshot();
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[handler] = [...(this.messageHandlers[handler] || []).filter((f) => f !== parser)];
      this.topics[topic]--;

      if (this.isConnected && !this.messageHandlers[handler]?.length) {
        this.cleanupMessageHandlerStats(handler);
        this.unsubscribe([topic]);
        // Clean up state
        delete this.orderBookState[marketId];
        delete this.orderBookSnapshotFetchers[marketId];
      }
    };
  };

  /**
   * Apply a depth update to the orderbook
   */
  private applyOrderBookUpdate = (orderBook: OrderBook, data: BackpackWsDepthUpdate) => {
    // Indexed for-loops avoid the per-level destructuring closure of forEach.
    const bids = data.b;
    if (bids && bids.length > 0) {
      for (let i = 0; i < bids.length; i++) {
        const o = bids[i];
        updateOrderBook(orderBook, 'bids', {
          price: parseFloat(o[0]),
          amount: parseFloat(o[1]),
          total: 0,
        });
      }
    }
    const asks = data.a;
    if (asks && asks.length > 0) {
      for (let i = 0; i < asks.length; i++) {
        const o = asks[i];
        updateOrderBook(orderBook, 'asks', {
          price: parseFloat(o[0]),
          amount: parseFloat(o[1]),
          total: 0,
        });
      }
    }
  };

  unsubscribe = (streams: string[]) => {
    if (streams.length > 0) {
      this.send(
        JSON.stringify({
          method: 'UNSUBSCRIBE',
          params: streams,
        }),
      );

      streams.forEach((stream) => this.subscriptions.delete(stream));
    }
  };

  dispose = () => {
    this.subscriptions.clear();
    this.orderBookState = {};
    this.orderBookSnapshotFetchers = {};
    if (this.ws) {
      this.ws.close();
    }
  };
}
