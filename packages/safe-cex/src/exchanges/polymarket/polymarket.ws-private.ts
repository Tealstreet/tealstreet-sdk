/**
 * Polymarket Private WebSocket Handler
 *
 * Handles private user data subscriptions:
 * - Order status updates
 * - Trade executions
 * - Position changes
 *
 * Requires authentication with API credentials.
 *
 * Reference: https://docs.polymarket.com/developers/CLOB/websocket/wss-overview
 */

import { LogSeverity, Order, OrderSide, OrderStatus, OrderType } from '../../types';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import type { PolymarketExchange } from './polymarket.exchange';
import {
  ORDER_SIDE_MAP,
  ORDER_STATUS_MAP,
  ORDER_TYPE_MAP,
  POLYMARKET_CONFIG,
  WS_CONFIG,
  WS_MESSAGE_TYPES,
} from './polymarket.constants';
import type {
  PolymarketAPIOrder,
  PolymarketCredentials,
  PolymarketWSOrderUpdate,
  PolymarketWSSubscription,
  PolymarketWSTrade,
} from './polymarket.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class PolymarketPrivateWebSocket extends BaseWebSocket<PolymarketExchange> {
  connectUrl = POLYMARKET_CONFIG.WS_USER;

  private pingInterval?: NodeJS.Timeout;
  private getCredentials: () => PolymarketCredentials | null;

  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePongEvent()],
    pong: [() => this.handlePongEvent()],
    order: [(data: Data) => this.handleOrderUpdate(data as PolymarketWSOrderUpdate)],
    trade: [(data: Data) => this.handleTradeUpdate(data as PolymarketWSTrade)],
  };

  constructor(parent: PolymarketExchange, getCredentials: () => PolymarketCredentials | null) {
    super(parent);
    this.getCredentials = getCredentials;
  }

  connectAndSubscribe = () => {
    const credentials = this.getCredentials();
    if (!credentials) {
      this.parent.log('Cannot connect private WebSocket: no credentials', LogSeverity.Warning);
      return;
    }

    if (!this.isDisposed && !this.ws) {
      this.parent.log('Connecting to Polymarket private WebSocket...', LogSeverity.Info);

      this.ws = this.createWebsocket(this.connectUrl);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('error', ((event: Event) => {
        this.parent.log(
          `Private WebSocket error: ${(event as any)?.message || 'Unknown error'}`,
          LogSeverity.Error,
        );
      }) as any);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.parent.log('Polymarket private WebSocket connected', LogSeverity.Info);

      // Send authentication and subscribe to USER channel
      this.authenticate();

      // Start ping/pong
      this.ping();
    }
  };

  private authenticate = () => {
    const credentials = this.getCredentials();
    if (!credentials) {
      this.parent.log('Cannot authenticate: no credentials', LogSeverity.Error);
      return;
    }

    const subscription: PolymarketWSSubscription = {
      type: WS_MESSAGE_TYPES.USER,
      auth: {
        apiKey: credentials.apiKey,
        secret: credentials.secret,
        passphrase: credentials.passphrase,
      },
    };

    this.send(JSON.stringify(subscription));
    this.parent.log('Sent private WebSocket authentication', LogSeverity.Debug);
  };

  onMessage = (event: MessageEvent) => {
    this.trackLastReceived();

    if (event.data === 'PONG') {
      this.handlePongEvent();
      return;
    }

    const data = jsonParse(event.data) as any;
    if (!data) return;

    // Handle different event types
    const eventType = data.event_type || data.type;

    if (eventType === 'pong') {
      this.handlePongEvent();
      return;
    }

    // Handle authentication response
    if (eventType === 'authenticated' || data.authenticated) {
      this.parent.log('Private WebSocket authenticated', LogSeverity.Info);
      // Fetch initial snapshot
      this.parent.fetchAndSetPrivateSnapshot();
      return;
    }

    // Handle errors
    if (eventType === 'error' || data.error) {
      this.parent.log(`Private WebSocket error: ${data.error || data.message}`, LogSeverity.Error);
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
    this.parent.log('Polymarket private WebSocket closed', LogSeverity.Warning);
    this.clearPingInterval();
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
    super.dispose();
  };

  // ==================== Message Handlers ====================

  /**
   * Handle order update events
   */
  private handleOrderUpdate = (data: PolymarketWSOrderUpdate) => {
    try {
      const apiOrder = data as PolymarketAPIOrder;
      const order = this.mapOrder(apiOrder);

      // Update store
      this.store.addOrUpdateOrders([order]);

      // Emit fill event if order was filled
      if (apiOrder.status.toLowerCase() === 'matched') {
        this.emitter.emit('fill', {
          id: order.id,
          side: order.side,
          symbol: order.symbol,
          price: order.price,
          amount: order.filled,
        });
      }

      this.parent.log(`Order update: ${order.id} - ${order.status}`, LogSeverity.Debug);
    } catch (err) {
      this.parent.log(`Error handling order update: ${err}`, LogSeverity.Error);
    }
  };

  /**
   * Handle trade/fill events
   */
  private handleTradeUpdate = (data: PolymarketWSTrade) => {
    try {
      const trade = data;

      // Get symbol from token ID
      const symbol = (this.parent as any).tokenToSymbol?.get(trade.asset_id) || trade.asset_id;

      // Emit fill event
      this.emitter.emit('fill', {
        id: trade.id,
        side: ORDER_SIDE_MAP[trade.side] || OrderSide.Buy,
        symbol,
        price: parseFloat(trade.price),
        amount: parseFloat(trade.size),
      });

      this.parent.log(
        `Trade: ${trade.side} ${trade.size} ${symbol} @ ${trade.price}`,
        LogSeverity.Debug,
      );

      // Update positions after trade
      this.parent.fetchPositions();
    } catch (err) {
      this.parent.log(`Error handling trade update: ${err}`, LogSeverity.Error);
    }
  };

  /**
   * Map Polymarket API order to internal Order type
   */
  private mapOrder(apiOrder: PolymarketAPIOrder): Order {
    const symbol = (this.parent as any).tokenToSymbol?.get(apiOrder.asset_id) || apiOrder.asset_id;

    return {
      id: apiOrder.id,
      status: (ORDER_STATUS_MAP[apiOrder.status] || apiOrder.status) as OrderStatus,
      symbol,
      type: ORDER_TYPE_MAP[apiOrder.order_type] || OrderType.Limit,
      side: ORDER_SIDE_MAP[apiOrder.side] || OrderSide.Buy,
      price: parseFloat(apiOrder.price),
      amount: parseFloat(apiOrder.original_size),
      filled: parseFloat(apiOrder.size_matched),
      remaining: parseFloat(apiOrder.original_size) - parseFloat(apiOrder.size_matched),
      reduceOnly: false,
      info: apiOrder,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };
  }

}
