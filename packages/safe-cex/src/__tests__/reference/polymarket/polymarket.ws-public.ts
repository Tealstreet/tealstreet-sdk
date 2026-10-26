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

import { LogSeverity, OrderBook, Ticker } from '../../../types';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import type { PolymarketExchange } from './polymarket.exchange';
import { POLYMARKET_CONFIG, WS_CONFIG, WS_MESSAGE_TYPES } from './polymarket.constants';
import type {
  PolymarketWSOrderBookUpdate,
  PolymarketWSPriceChange,
  PolymarketWSSubscription,
} from './polymarket.types';

type Data = Record<string, any>;
type LiveBook = { bids: Map<number, number>; asks: Map<number, number>; timestamp: number };
const number = (value: unknown): number => typeof value === 'number' ? value : typeof value === 'string' && value.trim() ? Number(value) : NaN;
const validPrice = (value: number) => Number.isFinite(value) && value >= 0 && value <= 1;
const validSize = (value: number) => Number.isFinite(value) && value >= 0;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class PolymarketPublicWebSocket extends BaseWebSocket<PolymarketExchange> {
  // Runs its own close loop; online recovery falls back to an account reload.
  protected readonly recoversAfterOffline = false;

  connectUrl = POLYMARKET_CONFIG.WS_MARKET;

  private pingInterval?: NodeJS.Timeout;
  private subscribedAssets = new Set<string>();
  private books = new Map<string, LiveBook>();
  private quoteTimes = new Map<string, number>();
  private tradeTimes = new Map<string, number>();
  private tickTimes = new Map<string, number>();

  // Order book callbacks
  private orderbookCallbacks = new Map<string, Set<(ob: OrderBook) => void>>();

  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePongEvent()],
    pong: [() => this.handlePongEvent()],
    price_change: [(data: Data) => this.handlePriceChange(data as PolymarketWSPriceChange)],
    book: [(data: Data) => this.handleOrderBookUpdate(data as PolymarketWSOrderBookUpdate)],
    last_trade_price: [(data: Data) => this.handleLastTradePrice(data)],
    tick_size_change: [(data: Data) => this.handleTickSizeChange(data)],
    market_resolved: [(data: Data) => this.handleMarketResolved(data)],
  };

  constructor(parent: PolymarketExchange) {
    super(parent);
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed && !this.ws) {
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
    if (assetIds.length === 0) {
      return;
    }

    const subscription: PolymarketWSSubscription = {
      type: WS_MESSAGE_TYPES.MARKET,
      assets_ids: assetIds,
      custom_feature_enabled: true,
    };

    this.send(JSON.stringify(subscription));
    this.parent.log(`Subscribed to ${assetIds.length} assets`, LogSeverity.Debug);
  };

  onMessage = (event: MessageEvent) => {
    this.trackLastReceived();

    if (event.data === 'PONG') {
      this.handlePongEvent();
      return;
    }

    const data = jsonParse(event.data) as any;
    if (!data) return;

    // Initial CLOB snapshots may contain several direct event objects.
    for (const message of Array.isArray(data) ? data : [data]) {
      if (!message || typeof message !== 'object' || Array.isArray(message)) continue;
      const eventType = message.event_type || message.type;
      const handlers = this.messageHandlers[eventType];
      if (Array.isArray(handlers)) {
        this.recordMessageHandlerStats(eventType);
        handlers.forEach((handler) => handler(message));
      }
    }
  };

  onClose = () => {
    // No close event is available on this path, so only the count is durable.
    this.recordCloseDiagnostics(undefined, { syntheticIfNoSocket: true });
    this.parent.log('Polymarket public WebSocket closed', LogSeverity.Warning);
    this.clearPingInterval();
    this.books.clear();
    this.ws = undefined;

    // Schedule reconnection
    if (!this.isDisposed) {
      setTimeout(() => this.connectAndSubscribe(), WS_CONFIG.RECONNECT_DELAY);
    }
  };

  ping = () => {
    if (this.isConnected) {
      this.send('PING');
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
    this.orderbookCallbacks.clear();
    this.books.clear();
    this.quoteTimes.clear();
    this.tradeTimes.clear();
    this.tickTimes.clear();
    super.dispose();
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
          custom_feature_enabled: true,
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
    this.books.delete(tokenId);

    if (this.isConnected) {
      this.send(
        JSON.stringify({
          assets_ids: [tokenId],
          operation: WS_MESSAGE_TYPES.UNSUBSCRIBE,
          custom_feature_enabled: true,
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
  private symbol = (tokenId: string): string | undefined =>
    this.store.marketsById[tokenId]?.symbol ?? (this.parent as any).tokenToSymbol?.get(tokenId);

  private timestamp = (data: Data, fallback = Date.now()): number | undefined => {
    const timestamp = data.timestamp === undefined ? fallback : number(data.timestamp);
    return Number.isFinite(timestamp) && timestamp >= 0 ? timestamp : undefined;
  };

  private emitBook = (tokenId: string, book: LiveBook) => {
    const levels = (side: Map<number, number>, descending: boolean) => {
      let total = 0;
      return [...side].sort(([a], [b]) => descending ? b - a : a - b).map(([price, amount]) => {
        total += amount;
        return { price, amount, total };
      });
    };
    const orderbook: OrderBook = { bids: levels(book.bids, true), asks: levels(book.asks, false) };
    this.orderbookCallbacks.get(tokenId)?.forEach((callback) => callback(orderbook));
  };

  private handlePriceChange = (data: PolymarketWSPriceChange) => {
    const timestamp = this.timestamp(data);
    if (timestamp === undefined || !Array.isArray(data.price_changes)) return;
    for (const change of data.price_changes) {
      if (!change || typeof change !== 'object') continue;
      const tokenId = change.asset_id;
      const symbol = this.symbol(tokenId);
      const price = number(change.price);
      const size = number(change.size);
      if (!symbol || !validPrice(price) || !validSize(size) || !['BUY', 'SELL'].includes(change.side)) continue;
      if (timestamp < (this.quoteTimes.get(tokenId) ?? 0)) continue;
      this.quoteTimes.set(tokenId, timestamp);
      const book = this.books.get(tokenId);
      if (book && timestamp >= book.timestamp) {
        const side = change.side === 'BUY' ? book.bids : book.asks;
        if (size === 0) side.delete(price);
        else side.set(price, size);
        book.timestamp = timestamp;
        this.emitBook(tokenId, book);
      }
      const bid = number(change.best_bid);
      const ask = number(change.best_ask);
      const update: Partial<Ticker> = {
        id: tokenId, symbol,
        ...(validPrice(bid) ? { bid } : {}),
        ...(validPrice(ask) ? { ask } : {}),
        ...(validPrice(bid) && validPrice(ask) ? { mark: (bid + ask) / 2, index: (bid + ask) / 2 } : {}),
      };
      this.store.updateTicker({ id: tokenId }, update);
    }
  };

  private handleLastTradePrice = (data: Data) => {
    const tokenId = data.asset_id;
    const symbol = this.symbol(tokenId);
    const last = number(data.price);
    const timestamp = this.timestamp(data);
    if (!symbol || !validPrice(last) || timestamp === undefined || timestamp < (this.tradeTimes.get(tokenId) ?? 0)) return;
    this.tradeTimes.set(tokenId, timestamp);
    this.store.updateTicker({ id: tokenId }, { id: tokenId, symbol, last });
  };

  private handleOrderBookUpdate = (data: PolymarketWSOrderBookUpdate) => {
    const tokenId = data.asset_id;
    if (!this.orderbookCallbacks.get(tokenId)?.size || !Array.isArray(data.bids) || !Array.isArray(data.asks)) return;
    const timestamp = this.timestamp(data, 0);
    if (timestamp === undefined || timestamp < (this.books.get(tokenId)?.timestamp ?? 0)) return;
    const bids = new Map<number, number>();
    const asks = new Map<number, number>();
    // Validate both sides before replacing the authoritative book: malformed
    // snapshots must not clear a previously healthy side or install a prefix.
    for (const [side, levels] of [[bids, data.bids], [asks, data.asks]] as const) {
      for (const level of levels) {
        if (!level) return;
        const price = number(level.price);
        const size = number(level.size);
        if (!validPrice(price) || !validSize(size)) return;
        if (size === 0) side.delete(price);
        else side.set(price, size);
      }
    }
    const book = { bids, asks, timestamp };
    this.books.set(tokenId, book);
    this.quoteTimes.set(tokenId, Math.max(this.quoteTimes.get(tokenId) ?? 0, timestamp));
    this.emitBook(tokenId, book);
  };

  private handleTickSizeChange = (data: Data) => {
    const tokenId = data.asset_id;
    const tickSize = number(data.new_tick_size);
    const market = this.store.marketsById[tokenId];
    const timestamp = this.timestamp(data);
    if (!market || ![0.1, 0.01, 0.005, 0.0025, 0.001, 0.0001].includes(tickSize) || timestamp === undefined || timestamp < (this.tickTimes.get(tokenId) ?? 0)) return;
    this.tickTimes.set(tokenId, timestamp);
    this.store.updateMarket({ id: tokenId }, {
      info: { ...market.info, tickSize },
      precision: { ...market.precision, price: (String(tickSize).split('.')[1] ?? '').length },
    });
  };

  private handleMarketResolved = (data: Data) => {
    if (!Array.isArray(data.assets_ids)) return;
    for (const tokenId of data.assets_ids) {
      if (typeof tokenId === 'string' && this.store.marketsById[tokenId]) this.store.updateMarket({ id: tokenId }, { active: false });
    }
  };
}
