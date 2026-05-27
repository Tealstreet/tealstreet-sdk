import { ReconnectingWebSocket } from '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket';
import {
  ENABLE_RECONNECTING_WEBSOCKET,
  RECONNECTING_WEBSOCKET_TUNING,
  WS_MIN_RECONNECTION_DELAY,
} from '../config/config';
import { Candle, LogSeverity, OrderBook, Ticker, Market, OHLCVOptions, Timeframe } from '../types';
import type { BaseExchange } from './base';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';

/**
 * Gateway message types for public data multiplexing
 */
export type GatewayMessageType =
  | 'subscribe'
  | 'unsubscribe'
  | 'subscribed'
  | 'unsubscribed'
  | 'data'
  | 'error'
  | 'ping'
  | 'pong';

/**
 * Gateway data channels
 */
export type GatewayChannel = 'ticker' | 'tickers' | 'orderbook' | 'candle' | 'trade' | 'market';

/**
 * Base gateway message structure
 */
export interface GatewayMessage {
  type: GatewayMessageType;
  exchangeId: string;
  channel?: GatewayChannel;
  symbol?: string;
  interval?: Timeframe;
  data?: unknown;
  error?: string;
  requestId?: string;
}

/**
 * Ticker update from gateway
 */
export interface GatewayTickerUpdate {
  type: 'data';
  exchangeId: string;
  channel: 'ticker' | 'tickers';
  data: Ticker | Ticker[];
}

/**
 * Orderbook update from gateway
 */
export interface GatewayOrderbookUpdate {
  type: 'data';
  exchangeId: string;
  channel: 'orderbook';
  symbol: string;
  data: OrderBook;
}

/**
 * Candle update from gateway
 */
export interface GatewayCandleUpdate {
  type: 'data';
  exchangeId: string;
  channel: 'candle';
  symbol: string;
  interval: Timeframe;
  data: Candle;
}

/**
 * Market update from gateway
 */
export interface GatewayMarketUpdate {
  type: 'data';
  exchangeId: string;
  channel: 'market';
  data: Market[];
}

/**
 * External public data provider for WebSocket multiplexing.
 *
 * This class connects to a public data gateway that maintains a single connection
 * to each exchange and fans out public data to multiple safe-cex instances.
 *
 * Usage:
 * - When `ExchangeOptions.externalPublicData` is set, the exchange creates this provider
 * - The provider connects to the gateway and subscribes to data streams
 * - Data is injected into the exchange's store/emitter just like direct WebSocket data
 */
export class ExternalPublicDataProvider {
  private ws?: ReconnectingWebSocket | WebSocket;
  private parent: BaseExchange;
  private url: string;
  private exchangeId: string;
  private authToken?: string;

  private subscriptions = new Map<string, Set<(data: unknown) => void>>();
  private reconnecting = false;
  private lastReceived = 0;
  private reconnectCount = 0;
  private isDisposed = false;

  // Orderbook callbacks by symbol
  private orderbookCallbacks = new Map<string, Set<(orderbook: OrderBook) => void>>();

  // Candle callbacks by symbol:interval
  private candleCallbacks = new Map<string, Set<(candle: Candle) => void>>();

  constructor(
    parent: BaseExchange,
    config: { url: string; exchangeId: string; authToken?: string },
  ) {
    this.parent = parent;
    this.url = config.url;
    this.exchangeId = config.exchangeId;
    this.authToken = config.authToken;
  }

  /**
   * Connect to the gateway and start receiving data
   */
  connect = () => {
    if (this.isDisposed) return;

    this.log('Connecting to public data gateway...', LogSeverity.Info);

    if (ENABLE_RECONNECTING_WEBSOCKET) {
      this.ws = new ReconnectingWebSocket(this.url, undefined, RECONNECTING_WEBSOCKET_TUNING);
    } else {
      this.ws = new WebSocket(this.url);
    }

    this.ws.addEventListener('open', this.onOpen);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.onClose);
    this.ws.addEventListener('error', this.onError as any);
  };

  private onOpen = () => {
    this.log('Connected to public data gateway', LogSeverity.Info);
    this.lastReceived = Date.now();

    // Authenticate if token provided
    if (this.authToken) {
      this.send({
        type: 'subscribe',
        exchangeId: this.exchangeId,
        channel: 'auth' as GatewayChannel,
        data: { token: this.authToken },
      });
    }

    // Subscribe to tickers (always needed)
    this.send({
      type: 'subscribe',
      exchangeId: this.exchangeId,
      channel: 'tickers',
    });

    // Subscribe to markets
    this.send({
      type: 'subscribe',
      exchangeId: this.exchangeId,
      channel: 'market',
    });

    // Re-subscribe to any active orderbook/candle subscriptions
    this.resubscribeAll();
  };

  private onMessage = (event: MessageEvent) => {
    if (this.isDisposed) return;

    this.lastReceived = Date.now();

    const message = jsonParse(event.data) as GatewayMessage;
    if (!message) return;

    // Filter messages for our exchange
    if (message.exchangeId !== this.exchangeId) return;

    switch (message.type) {
      case 'data':
        this.handleDataMessage(message);
        break;

      case 'subscribed':
        this.log(`Subscribed to ${message.channel}${message.symbol ? `:${message.symbol}` : ''}`, LogSeverity.Info);
        break;

      case 'unsubscribed':
        this.log(`Unsubscribed from ${message.channel}${message.symbol ? `:${message.symbol}` : ''}`, LogSeverity.Info);
        break;

      case 'error':
        this.log(`Gateway error: ${message.error}`, LogSeverity.Error);
        break;

      case 'pong':
        // Heartbeat response
        break;
    }
  };

  private handleDataMessage = (message: GatewayMessage) => {
    switch (message.channel) {
      case 'tickers':
        this.handleTickers(message.data as Ticker[]);
        break;

      case 'ticker':
        this.handleTicker(message.data as Ticker);
        break;

      case 'orderbook':
        if (message.symbol) {
          this.handleOrderbook(message.symbol, message.data as OrderBook);
        }
        break;

      case 'candle':
        if (message.symbol && message.interval) {
          this.handleCandle(message.symbol, message.interval, message.data as Candle);
        }
        break;

      case 'market':
        this.handleMarkets(message.data as Market[]);
        break;
    }
  };

  private handleTickers = (tickers: Ticker[]) => {
    // Update store with tickers - same as direct WebSocket would do
    this.parent.store.update({
      tickers,
      loaded: { ...this.parent.store.loaded, tickers: true },
    });
  };

  private handleTicker = (ticker: Ticker) => {
    // Update single ticker
    const tickers = [...this.parent.store.tickers];
    const index = tickers.findIndex((t) => t.symbol === ticker.symbol);
    if (index >= 0) {
      tickers[index] = ticker;
    } else {
      tickers.push(ticker);
    }
    this.parent.store.update({ tickers });
  };

  private handleOrderbook = (symbol: string, orderbook: OrderBook) => {
    const callbacks = this.orderbookCallbacks.get(symbol);
    if (callbacks) {
      callbacks.forEach((cb) => cb(orderbook));
    }
  };

  private handleCandle = (symbol: string, interval: Timeframe, candle: Candle) => {
    const key = `${symbol}:${interval}`;
    const callbacks = this.candleCallbacks.get(key);
    if (callbacks) {
      callbacks.forEach((cb) => cb(candle));
    }
  };

  private handleMarkets = (markets: Market[]) => {
    this.parent.store.update({
      markets,
      loaded: { ...this.parent.store.loaded, markets: true },
    });
  };

  private onClose = () => {
    if (this.isDisposed) return;

    this.log('Disconnected from public data gateway', LogSeverity.Warning);

    if (this.reconnecting) return;
    this.reconnecting = true;

    this.reconnectCount++;
    if (this.reconnectCount > 100) {
      this.log('Reconnect count exceeded, giving up', LogSeverity.Error);
      return;
    }

    setTimeout(() => {
      if (this.isDisposed) return;
      this.reconnecting = false;
      this.connect();
    }, WS_MIN_RECONNECTION_DELAY);
  };

  private onError = (event: Event) => {
    const errorMessage = (event as any)?.message || 'Unknown error';
    this.log(`Gateway WebSocket error: ${errorMessage}`, LogSeverity.Error);
  };

  /**
   * Subscribe to orderbook updates for a symbol
   */
  subscribeOrderbook = (symbol: string, callback: (orderbook: OrderBook) => void): (() => void) => {
    // Add callback
    if (!this.orderbookCallbacks.has(symbol)) {
      this.orderbookCallbacks.set(symbol, new Set());

      // Send subscribe message only on first subscriber
      this.send({
        type: 'subscribe',
        exchangeId: this.exchangeId,
        channel: 'orderbook',
        symbol,
      });
    }
    this.orderbookCallbacks.get(symbol)!.add(callback);

    // Return unsubscribe function
    return () => {
      const callbacks = this.orderbookCallbacks.get(symbol);
      if (callbacks) {
        callbacks.delete(callback);
        if (callbacks.size === 0) {
          this.orderbookCallbacks.delete(symbol);
          this.send({
            type: 'unsubscribe',
            exchangeId: this.exchangeId,
            channel: 'orderbook',
            symbol,
          });
        }
      }
    };
  };

  /**
   * Subscribe to candle updates for a symbol/interval
   */
  subscribeCandle = (opts: OHLCVOptions, callback: (candle: Candle) => void): (() => void) => {
    const key = `${opts.symbol}:${opts.interval}`;

    // Add callback
    if (!this.candleCallbacks.has(key)) {
      this.candleCallbacks.set(key, new Set());

      // Send subscribe message only on first subscriber
      this.send({
        type: 'subscribe',
        exchangeId: this.exchangeId,
        channel: 'candle',
        symbol: opts.symbol,
        interval: opts.interval,
      });
    }
    this.candleCallbacks.get(key)!.add(callback);

    // Return unsubscribe function
    return () => {
      const callbacks = this.candleCallbacks.get(key);
      if (callbacks) {
        callbacks.delete(callback);
        if (callbacks.size === 0) {
          this.candleCallbacks.delete(key);
          this.send({
            type: 'unsubscribe',
            exchangeId: this.exchangeId,
            channel: 'candle',
            symbol: opts.symbol,
            interval: opts.interval,
          });
        }
      }
    };
  };

  /**
   * Re-subscribe to all active subscriptions after reconnect
   */
  private resubscribeAll = () => {
    // Re-subscribe orderbooks
    for (const symbol of this.orderbookCallbacks.keys()) {
      this.send({
        type: 'subscribe',
        exchangeId: this.exchangeId,
        channel: 'orderbook',
        symbol,
      });
    }

    // Re-subscribe candles
    for (const key of this.candleCallbacks.keys()) {
      const [symbol, interval] = key.split(':');
      this.send({
        type: 'subscribe',
        exchangeId: this.exchangeId,
        channel: 'candle',
        symbol,
        interval: interval as Timeframe,
      });
    }
  };

  private send = (message: Omit<GatewayMessage, 'type'> & { type: GatewayMessageType }) => {
    if (this.ws?.readyState !== WebSocket.OPEN) {
      this.log('Cannot send - gateway not connected', LogSeverity.Warning);
      return;
    }
    this.ws.send(JSON.stringify(message));
  };

  private log = (message: string, severity: LogSeverity) => {
    this.parent.emitter.emit('log', `[${this.parent.name}:Gateway] ${message}`, severity);
  };

  /**
   * Check if provider is connected
   */
  get isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Dispose the provider and clean up
   */
  dispose = () => {
    this.isDisposed = true;
    this.orderbookCallbacks.clear();
    this.candleCallbacks.clear();
    this.ws?.close();
    this.ws = undefined;
  };
}
