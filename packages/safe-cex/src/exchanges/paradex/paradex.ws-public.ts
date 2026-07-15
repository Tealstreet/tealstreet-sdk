import throttle from 'lodash/throttle';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Candle, LogSeverity, OHLCVOptions, OrderBook, Ticker } from '../../types';
import { BaseWebSocket } from '../base.ws';
import { ParadexExchange } from './paradex.exchange';
import { BASE_WS_URL } from './paradex.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

const getOrderBookChannel = (symbol: string) => `order_book.${symbol}.snapshot@15@100ms`;

export class ParadexPublicWebSocket extends BaseWebSocket<ParadexExchange> {
  connectUrl = '';
  private pingInterval?: NodeJS.Timeout;
  private orderbookCallbacks: Map<string, Set<(orderbook: OrderBook) => void>> = new Map();
  private tradeSubscriptionCounts: Map<string, number> = new Map();
  private messageId = 0;

  // OHLCV polling state + trades integration
  private ohlcvPollingTimeouts = new Map<string, NodeJS.Timeout>();
  private ohlcvPollingActive = new Map<string, boolean>();
  private lastCandleCache = new Map<string, Candle>();
  private ohlcvCallbacks = new Map<string, (candle: Candle) => void>(); // Store callbacks for trade updates
  private ohlcvIntervals = new Map<string, number>(); // Store interval in ms for each subscription

  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePongEvent()],
    pong: [() => this.handlePongEvent()],
    // bbo provides per-symbol best bid/ask tick updates at near-real-time cadence.
    // Without it, watchlist/positions PnL becomes laggy because markets_summary
    // alone does not push frequently enough for live UX.
    bbo: [(data: Data) => this.handleBBOUpdate(data)],
    // markets_summary kept for slow-changing fields (volume, openInterest, fundingRate)
    // that bbo doesn't carry. Throttled aggregate; do NOT rely on it for bid/ask.
    markets_summary: [(data: Data) => this.handleMarketSummaryUpdate(data)],
    order_book: [(data: Data) => this.handleOrderbookUpdate(data)],
    trades: [(data: Data) => this.handleTradesUpdate(data)],
  };

  constructor(parent: ParadexExchange) {
    super(parent);
    this.connectUrl = BASE_WS_URL.public[parent.options.testnet ? 'testnet' : 'livenet'];
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(this.connectUrl);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.parent.log('Paradex public WebSocket connected', LogSeverity.Info);
      this.ping();

      // Verify websocket is still connected before subscribing
      if (!this.isConnected) {
        this.parent.log('WebSocket disconnected in onOpen, closing connection', LogSeverity.Warning);
        this.ws?.close();
        return;
      }

      this.subscribeToChannels();
    }
  };

  onMessage = (event: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      const data = jsonParse(event.data);
      if (!data) return;

      // Handle JSON-RPC 2.0 format
      if (data.jsonrpc === '2.0') {
        // Handle subscription updates
        if (data.method === 'subscription' && data.params) {
          const channel = data.params.channel;
          // Extract channel name (e.g., "order_book.BTC-USD-PERP.snapshot@15@100ms" -> "order_book")
          const channelParts = channel?.split('.') || [];
          const channelType = channelParts[0];
          const symbol = channelParts[1]; // Symbol if present (e.g., "BTC-USD-PERP")
          const handlers = this.messageHandlers[channelType];

          if (handlers) {
            // Record message stats to track WebSocket health
            // Only orderbook needs per-symbol tracking (users subscribe to specific orderbooks)
            // markets_summary and trades are tracked generically (we just need *some* updates)
            if (symbol && channelType === 'order_book') {
              this.recordMessageHandlerStats(`${channelType}.${symbol}`);
            } else {
              this.recordMessageHandlerStats(channelType);
            }
            handlers.forEach((handler) => handler(data.params));
          } else if (channelType) {
            this.parent.log(`No handler for channel type: ${channelType} (channel: ${channel})`, LogSeverity.Warning);
          }
        }
        // Handle responses (success/error)
        else if (data.id !== undefined) {
          if (data.error) {
            this.parent.log(`WebSocket error for request ${data.id}: ${data.error.message}`, LogSeverity.Error);
          }
        }
      }
      // Legacy format (ping/pong)
      else {
        const topic = data.type || data.channel;
        const handlers = this.messageHandlers[topic];
        if (handlers) {
          // Record message stats to track WebSocket health
          this.recordMessageHandlerStats(topic);
          handlers.forEach((handler) => handler(data));
        }
      }
    }
  };

  ping = () => {
    if (this.isConnected) {
      this.send(JSON.stringify({ type: 'ping' }));
    }

    this.pingInterval = setTimeout(() => {
      if (!this.isDisposed && this.isConnected) {
        this.ping();
      }
    }, 30000);
  };

  dispose = () => {
    if (this.pingInterval) {
      clearTimeout(this.pingInterval);
      this.pingInterval = undefined;
    }
    super.dispose();
  };

  private handleBBOUpdate = throttle(
    (data: any) => {
      try {
        const bboData = data?.data;
        if (!bboData) {
          return;
        }

        const channel = String(data?.channel || '');
        const channelSymbol = channel.split('.')[1];
        const symbol = String(bboData.market || channelSymbol || '');
        if (!symbol) {
          return;
        }

        // BBO format: { market, bid_price, bid_size, ask_price, ask_size, mark_price, index_price }
        // last_price is intentionally NOT updated from bbo — that comes from the trades feed.
        const bid = parseFloat(bboData.bid_price || '0');
        const ask = parseFloat(bboData.ask_price || '0');
        const index = parseFloat(bboData.index_price || '0');

        const update: Partial<Ticker> = {
          symbol,
          ...(Number.isFinite(bid) && bid > 0 ? { bid } : {}),
          ...(Number.isFinite(ask) && ask > 0 ? { ask } : {}),
          ...(Number.isFinite(index) && index > 0 ? { index } : {}),
        };

        this.parent.store.updateTicker({ id: symbol }, update);
      } catch (err) {
        this.parent.log(`Error handling BBO update: ${err}`, LogSeverity.Error);
      }
    },
    100,
    { leading: true, trailing: true },
  );

  private handleMarketSummaryUpdate = throttle(
    (data: any) => {
      try {
        const rawData = Array.isArray(data?.data) ? data.data : [data?.data || data];
        for (const summary of rawData) {
          if (!summary || typeof summary !== 'object') {
            continue;
          }

          const symbol = String(summary.symbol || summary.market || '');
          if (!symbol) {
            continue;
          }

          const bid = parseFloat(summary.bid || '0');
          const ask = parseFloat(summary.ask || '0');
          const last = parseFloat(summary.last_traded_price || summary.last_price || summary.last || '0');
          const mark = parseFloat(summary.mark_price || '0');
          const index = parseFloat(summary.oracle_price || summary.index_price || summary.underlying_price || '0');
          const percentageRaw = parseFloat(summary.price_change_rate_24h || '0');
          const volume = parseFloat(summary.volume_24h || summary.volume || '0');
          const openInterest = parseFloat(summary.open_interest || '0');
          const fundingRate = parseFloat(summary.funding_rate || '0');

          this.parent.store.updateTicker(
            { id: symbol },
            {
              symbol,
              ...(Number.isFinite(bid) && bid > 0 ? { bid } : {}),
              ...(Number.isFinite(ask) && ask > 0 ? { ask } : {}),
              ...(Number.isFinite(last) && last > 0 ? { last } : {}),
              ...(Number.isFinite(mark) && mark > 0 ? { mark } : {}),
              ...(Number.isFinite(index) && index > 0 ? { index } : {}),
              ...(Number.isFinite(percentageRaw) ? { percentage: percentageRaw * 100 } : {}),
              ...(Number.isFinite(volume) ? { volume } : {}),
              ...(Number.isFinite(openInterest) ? { openInterest } : {}),
              ...(Number.isFinite(fundingRate) ? { fundingRate } : {}),
            },
          );
        }
      } catch (err) {
        this.parent.log(`Error handling markets_summary update: ${err}`, LogSeverity.Error);
      }
    },
    100,
    { leading: true, trailing: true },
  );

  private handleOrderbookUpdate = throttle(
    (data: any) => {
      try {
        const rawData = Array.isArray(data?.data) ? data.data[0] : data?.data || data;
        const channel = String(data?.channel || '');
        const channelSymbol = channel.split('.')[1];
        const orderbookData = rawData && typeof rawData === 'object' ? rawData : {};
        const symbol = String(orderbookData.market || channelSymbol || '');

        if (!symbol) {
          return;
        }

        // Paradex typically sends inserts/updates/deletes deltas, but keep a fallback for bids/asks snapshots.
        const inserts = Array.isArray(orderbookData.inserts) ? orderbookData.inserts : [];
        const updates = Array.isArray(orderbookData.updates) ? orderbookData.updates : [];
        const allLevels = [...inserts, ...updates];
        const rawBids = Array.isArray(orderbookData.bids) ? orderbookData.bids : [];
        const rawAsks = Array.isArray(orderbookData.asks) ? orderbookData.asks : [];

        const normalizedBidLevels =
          allLevels.length > 0
            ? allLevels.filter((level: any) => level.side === 'BUY')
            : rawBids.map((level: any) =>
                Array.isArray(level)
                  ? { side: 'BUY', price: level[0], size: level[1] }
                  : { side: 'BUY', price: level?.price, size: level?.size || level?.amount },
              );
        const normalizedAskLevels =
          allLevels.length > 0
            ? allLevels.filter((level: any) => level.side === 'SELL')
            : rawAsks.map((level: any) =>
                Array.isArray(level)
                  ? { side: 'SELL', price: level[0], size: level[1] }
                  : { side: 'SELL', price: level?.price, size: level?.size || level?.amount },
              );

        // Update best bid/ask in ticker
        if (normalizedBidLevels.length > 0 && normalizedAskLevels.length > 0) {
          // Sort to get best prices
          const sortedBids = normalizedBidLevels.sort((a: any, b: any) => parseFloat(b.price) - parseFloat(a.price));
          const sortedAsks = normalizedAskLevels.sort((a: any, b: any) => parseFloat(a.price) - parseFloat(b.price));

          const bestBid = parseFloat(sortedBids[0].price);
          const bestAsk = parseFloat(sortedAsks[0].price);

          this.parent.store.updateTicker(
            { id: symbol },
            {
              bid: bestBid,
              ask: bestAsk,
            },
          );
        }

        // Emit to registered callbacks
        const callbacks = this.orderbookCallbacks.get(symbol);
        if (callbacks && callbacks.size > 0) {
          const orderbook: OrderBook = {
            bids: normalizedBidLevels.map((level: any) => ({
              price: parseFloat(level.price),
              amount: parseFloat(level.size),
              total: parseFloat(level.price) * parseFloat(level.size),
            })),
            asks: normalizedAskLevels.map((level: any) => ({
              price: parseFloat(level.price),
              amount: parseFloat(level.size),
              total: parseFloat(level.price) * parseFloat(level.size),
            })),
          };

          callbacks.forEach((callback) => callback(orderbook));
        }
      } catch (err) {
        this.parent.log(`Error handling orderbook update: ${err}`, LogSeverity.Error);
      }
    },
    100,
    { leading: true, trailing: true },
  );

  private handleTradesUpdate = (data: any) => {
    try {
      // Debug: log when trades handler is called (commented out to reduce log spam)
      // this.parent.log(`Trades handler called with data: ${JSON.stringify(data)}`, LogSeverity.Debug);

      const rawData = Array.isArray(data?.data) ? data.data[0] : data?.data || data;
      const channel = String(data?.channel || '');
      const channelSymbol = channel.split('.')[1];
      const tradesData = rawData && typeof rawData === 'object' ? rawData : {};

      if (!tradesData || Object.keys(tradesData).length === 0) {
        this.parent.log(`Trades data is undefined. Full data: ${JSON.stringify(data)}`, LogSeverity.Warning);
        return;
      }

      const symbol = String(tradesData.market || channelSymbol || '');
      if (!symbol) {
        this.parent.log(`Trade missing market symbol. Data: ${JSON.stringify(tradesData)}`, LogSeverity.Warning);
        return;
      }

      // Update last price from trade
      const tradePrice = parseFloat(tradesData.price || '0');
      const tradeSize = parseFloat(tradesData.size || '0');
      const tradeTimestamp = Date.now(); // Use current time as trade timestamp

      if (tradePrice > 0) {
        // this.parent.log(
        //   `✅ Updating last price from trade: ${symbol} = ${tradePrice} (size=${tradesData.size || 'N/A'})`,
        //   LogSeverity.Info,
        // );

        this.parent.store.updateTicker(
          { id: symbol },
          {
            symbol,
            last: tradePrice,
            mark: tradePrice,
          },
        );

        // Update OHLCV candles for all active subscriptions for this symbol
        this.updateOHLCVFromTrade(symbol, tradePrice, tradeSize, tradeTimestamp);

        // console.log(symbol, tradePrice);
      } else {
        this.parent.log(`Trade price is 0 or invalid: ${tradesData.price}`, LogSeverity.Warning);
      }
    } catch (err) {
      this.parent.log(`Error handling trades update: ${err}`, LogSeverity.Error);
    }
  };

  private updateOHLCVFromTrade(symbol: string, price: number, size: number, timestamp: number) {
    // Check all active OHLCV subscriptions for this symbol
    for (const [pollKey, callback] of this.ohlcvCallbacks.entries()) {
      // Extract symbol from pollKey (format: "SYMBOL:interval")
      const [subscriptionSymbol] = pollKey.split(':');

      if (subscriptionSymbol !== symbol) {
        continue;
      }

      // Get the interval in milliseconds for this subscription
      const intervalMs = this.ohlcvIntervals.get(pollKey);
      if (!intervalMs) {
        continue;
      }

      // Get or create the current candle
      let currentCandle = this.lastCandleCache.get(pollKey);

      // Calculate the candle's timestamp (start of the interval period)
      const candleTimestamp = Math.floor(timestamp / intervalMs) * intervalMs;

      // If we don't have a candle or it's a new candle period, initialize it
      if (!currentCandle || currentCandle.timestamp !== candleTimestamp) {
        currentCandle = {
          timestamp: candleTimestamp,
          open: price,
          high: price,
          low: price,
          close: price,
          volume: size,
        };
      } else {
        // Update existing candle
        currentCandle = {
          ...currentCandle,
          high: Math.max(currentCandle.high, price),
          low: Math.min(currentCandle.low, price),
          close: price,
          volume: currentCandle.volume + size,
        };
      }

      // Update cache and emit if changed
      const previousCandle = this.lastCandleCache.get(pollKey);
      if (
        !previousCandle ||
        previousCandle.timestamp !== currentCandle.timestamp ||
        previousCandle.close !== currentCandle.close ||
        previousCandle.high !== currentCandle.high ||
        previousCandle.low !== currentCandle.low ||
        previousCandle.volume !== currentCandle.volume
      ) {
        this.lastCandleCache.set(pollKey, currentCandle);
        callback(currentCandle);
      }
    }
  }

  private subscribeToChannels = () => {
    const markets = this.parent.store.markets;

    // Per-symbol bbo + trades broadcast subscriptions: required for live bid/ask/last
    // updates on the watchlist and positions table. markets_summary alone is too slow
    // for tick-rate UX. Orderbook stays on-demand to avoid subscribing to 100+ books.
    markets.forEach((market) => {
      this.send(
        JSON.stringify({
          id: this.messageId++,
          jsonrpc: '2.0',
          method: 'subscribe',
          params: { channel: `bbo.${market.symbol}` },
        }),
      );
    });

    markets.forEach((market) => {
      this.send(
        JSON.stringify({
          id: this.messageId++,
          jsonrpc: '2.0',
          method: 'subscribe',
          params: { channel: `trades.${market.symbol}` },
        }),
      );
    });

    // markets_summary: bonus aggregate for slow-changing fields (volume_24h,
    // open_interest, funding_rate, etc.) that bbo doesn't carry.
    this.send(
      JSON.stringify({
        id: this.messageId++,
        jsonrpc: '2.0',
        method: 'subscribe',
        params: { channel: 'markets_summary' },
      }),
    );

    this.parent.log(`Subscribed to bbo + trades for ${markets.length} markets + markets_summary`, LogSeverity.Info);

    // Resubscribe to orderbooks that have active callbacks
    // This handles the case where listenOrderBook was called for symbols
    // that might not be in the store.markets list
    if (this.orderbookCallbacks.size > 0) {
      this.orderbookCallbacks.forEach((callbacks, symbol) => {
        if (callbacks.size > 0) {
          this.subscribeToOrderBook(symbol);
        }
      });
      this.parent.log(`Resubscribed to ${this.orderbookCallbacks.size} orderbooks`, LogSeverity.Info);
    }
  };

  subscribeToOrderBook = (symbol: string) => {
    if (this.isConnected) {
      // Paradex uses JSON-RPC 2.0 format with channel format:
      // Paradex public WS orderbook channel format: order_book.{market}.snapshot@{depth}@{refresh_rate}
      const channel = getOrderBookChannel(symbol);
      const subscribeMsg = {
        id: this.messageId++,
        jsonrpc: '2.0',
        method: 'subscribe',
        params: { channel },
      };
      this.send(JSON.stringify(subscribeMsg));
      // this.parent.log(`Subscribing to orderbook: ${channel}`, LogSeverity.Debug);
    }
  };

  unsubscribeFromOrderBook = (symbol: string) => {
    if (this.isConnected) {
      const channel = getOrderBookChannel(symbol);
      const unsubscribeMsg = {
        id: this.messageId++,
        jsonrpc: '2.0',
        method: 'unsubscribe',
        params: { channel },
      };
      this.send(JSON.stringify(unsubscribeMsg));
      // this.parent.log(`Unsubscribing from orderbook: ${channel}`, LogSeverity.Debug);
    }
  };

  // trades.{symbol} is now broadcast-subscribed for every market in subscribeToChannels.
  // The OHLCV listener flow still calls subscribeToTrades/unsubscribeFromTrades for
  // historical reasons, but we keep them as ref-counted no-ops so that an OHLCV cleanup
  // can never tear down a trades subscription that the watchlist/positions UI still
  // depends on for last-price updates. We retain the counter only for diagnostics.
  subscribeToTrades = (symbol: string) => {
    const currentCount = this.tradeSubscriptionCounts.get(symbol) || 0;
    this.tradeSubscriptionCounts.set(symbol, currentCount + 1);
  };

  unsubscribeFromTrades = (symbol: string) => {
    const currentCount = this.tradeSubscriptionCounts.get(symbol) || 0;
    if (currentCount <= 1) {
      this.tradeSubscriptionCounts.delete(symbol);
      return;
    }
    this.tradeSubscriptionCounts.set(symbol, currentCount - 1);
  };

  listenOrderBook = (symbol: string, callback: (orderbook: OrderBook) => void) => {
    // Get or create callback set for this symbol
    if (!this.orderbookCallbacks.has(symbol)) {
      this.orderbookCallbacks.set(symbol, new Set());
    }

    const callbacks = this.orderbookCallbacks.get(symbol)!;
    callbacks.add(callback);

    // Subscribe to orderbook if this is the first listener
    if (callbacks.size === 1) {
      this.subscribeToOrderBook(symbol);

      // Asynchronously set max slippage to 20% when listening to orderbook
      if (!this.parent.isPublicDataOnly && this.parent.jwtManager) {
        this.parent.setMaxSlippage('0.2').catch((err) => {
          this.parent.log(`Failed to set max slippage for ${symbol}: ${err}`, LogSeverity.Warning);
        });
      }
    }

    // Return unsubscribe function
    return () => {
      callbacks.delete(callback);

      // Unsubscribe from orderbook if no more listeners
      if (callbacks.size === 0) {
        this.orderbookCallbacks.delete(symbol);
        this.unsubscribeFromOrderBook(symbol);
        this.cleanupMessageHandlerStats(`order_book.${symbol}`);
      }
    };
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    // Paradex doesn't support OHLCV via WebSocket natively,
    // so we use a hybrid approach: poll REST API + update from trades feed

    const pollKey = `${opts.symbol}:${opts.interval}`;

    // Clear any existing polling for this key
    if (this.ohlcvPollingTimeouts.has(pollKey)) {
      clearTimeout(this.ohlcvPollingTimeouts.get(pollKey)!);
      this.ohlcvPollingTimeouts.delete(pollKey);
    }

    // Calculate interval in milliseconds for trade updates
    const intervalMatch = opts.interval.match(/^(\d+)([mh])$/);
    const intervalMs = intervalMatch
      ? intervalMatch[2] === 'h'
        ? parseInt(intervalMatch[1]) * 60 * 60 * 1000
        : parseInt(intervalMatch[1]) * 60 * 1000
      : 60 * 1000; // Default to 1 minute

    // Store callback and interval for trade-based updates
    this.ohlcvCallbacks.set(pollKey, callback);
    this.ohlcvIntervals.set(pollKey, intervalMs);

    // Subscribe to symbol trades while this OHLCV listener is active.
    this.subscribeToTrades(opts.symbol);

    // Mark polling as active
    this.ohlcvPollingActive.set(pollKey, true);

    // Asynchronously set max slippage to 20% when listening to OHLCV
    if (!this.parent.isPublicDataOnly && this.parent.jwtManager) {
      this.parent.setMaxSlippage('0.2').catch((err) => {
        this.parent.log(`Failed to set max slippage for ${opts.symbol}: ${err}`, LogSeverity.Warning);
      });
    }

    // Recursive polling function using setTimeout
    const pollCandle = async () => {
      // Check if polling is still active
      if (!this.ohlcvPollingActive.get(pollKey)) {
        return;
      }

      try {
        const now = Date.now();

        // Calculate how far back to fetch based on interval
        // Parse interval (e.g., "1m", "5m", "1h") to milliseconds
        const intervalMatch = opts.interval.match(/^(\d+)([mh])$/);
        const intervalMs = intervalMatch
          ? intervalMatch[2] === 'h'
            ? parseInt(intervalMatch[1]) * 60 * 60 * 1000
            : parseInt(intervalMatch[1]) * 60 * 1000
          : 60 * 1000; // Default to 1 minute

        // Fetch last 2 candles (go back 3x interval to be safe)
        const from = now - intervalMs * 3;

        const candles = await this.parent.fetchOHLCV({
          symbol: opts.symbol,
          interval: opts.interval,
          from: from,
          to: now,
        });

        if (candles.length > 0) {
          const latestCandle = candles[candles.length - 1];
          const previousCandle = this.lastCandleCache.get(pollKey);

          // Only emit if the candle has changed
          if (
            !previousCandle ||
            previousCandle.timestamp !== latestCandle.timestamp ||
            previousCandle.close !== latestCandle.close ||
            previousCandle.volume !== latestCandle.volume
          ) {
            callback(latestCandle);
            this.lastCandleCache.set(pollKey, latestCandle);
          }
        }
      } catch {
        // this.parent.log(`Error polling OHLCV for ${opts.symbol}: ${err}`, LogSeverity.Warning);
      }

      // Schedule next poll only if still active (poll every 3 seconds)
      if (this.ohlcvPollingActive.get(pollKey)) {
        const timeoutId = setTimeout(pollCandle, 3000);
        this.ohlcvPollingTimeouts.set(pollKey, timeoutId);
      }
    };

    // Start polling immediately
    pollCandle();

    // Return cleanup function
    return () => {
      // Mark as inactive
      this.ohlcvPollingActive.set(pollKey, false);

      // Clear timeout if exists
      if (this.ohlcvPollingTimeouts.has(pollKey)) {
        clearTimeout(this.ohlcvPollingTimeouts.get(pollKey)!);
        this.ohlcvPollingTimeouts.delete(pollKey);
      }

      // Clear callback and interval for trades
      this.ohlcvCallbacks.delete(pollKey);
      this.ohlcvIntervals.delete(pollKey);
      this.unsubscribeFromTrades(opts.symbol);

      // Clear cache
      this.lastCandleCache.delete(pollKey);
    };
  };
}
