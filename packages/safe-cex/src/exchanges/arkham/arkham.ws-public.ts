import throttle from 'lodash/throttle';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { resolveExchangeRouting } from '../../routing/resolveExchangeRouting';
import { Candle, LogSeverity, OHLCVOptions, OrderBook, Ticker } from '../../types';
import { calcOrderBookTotal, sortOrderBook } from '../../utils/orderbook';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { ArkhamExchange } from './arkham.exchange';
import { WS_SUBSCRIPTION } from './arkham.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class ArkhamPublicWebSocket extends BaseWebSocket<ArkhamExchange> {
  connectUrl = '';
  private pingInterval?: NodeJS.Timeout;
  private authenticated = false;

  // Track active subscriptions for reconnection with reference counting
  private activeOrderbookSymbols = new Map<string, number>(); // symbol -> refCount
  private activeCandleSubscriptions = new Map<string, number>(); // "symbol:interval" -> refCount

  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePongEvent()],
    pong: [() => this.handlePongEvent()],
    auth: [(data: Data) => this.handleAuthResponse(data)],
    ticker: [(data: Data) => this.handleTickerUpdate(data)],
    l2_updates: [(data: Data) => this.handleOrderbookUpdate(data)],
    trades: [(data: Data) => this.handleTradesUpdate(data)],
    candles: [(data: Data) => this.handleKlineUpdate(data)],
  };

  constructor(parent: ArkhamExchange) {
    super(parent);
    // Use the proxy WebSocket server
    this.connectUrl = resolveExchangeRouting(this.parent.name, this.parent.options).wsPublic.host!;
  }

  connectAndSubscribe = () => {
    if (!this.hasLegacyProxyCredentials()) {
      this.parent.log(
        'Arkham public WebSocket requires a legacy API key and secret; skipping the connection',
        LogSeverity.Warning,
      );
      return;
    }

    if (!this.isDisposed) {
      // Connect to the proxy WebSocket - auth happens after connection
      this.ws = this.createWebsocket(this.connectUrl);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.handleClose);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('error', ((event: Event) => {
        this.parent.log(`WebSocket connection error: ${(event as any)?.message || 'Unknown error'}`, LogSeverity.Error);
      }) as any);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.parent.log('Arkham public WebSocket connected to proxy');

      // Send proxy authentication message (must be first message)
      this.authenticate();
    }
  };

  onMessage = (event: MessageEvent) => {
    this.trackLastReceived();
    const data = jsonParse(event.data) as any;
    if (!data) return;

    // Handle proxy auth responses
    if (data.type === 'auth_success') {
      this.authenticated = true;
      this.parent.log('Arkham proxy WebSocket authenticated');
      this.ping(); // Start pinging after successful auth
      this.subscribeToChannels();
      return;
    }

    if (data.type === 'error') {
      this.parent.log(`WebSocket proxy error: ${data.error}`, LogSeverity.Error);
      return;
    }

    // Handle Arkham errors
    if (data.error) {
      this.parent.log(`WebSocket error: ${data.error.name} - ${data.error.message}`, LogSeverity.Error);
      return;
    }

    // Route messages based on channel
    const handlers = this.messageHandlers[data.channel || ''];
    if (handlers && data.data) {
      // Record message stats for WebSocket health tracking
      if (data.channel) {
        this.recordMessageHandlerStats(data.channel);
      }
      handlers.forEach((handler) => handler(data.data));
    }
  };

  ping = () => {
    this.clearPingInterval();

    if (!this.isDisposed && this.isConnected) {
      this.send(JSON.stringify({ type: 'ping' }));
      this.pingInterval = setTimeout(this.ping, 30000);
    }
  };

  private handleTickerUpdate = (data: any) => {
    try {
      const ticker = data;
      const symbol = ticker.symbol;
      const currentTicker = this.parent.store.tickersBySymbol[symbol];

      if (!currentTicker) {
        this.parent.log(`Ignoring ticker update for unknown Arkham symbol ${symbol}`, LogSeverity.Warning);
        return;
      }

      const update: Partial<Ticker> = {
        bid: parseFloat(ticker.bid_price),
        ask: parseFloat(ticker.ask_price),
        last: parseFloat(ticker.last_price),
        mark: parseFloat(ticker.last_price),
        index: parseFloat(ticker.last_price),
        percentage: parseFloat(ticker.price_change_percent_24h || '0'),
        volume: parseFloat(ticker.volume_24h || '0'),
        quoteVolume: parseFloat(ticker.quote_volume_24h || '0'),
        fundingRate: parseFloat(ticker.funding_rate || '0'),
        fundingTime: ticker.next_funding_time ? parseInt(ticker.next_funding_time) : 0,
        openInterest: parseFloat(ticker.open_interest || '0'),
        info: ticker,
      };

      this.parent.store.updateTicker(currentTicker, update);
    } catch (err) {
      this.parent.log(`Error handling ticker update: ${err}`);
    }
  };

  private hasLegacyProxyCredentials = () =>
    typeof this.parent.options.key === 'string' &&
    this.parent.options.key.length > 0 &&
    typeof this.parent.options.secret === 'string' &&
    this.parent.options.secret.length > 0;

  private clearPingInterval = () => {
    if (this.pingInterval) {
      clearTimeout(this.pingInterval);
      this.pingInterval = undefined;
    }
  };

  private handleClose = () => {
    this.clearPingInterval();
    this.authenticated = false;
  };

  override dispose() {
    this.clearPingInterval();
    this.authenticated = false;
    super.dispose();
  }

  private handleOrderbookUpdate = throttle(
    (_data: any) => {
      try {
        // L2 updates are incremental updates with: symbol, side, price, size, type, revisionId, time
        // Actual orderbook state is maintained in listenOrderBook
      } catch (err) {
        this.parent.log(`Error handling orderbook update: ${err}`, LogSeverity.Error);
      }
    },
    100,
    { leading: true, trailing: true },
  );

  private handleAuthResponse = (data: any) => {
    if (data.success || data.authenticated) {
      this.authenticated = true;
      this.parent.log('Arkham public WebSocket authenticated');
      this.subscribeToChannels();
    } else {
      this.parent.log(`Arkham authentication failed: ${data.error}`);
    }
  };

  private authenticate = () => {
    // Generate proxy auth parameters
    const crypto = require('crypto');
    const timestamp = virtualClock.getCurrentTimeMs() * 1000; // Convert to microseconds
    const expiry = timestamp + 5 * 1000000; // 5 seconds in the future (in microseconds)
    const method = 'GET';
    const requestPath = '/ws';
    const message = `${this.parent.options.key}${expiry}${method}${requestPath}`;
    const decodedSecret = Buffer.from(this.parent.options.secret, 'base64');
    const signature = crypto.createHmac('sha256', decodedSecret).update(message).digest('base64');

    const authMessage = {
      type: 'auth',
      apikey: this.parent.options.key,
      expires: expiry.toString(),
      signature,
    };

    this.send(JSON.stringify(authMessage));
  };

  private handleTradesUpdate = (data: any) => {
    try {
      const trades = Array.isArray(data) ? data : [data];

      trades.forEach((_trade: any) => {
        // Process trade data if needed
        // Could emit trade events or update last price
      });
    } catch (err) {
      this.parent.log(`Error handling trades update: ${err}`);
    }
  };

  private handleKlineUpdate = (_data: any) => {
    try {
      // Process kline/candle data if needed
      // Could be used for chart updates
    } catch (err) {
      this.parent.log(`Error handling kline update: ${err}`);
    }
  };

  private subscribeToChannels = () => {
    const markets = this.parent.store.markets;

    // Subscribe to tickers for all markets
    markets.forEach((market) => {
      const subscription: WS_SUBSCRIPTION = {
        method: 'subscribe',
        args: {
          channel: 'ticker',
          params: {
            symbol: market.symbol,
          },
        },
      };
      this.send(JSON.stringify(subscription));
    });

    // Resubscribe to active orderbook subscriptions
    this.activeOrderbookSymbols.forEach((refCount, symbol) => {
      if (refCount > 0) {
        this.parent.log(`Resubscribing to orderbook for ${symbol} (${refCount} listeners)`);
        this.subscribeToOrderBook(symbol);
      }
    });

    // Resubscribe to active candle subscriptions
    this.activeCandleSubscriptions.forEach((refCount, key) => {
      if (refCount > 0) {
        const [symbol, interval] = key.split(':');
        this.parent.log(`Resubscribing to candles for ${symbol} ${interval} (${refCount} listeners)`);
        const subscription: WS_SUBSCRIPTION = {
          method: 'subscribe',
          args: {
            channel: 'candles',
            params: {
              symbol,
              duration: interval,
            },
          },
        };
        this.send(JSON.stringify(subscription));
      }
    });
  };

  subscribeToOrderBook = (symbol: string) => {
    if (this.isConnected) {
      const subscription: WS_SUBSCRIPTION = {
        method: 'subscribe',
        args: {
          channel: 'l2_updates',
          params: {
            symbol,
          },
        },
      };
      this.send(JSON.stringify(subscription));
    }
  };

  unsubscribeFromOrderBook = (symbol: string) => {
    if (this.isConnected) {
      const subscription: WS_SUBSCRIPTION = {
        method: 'unsubscribe',
        args: {
          channel: 'l2_updates',
          params: {
            symbol,
          },
        },
      };
      this.send(JSON.stringify(subscription));
    }
  };

  subscribeToTrades = (symbol: string) => {
    if (this.isConnected) {
      const subscription: WS_SUBSCRIPTION = {
        method: 'subscribe',
        args: {
          channel: 'trades',
          params: {
            symbol,
          },
        },
      };
      this.send(JSON.stringify(subscription));
    }
  };

  unsubscribeFromTrades = (symbol: string) => {
    if (this.isConnected) {
      const subscription: WS_SUBSCRIPTION = {
        method: 'unsubscribe',
        args: {
          channel: 'trades',
          params: {
            symbol,
          },
        },
      };
      this.send(JSON.stringify(subscription));
    }
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    if (!this.parent.store.loaded.markets) {
      timeoutId = setTimeout(() => this.listenOrderBook(symbol, callback), 100);
      return () => {
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      };
    }

    const market = this.parent.store.markets.find((m) => m.symbol === symbol);
    if (!market) return () => {};

    // Maintain orderbook state as maps for efficient updates
    const bidsMap = new Map<string, number>(); // price -> size
    const asksMap = new Map<string, number>(); // price -> size

    const parser = (data: any) => {
      try {
        if (!data.symbol || data.symbol !== symbol) return;

        // Record stats for this specific orderbook
        this.recordMessageHandlerStats(`l2_updates.${symbol}`);

        // Handle incremental L2 update
        // Message format: { symbol, side: 'buy'|'sell', price, size, type: 'update', revisionId, time }
        const price = data.price;
        const size = parseFloat(data.size);
        const side = data.side; // 'buy' or 'sell'

        // Update the appropriate side
        const targetMap = side === 'buy' ? bidsMap : asksMap;

        if (size === 0) {
          // Remove price level
          targetMap.delete(price);
        } else {
          // Add or update price level
          targetMap.set(price, size);
        }

        // Convert maps to OrderBook format
        const orderBook: OrderBook = {
          bids: Array.from(bidsMap.entries()).map(([price, amount]) => ({
            price: parseFloat(price),
            amount,
            total: 0,
          })),
          asks: Array.from(asksMap.entries()).map(([price, amount]) => ({
            price: parseFloat(price),
            amount,
            total: 0,
          })),
        };

        // Filter by last price to keep orderbook reasonable. O(1) hashmap
        // lookup — this fires on every l2_updates frame (30+/sec/symbol),
        // so the prior `tickers.find` was N scans per orderbook tick.
        const ticker = this.parent.store.tickersBySymbol[symbol];
        const lastPrice = ticker?.last || 0;

        if (lastPrice > 0) {
          orderBook.asks = orderBook.asks.filter((a) => a.price >= lastPrice * 0.8);
          orderBook.bids = orderBook.bids.filter((b) => b.price <= lastPrice * 1.2);
        }

        sortOrderBook(orderBook);
        calcOrderBookTotal(orderBook);

        callback(orderBook);
      } catch (err) {
        this.parent.log(`Error in listenOrderBook parser: ${err}`, LogSeverity.Error);
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers.l2_updates = [...(this.messageHandlers.l2_updates || []), parser];

          // Track active subscription with reference counting
          const currentCount = this.activeOrderbookSymbols.get(symbol) || 0;
          this.activeOrderbookSymbols.set(symbol, currentCount + 1);

          // Only subscribe if this is the first listener
          if (currentCount === 0) {
            this.subscribeToOrderBook(symbol);
          }
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      if (this.messageHandlers.l2_updates) {
        this.messageHandlers.l2_updates = this.messageHandlers.l2_updates.filter((h) => h !== parser);
        if (this.messageHandlers.l2_updates.length === 0) {
          delete this.messageHandlers.l2_updates;
        }
      }

      // Cleanup stats for this specific orderbook
      this.cleanupMessageHandlerStats(`l2_updates.${symbol}`);

      // Decrement reference count and only remove if last listener
      const currentCount = this.activeOrderbookSymbols.get(symbol) || 0;
      if (currentCount <= 1) {
        this.activeOrderbookSymbols.delete(symbol);
        // Only unsubscribe if this was the last listener
        if (this.isConnected) {
          this.unsubscribeFromOrderBook(symbol);
        }
      } else {
        this.activeOrderbookSymbols.set(symbol, currentCount - 1);
      }

      // Clear orderbook state
      bidsMap.clear();
      asksMap.clear();

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
    };
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;

    if (!this.parent.store.loaded.markets) {
      timeoutId = setTimeout(() => this.listenOHLCV(opts, callback), 100);
      return () => {
        if (timeoutId) {
          clearTimeout(timeoutId);
          timeoutId = null;
        }
      };
    }

    const market = this.parent.store.markets.find((m) => m.symbol === opts.symbol);
    if (!market) return () => {};

    const parser = (data: any) => {
      try {
        if (!data.symbol || data.symbol !== opts.symbol) return;

        // Record stats for this specific candle subscription
        this.recordMessageHandlerStats(`candles.${opts.symbol}.${opts.interval}`);

        const candle: Candle = {
          timestamp: Math.floor(data.time / 1000), // Convert microseconds to milliseconds
          open: parseFloat(data.open),
          high: parseFloat(data.high),
          low: parseFloat(data.low),
          close: parseFloat(data.close),
          volume: parseFloat(data.volume),
        };

        callback(candle);
      } catch (err) {
        this.parent.log(`Error in listenOHLCV parser: ${err}`, LogSeverity.Error);
      }
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers.candles = [...(this.messageHandlers.candles || []), parser];

          // Track active subscription with reference counting using composite key
          const key = `${opts.symbol}:${opts.interval}`;
          const currentCount = this.activeCandleSubscriptions.get(key) || 0;
          this.activeCandleSubscriptions.set(key, currentCount + 1);

          // Only subscribe if this is the first listener
          if (currentCount === 0) {
            const subscription: WS_SUBSCRIPTION = {
              method: 'subscribe',
              args: {
                channel: 'candles',
                params: {
                  symbol: opts.symbol,
                  duration: opts.interval,
                },
              },
            };
            this.send(JSON.stringify(subscription));
          }
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      if (this.messageHandlers.candles) {
        this.messageHandlers.candles = this.messageHandlers.candles.filter((h) => h !== parser);
        if (this.messageHandlers.candles.length === 0) {
          delete this.messageHandlers.candles;
        }
      }

      // Cleanup stats for this specific candle subscription
      this.cleanupMessageHandlerStats(`candles.${opts.symbol}.${opts.interval}`);

      // Decrement reference count and only remove if last listener
      const key = `${opts.symbol}:${opts.interval}`;
      const currentCount = this.activeCandleSubscriptions.get(key) || 0;
      if (currentCount <= 1) {
        this.activeCandleSubscriptions.delete(key);
        // Only unsubscribe if this was the last listener
        if (this.isConnected) {
          const subscription: WS_SUBSCRIPTION = {
            method: 'unsubscribe',
            args: {
              channel: 'candles',
              params: {
                symbol: opts.symbol,
                duration: opts.interval,
              },
            },
          };
          this.send(JSON.stringify(subscription));
        }
      } else {
        this.activeCandleSubscriptions.set(key, currentCount - 1);
      }

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }
    };
  };
}
