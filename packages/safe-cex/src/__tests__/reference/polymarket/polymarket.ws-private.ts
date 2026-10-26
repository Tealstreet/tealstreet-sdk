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

import { LogSeverity, Order, OrderSide, OrderStatus, OrderType } from '../../../types';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
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
type MakerExecution = {
  order_id: string;
  owner?: string;
  maker_address?: string;
  asset_id?: string;
  side?: 'BUY' | 'SELL';
  matched_amount: string;
  price: string;
};
type UserTrade = PolymarketWSTrade & {
  owner?: string;
  trade_owner?: string;
  trader_side?: 'TAKER' | 'MAKER';
  maker_orders?: MakerExecution[];
};

export class PolymarketPrivateWebSocket extends BaseWebSocket<PolymarketExchange> {
  // Runs its own close loop; online recovery falls back to an account reload.
  protected readonly recoversAfterOffline = false;

  connectUrl = POLYMARKET_CONFIG.WS_USER;

  private pingInterval?: NodeJS.Timeout;
  private getCredentials: () => PolymarketCredentials | null;
  private executionIds = new Set<string>();
  private matchedSizes = new Map<string, number>();
  private orderTimes = new Map<string, number>();

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
      this.refreshPositions('authentication');
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
    // No close event is available on this path, so only the count is durable.
    this.recordCloseDiagnostics(undefined, { syntheticIfNoSocket: true });
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
      const wire = data as PolymarketWSOrderUpdate & { type?: string; timestamp?: string | number };
      if (!wire.id) return;
      let timestamp = wire.timestamp === undefined ? virtualClock.getCurrentTimeMs() : Number(wire.timestamp);
      if (!Number.isFinite(timestamp)) return;
      if (timestamp < 1e12) timestamp *= 1000;
      if ((this.orderTimes.get(wire.id) ?? -Infinity) > timestamp) return;

      const status = wire.status?.toUpperCase();
      if (wire.type === 'CANCELLATION' || status === 'CANCELED' || status === 'CANCELLED') {
        this.orderTimes.set(wire.id, timestamp);
        this.store.markOrdersTerminal([{ id: wire.id }], OrderStatus.Canceled);
        this.store.removeOrders([{ id: wire.id }]);
        return;
      }
      const existing = this.store.orders.find((order) => order.id === wire.id);
      if (status && !['LIVE', 'MATCHED', 'DELAYED', 'UNMATCHED'].includes(status)) return;
      const merged = { ...existing?.info, ...wire } as PolymarketAPIOrder;
      if (!ORDER_SIDE_MAP[merged.side]) return;
      const mapped = this.mapOrder(merged);
      if (!Number.isFinite(mapped.price) || mapped.price <= 0 || mapped.price >= 1
        || !Number.isFinite(mapped.amount) || mapped.amount <= 0
        || !Number.isFinite(mapped.filled) || mapped.filled < 0 || mapped.filled > mapped.amount) return;
      this.orderTimes.set(wire.id, timestamp);
      const filled = Math.max(mapped.filled, existing?.filled ?? 0, this.matchedSizes.get(mapped.id) ?? 0);
      const order: Order = { ...mapped, filled, remaining: Math.max(0, mapped.amount - filled), lastUpdate: timestamp, status: OrderStatus.Open };
      this.matchedSizes.set(order.id, order.filled);
      if (order.remaining === 0) {
        this.store.markOrdersTerminal([{ id: order.id }], OrderStatus.Closed);
        this.store.removeOrders([{ id: order.id }]);
      } else {
        // MATCHED notifications contain cumulative size, including partials.
        this.store.addOrUpdateOrders([order]);
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
      const trade = data as UserTrade;
      const status = trade.status?.replace(/^TRADE_STATUS_/, '') ?? '';
      if (!trade.id || !['MATCHED', 'MATCHED_NOT_BROADCASTED', 'MINED', 'CONFIRMED'].includes(status)) return;
      const credentials = this.getCredentials();
      if (!credentials) return;
      const funder = ((this.parent as any).funderAddress || (this.parent as any).wallet?.address || '').toLowerCase();
      const makers = trade.maker_orders ?? [];
      const ownsMaker = (maker: MakerExecution) => maker.owner === credentials.apiKey
        || (!!funder && maker.maker_address?.toLowerCase() === funder)
        || this.store.orders.some((order) => order.id === maker.order_id);
      const taker = trade.trader_side === 'TAKER' || (trade.trader_side !== 'MAKER'
        && (this.store.orders.some((order) => order.id === trade.taker_order_id)
          || (!makers.some(ownsMaker) && (trade.owner === credentials.apiKey || trade.trade_owner === credentials.apiKey))));
      if (taker) this.emitExecution(trade.id, trade.taker_order_id, trade.asset_id, trade.side, trade.price, trade.size);
      makers.filter(ownsMaker).forEach((maker) => {
        const asset = maker.asset_id || trade.asset_id;
        const existing = this.store.orders.find((order) => order.id === maker.order_id);
        const side = maker.side || (existing?.side === OrderSide.Buy ? 'BUY' : existing?.side === OrderSide.Sell ? 'SELL' : undefined)
          || (asset === trade.asset_id ? trade.side === 'BUY' ? 'SELL' : trade.side === 'SELL' ? 'BUY' : undefined : undefined);
        if (side) this.emitExecution(trade.id, maker.order_id, asset, side, maker.price, maker.matched_amount);
      });
    } catch (err) {
      this.parent.log(`Error handling trade update: ${err}`, LogSeverity.Error);
    } finally {
      // Settlement failures and replayed notifications still reconcile holdings.
      this.refreshPositions('trade update');
    }
  };

  private emitExecution = (tradeId: string, orderId: string, asset: string, side: string, priceText: string, amountText: string) => {
    if (priceText == null || String(priceText).trim() === '') return;
    const price = Number(priceText);
    const amount = Number(amountText);
    const mappedSide = ORDER_SIDE_MAP[side];
    const key = `${tradeId}:${orderId}`;
    if (!orderId || !asset || !mappedSide || !Number.isFinite(price) || price < 0 || price > 1
      || !Number.isFinite(amount) || amount <= 0 || this.executionIds.has(key)) return;
    this.executionIds.add(key);
    this.emitter.emit('fill', {
      id: orderId,
      side: mappedSide,
      symbol: (this.parent as any).tokenToSymbol?.get(asset) || asset,
      price,
      amount,
    });
  };

  private refreshPositions = (reason: string) => {
    void this.parent.fetchAndSetPositions().catch((err) => {
      this.parent.log(`Error refreshing positions after ${reason}: ${err}`, LogSeverity.Error);
    });
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
