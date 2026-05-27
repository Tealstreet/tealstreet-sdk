/**
 * Polymarket Public WebSocket Handler
 *
 * Handles public market data subscriptions:
 * - Price updates
 * - Order book updates
 * - Trade feed
 *
 * Reference: https://docs.polymarket.com/developers/CLOB/websocket/wss-overview
 */

import throttle from 'lodash/throttle';

import { LogSeverity, OrderBook, Ticker } from '../../types';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { BaseWebSocket } from '../base.ws';
import type { PolymarketExchange } from './polymarket.exchange';
import { POLYMARKET_CONFIG, WS_CONFIG, WS_MESSAGE_TYPES } from './polymarket.constants';
import type {
  PolymarketWSOrderBookUpdate,
  PolymarketWSPriceChange,
  PolymarketWSSubscription,
} from './polymarket.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class PolymarketPublicWebSocket extends BaseWebSocket<PolymarketExchange> {
  connectUrl = POLYMARKET_CONFIG.WS_CLOB;

  private pingInterval?: NodeJS.Timeout;
  private subscribedAssets = new Set<string>();
  private subscribedMarkets = new Set<string>();

  // Order book callbacks
  private orderbookCallbacks = new Map<string, Set<(ob: OrderBook) => void>>();

  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePongEvent()],
    pong: [() => this.handlePongEvent()],
    price_change: [(data: Data) => this.handlePriceChange(data as PolymarketWSPriceChange)],
    book: [(data: Data) => this.handleOrderBookUpdate(data as PolymarketWSOrderBookUpdate)],
  };

  constructor(parent: PolymarketExchange) {
    super(parent);
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.parent.log('Connecting to Polymarket public WebSocket...', LogSeverity.Info);

      this.ws = this.createWebsocket(this.connectUrl);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('error', ((event: Event) => {
        this.parent.log(
          `Public WebSocket error: ${(event as any)?.message || 'Unknown error'}`,
          LogSeverity.Error,
        );
      }) as any);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.parent.log('Polymarket public WebSocket connected', LogSeverity.Info);

      // Subscribe to MARKET channel
      this.sendSubscription();

      // Start ping/pong
      this.ping();

      // Emit reconnected event
      this.emitWsReconnected();
    }
  };

  private sendSubscription = () => {
    const assetIds = Array.from(this.subscribedAssets);
    const markets = Array.from(this.subscribedMarkets);

    if (assetIds.length === 0 && markets.length === 0) {
      return;
    }

    const subscription: PolymarketWSSubscription = {
      type: WS_MESSAGE_TYPES.MARKET,
      assets_ids: assetIds,
      markets,
    };

    this.send(JSON.stringify(subscription));
    this.parent.log(`Subscribed to ${assetIds.length} assets, ${markets.length} markets`, LogSeverity.Debug);
  };

  onMessage = (event: MessageEvent) => {
    this.trackLastReceived();

    const data = jsonParse(event.data) as any;
    if (!data) return;

    // Handle different event types
    const eventType = data.event_type || data.type;

    if (eventType === 'pong') {
      this.handlePongEvent();
      return;
    }

    // Route to appropriate handler
    const handlers = this.messageHandlers[eventType];
    if (handlers) {
      this.recordMessageHandlerStats(eventType);
      handlers.forEach((handler) => handler(data));
    }
  };

  onClose = () => {
    this.parent.log('Polymarket public WebSocket closed', LogSeverity.Warning);
    this.clearPingInterval();

    // Schedule reconnection
    if (!this.isDisposed) {
      setTimeout(() => this.connectAndSubscribe(), WS_CONFIG.RECONNECT_DELAY);
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
    }, WS_CONFIG.PING_INTERVAL);
  };

  private clearPingInterval = () => {
    if (this.pingInterval) {
      clearTimeout(this.pingInterval);
      this.pingInterval = undefined;
    }
  };

  dispose = () => {
    this.clearPingInterval();
    this.subscribedAssets.clear();
    this.subscribedMarkets.clear();
    this.orderbookCallbacks.clear();

    if (this.ws) {
      this.ws.close();
      this.ws = undefined;
    }
  };

  // ==================== Subscription Management ====================

  /**
   * Subscribe to price updates for a token
   */
  subscribeToAsset = (tokenId: string) => {
    if (this.subscribedAssets.has(tokenId)) return;

    this.subscribedAssets.add(tokenId);

    if (this.isConnected) {
      this.send(
        JSON.stringify({
          assets_ids: [tokenId],
          operation: WS_MESSAGE_TYPES.SUBSCRIBE,
        }),
      );
    }
  };

  /**
   * Unsubscribe from price updates for a token
   */
  unsubscribeFromAsset = (tokenId: string) => {
    if (!this.subscribedAssets.has(tokenId)) return;

    this.subscribedAssets.delete(tokenId);

    if (this.isConnected) {
      this.send(
        JSON.stringify({
          assets_ids: [tokenId],
          operation: WS_MESSAGE_TYPES.UNSUBSCRIBE,
        }),
      );
    }
  };

  /**
   * Subscribe to order book for a symbol
   */
  subscribeToOrderBook = (tokenId: string, callback: (ob: OrderBook) => void) => {
    // Add callback
    if (!this.orderbookCallbacks.has(tokenId)) {
      this.orderbookCallbacks.set(tokenId, new Set());
    }
    this.orderbookCallbacks.get(tokenId)!.add(callback);

    // Subscribe if not already
    this.subscribeToAsset(tokenId);

    // Return unsubscribe function
    return () => {
      const callbacks = this.orderbookCallbacks.get(tokenId);
      if (callbacks) {
        callbacks.delete(callback);
        if (callbacks.size === 0) {
          this.orderbookCallbacks.delete(tokenId);
          this.unsubscribeFromAsset(tokenId);
        }
      }
    };
  };

  // ==================== Message Handlers ====================

  /**
   * Handle price change events
   */
  private handlePriceChange = throttle(
    (data: PolymarketWSPriceChange) => {
      try {
        const tokenId = data.asset_id;
        const price = parseFloat(data.price);

        // Get symbol from exchange's mapping
        const symbol = (this.parent as any).tokenToSymbol?.get(tokenId);
        if (!symbol) return;

        const update: Partial<Ticker> = {
          id: tokenId,
          symbol,
          bid: price,
          ask: price,
          last: price,
          mark: price,
          index: price,
        };

        this.store.updateTicker(symbol, update);
      } catch (err) {
        this.parent.log(`Error handling price change: ${err}`, LogSeverity.Error);
      }
    },
    500,
    { leading: true, trailing: true },
  );

  /**
   * Handle order book updates
   */
  private handleOrderBookUpdate = (data: PolymarketWSOrderBookUpdate) => {
    try {
      const tokenId = data.asset_id;
      const callbacks = this.orderbookCallbacks.get(tokenId);

      if (!callbacks || callbacks.size === 0) return;

      // Build order book
      let bidTotal = 0;
      const bids = (data.bids || []).map((b) => {
        const price = parseFloat(b.price);
        const amount = parseFloat(b.size);
        bidTotal += amount;
        return { price, amount, total: bidTotal };
      });

      let askTotal = 0;
      const asks = (data.asks || []).map((a) => {
        const price = parseFloat(a.price);
        const amount = parseFloat(a.size);
        askTotal += amount;
        return { price, amount, total: askTotal };
      });

      const orderBook: OrderBook = { bids, asks };

      // Notify all callbacks
      callbacks.forEach((cb) => cb(orderBook));
    } catch (err) {
      this.parent.log(`Error handling order book update: ${err}`, LogSeverity.Error);
    }
  };

}
