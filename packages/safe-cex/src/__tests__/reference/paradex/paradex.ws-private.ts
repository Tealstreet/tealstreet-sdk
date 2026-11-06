import { Balance, LogSeverity, MarginType, Order, OrderStatus, Position, PositionMode, PositionSide } from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { ParadexExchange } from './paradex.exchange';
import { BASE_WS_URL, isParadexTriggerOrderType, ORDER_SIDE, paradexTimeInForce } from './paradex.types';
import { decodeParadexMessage } from './paradex.sbe';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class ParadexPrivateWebSocket extends BaseWebSocket<ParadexExchange> {
  connectUrl = '';
  private authenticated = false;
  private messageId = 0;
  private readonly seenFills = new Set<string>();
  private authRequestId?: number;
  private ordersThrottleTimeout?: NodeJS.Timeout;
  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 500;
  private positionFetchScheduled = false;
  private hasPendingPositionEvents = false;

  messageHandlers: MessageHandlers = {};

  constructor(parent: ParadexExchange) {
    super(parent);
    this.connectUrl = BASE_WS_URL.private[parent.options.testnet ? 'testnet' : 'livenet'];

    // Initialize message handlers
    // Note: Use channel prefix matching to handle channels like "orders.ALL", "orders.BTC-USD-PERP", etc.
    this.messageHandlers = {
      'ping': [() => this.handlePongEvent()],
      'pong': [() => this.handlePongEvent()],
      'orders.ALL': [(data: Data) => this.handleOrderUpdate(data)],
      'positions': [(data: Data) => this.handlePositionUpdate(data)],
      'balance_events': [(data: Data) => this.handleBalanceUpdate(data)],
      'fills.ALL': [(data: Data) => this.handleFillUpdate(data)],
    };
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      this.ws = this.createWebsocket(this.connectUrl);
      this.ws.binaryType = 'arraybuffer';
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = async () => {
    if (!this.isDisposed) {
      this.authenticated = false;
      this.authRequestId = undefined;
      this.parent.log('Paradex private WebSocket connected', LogSeverity.Info);
      await this.authenticate();
    }
  };

  onMessage = (event: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      let data: Data | null;
      try {
        data = decodeParadexMessage(event.data);
      } catch (error) {
        this.parent.log(`Invalid Paradex websocket frame: ${error}`, LogSeverity.Warning);
        return;
      }
      if (!data) return;

      // Log raw message for debugging (commented out to reduce log spam)
      // this.parent.log(`Private WS message: ${JSON.stringify(data)}`, LogSeverity.Debug);

      // Handle JSON-RPC 2.0 format
      if (data.jsonrpc === '2.0') {
        // Handle subscription data
        if (data.method === 'subscription' && data.params) {
          const channel = data.params.channel;
          const handlers = this.messageHandlers[channel];
          if (handlers) {
            // Record message stats to track WebSocket health
            this.recordMessageHandlerStats(channel);
            handlers.forEach((handler) => handler(data.params));
          } else {
            this.parent.log(`No handler registered for channel: ${channel}`, LogSeverity.Warning);
          }
        }

        // Handle result responses (auth, subscribe, etc.)
        if (data.result !== undefined) {
          // this.parent.log(`WebSocket result for request ${data.id}: ${JSON.stringify(data.result)}`, LogSeverity.Debug);

          // Authentication response to latest auth request.
          // The documented result is {node_id}; older servers returned {}.
          if (this.authRequestId !== undefined && data.id === this.authRequestId && !this.authenticated) {
            const result = data.result;
            if (!result || typeof result !== 'object' || Array.isArray(result) ||
              Object.keys(result).some((key) => key !== 'node_id') ||
              ('node_id' in result && (typeof result.node_id !== 'string' || !result.node_id))) return;
            this.authenticated = true;
            this.parent.log('Paradex private WebSocket authenticated', LogSeverity.Info);

            // Verify websocket is still connected before subscribing
            if (!this.isConnected) {
              this.parent.log('WebSocket disconnected after authentication, closing connection', LogSeverity.Warning);
              this.ws?.close();
              return;
            }

            this.subscribeToPrivateChannels();
          }

          // Log subscription confirmations
          if (data.result && typeof data.result === 'object' && data.result.channel) {
            this.parent.log(`Successfully subscribed to channel: ${data.result.channel}`, LogSeverity.Info);
          }
        }

        // Handle errors
        if (data.error) {
          this.parent.log(`WebSocket error for request ${data.id}: ${data.error.message}`, LogSeverity.Error);
          if (this.authRequestId !== undefined && data.id === this.authRequestId) {
            this.authenticated = false;
          }
          // this.parent.log(`Error details: ${JSON.stringify(data.error)}`, LogSeverity.Debug);
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

  private handleOrderUpdate = (data: any) => {
    try {
      const order = data.data;
      const side = ORDER_SIDE[order.side];
      const status = order.status === 'CLOSED'
        ? order.cancel_reason || Number(order.remaining_size) > 0 ? OrderStatus.Canceled : OrderStatus.Closed
        : this.mapOrderStatus(order.status);
      const reduceOnly =
        typeof order.reduce_only === 'boolean'
          ? order.reduce_only
          : Array.isArray(order.flags)
            ? order.flags.includes('REDUCE_ONLY')
            : false;
      const orderType = this.parent['reverseMapOrderType'](order.type, reduceOnly);

      // For stop orders, use trigger_price as the price (since _MARKET types have no limit price)
      const isStopOrder = isParadexTriggerOrderType(order.type);
      const displayPrice = isStopOrder && order.trigger_price ? parseFloat(order.trigger_price) : (order.price ? parseFloat(order.price) : 0);

      const update: Order = {
        id: order.id,
        symbol: order.market,
        side,
        type: orderType,
        status,
        price: displayPrice,
        amount: parseFloat(order.size),
        filled: parseFloat(order.size) - parseFloat(order.remaining_size),
        remaining: parseFloat(order.remaining_size),
        reduceOnly,
        clientOrderId: order.client_id || order.client_order_id || undefined,
        timeInForce: paradexTimeInForce(order),
        lastUpdate: Date.now(),
        info: order, // Store all raw data from WebSocket
      };

      // Check for cancel reason and emit error notification
      // This happens even if order isn't in store (e.g., immediate rejections)
      if (order.cancel_reason && order.cancel_reason !== '') {
        const cancelReason = this.formatCancelReason(order.cancel_reason);
        const errorMessage = `Order ${order.market} ${order.side} rejected: ${cancelReason}`;
        this.parent.emitAndLogError(errorMessage);
        this.parent.log(
          `Order ${order.id} cancel reason: ${order.cancel_reason}`,
          LogSeverity.Warning
        );
      }

      // Always update the order in the store with the current status
      // This includes Canceled status, which allows UI to display canceled orders
      // The store will handle removing old orders automatically
      this.parent.store.addOrUpdateOrder(update);
      if (status === OrderStatus.Closed || status === OrderStatus.Canceled) {
        this.parent.store.markOrdersTerminal([update], status);
      }

      this.parent.log(
        `Order ${order.id} status updated to ${status} (raw: ${order.status})`,
        LogSeverity.Info
      );

      // Refresh orders after any order event processing (similar to Binance)
      this.throttledFetchOrders();
    } catch (err) {
      this.parent.log(`Error handling order update: ${err}`, LogSeverity.Error);
    }
  };

  /**
   * Format cancel reason into user-friendly message
   */
  private formatCancelReason = (reason: string): string => {
    const reasonMap: Record<string, string> = {
      'ORDER_SIZE_BELOW_MIN': 'Order size below minimum required',
      'INSUFFICIENT_MARGIN': 'Insufficient margin/balance',
      'PRICE_OUT_OF_RANGE': 'Price outside allowed range',
      'MARKET_CLOSED': 'Market is closed',
      'POST_ONLY_WOULD_CROSS': 'Post-only order would execute immediately',
      'INVALID_PRICE': 'Invalid order price',
      'INVALID_SIZE': 'Invalid order size',
      'MAX_OPEN_ORDERS': 'Maximum open orders reached',
      'SELF_TRADE_PREVENTION': 'Self-trade prevention triggered',
      'USER_CANCELED': 'Canceled by user',
    };

    return reasonMap[reason] || reason.replace(/_/g, ' ').toLowerCase();
  };

  private handleFillUpdate = (data: Data) => {
    const rows = Array.isArray(data.data) ? data.data : [data.data];
    for (const fill of rows) {
      if (!fill || typeof fill.id !== 'string' || !fill.id || this.seenFills.has(fill.id)) continue;
      const side = ORDER_SIDE[fill.side];
      const price = Number(fill.price);
      const amount = Number(fill.size);
      if (!side || !fill.order_id || !fill.market || !Number.isFinite(price) || price <= 0 ||
          !Number.isFinite(amount) || amount <= 0) continue;
      this.seenFills.add(fill.id);
      if (this.seenFills.size > 8192) this.seenFills.delete(this.seenFills.values().next().value!);
      this.parent.emitter.emit('fill', { id: String(fill.order_id), symbol: fill.market, side, price, amount });
    }
  };

  private handlePositionUpdate = (data: any) => {
    try {
      // data contains { channel: "positions", data: {...} }
      const position = data.data;

      if (!position || !position.market) {
        this.parent.log('Invalid position update - missing data or market', LogSeverity.Warning);
        return;
      }

      if (position.status === 'CLOSED' || parseFloat(position.size) === 0) {
        const id = createPositionId(position.market, position.side === 'LONG' ? PositionSide.Long : PositionSide.Short, PositionMode.OneWay);
        this.parent.store.updatePositions([[{ id }, { contracts: 0, notional: 0, unrealizedPnl: 0 }]]);
        this.hasPendingPositionEvents = true;
        this.throttledFetchPositions();
        return;
      }

      // Mark that we have pending position events for throttled fetch
      this.hasPendingPositionEvents = true;

      // Position update
      const entryPrice = parseFloat(position.average_entry_price);
      const contracts = Math.abs(parseFloat(position.size));

      const update: Position = {
        id: createPositionId(
          position.market,
          position.side === 'LONG' ? PositionSide.Long : PositionSide.Short,
          PositionMode.OneWay
        ),
        symbol: position.market,
        side: position.side === 'LONG' ? PositionSide.Long : PositionSide.Short,
        contracts,
        notional: contracts * entryPrice,
        unrealizedPnl: parseFloat(position.unrealized_pnl),
        realizedPnl: parseFloat(position.realized_positional_pnl || '0'),
        leverage: parseFloat(position.leverage || '0'),
        entryPrice,
        liquidationPrice: parseFloat(position.liquidation_price || '0'),
        marginType: MarginType.Cross,
        positionMode: PositionMode.OneWay,
        lastUpdate: position.last_updated_at || position.created_at,
        info: position, // Store all raw data from WebSocket
      };

      this.parent.log(
        `Position update: ${update.symbol} ${update.side} ${update.contracts} @ ${update.entryPrice}`,
        LogSeverity.Debug
      );

      this.parent.store.addOrUpdatePosition(update);

      // Refresh positions after any position event processing (similar to Binance)
      this.throttledFetchPositions();
    } catch (err) {
      this.parent.log(`Error handling position update: ${err}`, LogSeverity.Error);
    }
  };

  private handleBalanceUpdate = (data: any) => {
    try {
      const payload = data?.data ?? data;
      const account =
        Array.isArray(payload)
          ? payload.find((entry) => entry && typeof entry === 'object')
          : payload?.data && typeof payload.data === 'object'
            ? payload.data
            : payload;

      if (!account || typeof account !== 'object') {
        this.parent.log(`Invalid balance payload: ${JSON.stringify(data)}`, LogSeverity.Warning);
        return;
      }

      const readNumber = (...values: unknown[]): number | undefined => {
        for (const value of values) {
          if (value === undefined || value === null) {
            continue;
          }
          const parsed = Number.parseFloat(String(value));
          if (Number.isFinite(parsed)) {
            return parsed;
          }
        }
        return undefined;
      };

      const total =
        readNumber(
          (account as Record<string, unknown>).total_collateral,
          (account as Record<string, unknown>).account_value,
          (account as Record<string, unknown>).equity,
          (account as Record<string, unknown>).margin_balance,
          (account as Record<string, unknown>).wallet_balance,
        ) ?? 0;
      const free =
        readNumber(
          (account as Record<string, unknown>).free_collateral,
          (account as Record<string, unknown>).free,
          (account as Record<string, unknown>).available_margin,
          (account as Record<string, unknown>).available_collateral,
        ) ?? 0;
      const used =
        readNumber(
          (account as Record<string, unknown>).initial_margin_requirement,
          (account as Record<string, unknown>).position_margin,
          (account as Record<string, unknown>).used_margin,
        ) ?? Math.max(total - free, 0);

      const balance: Balance = {
        total,
        free,
        used,
        upnl: this.parent.store.positions.reduce((sum, position) => sum + position.unrealizedPnl, 0),
      };

      // Calculate unrealized PnL from positions
      const positions = this.parent.store.positions;
      if (positions.length > 0) {
        // upnl will be calculated elsewhere as it's readonly
      }

      this.parent.store.update({ balance });

      // Balance update stored
    } catch (err) {
      this.parent.log(`Error handling balance update: ${err}`, LogSeverity.Error);
    }
  };

  private throttledFetchPositions = () => {
    const now = Date.now();

    if (this.isDisposed) {
      return;
    }

    // If a fetch is already scheduled, mark as having pending events
    // and let the scheduled fetch handle it
    if (this.positionFetchScheduled) {
      return;
    }

    const timeSinceLastFetch = now - this.lastPositionFetchTime;

    if (timeSinceLastFetch >= this.positionFetchThrottleMs) {
      // Execute immediately if enough time has passed
      this.executePositionFetch();
    } else {
      // Schedule for later if we need to throttle
      this.positionFetchScheduled = true;
      setTimeout(() => {
        this.positionFetchScheduled = false;
        if (!this.isDisposed) {
          // Always execute if we had pending events
          if (this.hasPendingPositionEvents) {
            this.executePositionFetch();
          }
        }
      }, this.positionFetchThrottleMs - timeSinceLastFetch);
    }
  };

  private executePositionFetch = async () => {
    try {
      this.lastPositionFetchTime = Date.now();
      this.hasPendingPositionEvents = false;
      const positions = await this.parent.fetchPositions();
      if (this.isDisposed || this.parent.isDisposed) return;
      this.parent.store.addOrUpdatePositionsFull(positions);
    } catch {
      // Silent failure, will retry on next event
    }
  };

  private authenticate = async () => {
    try {
      // Wait for SDK initialization if needed
      if (this.parent.sdkInitPromise) {
        await this.parent.sdkInitPromise;
      }

      // Get JWT token from manager
      if (this.parent.jwtManager) {
        const token = await this.parent.jwtManager.getToken();
        const requestId = this.messageId++;
        this.authRequestId = requestId;

        // Send authentication message using JSON-RPC 2.0 format
        this.send(JSON.stringify({
          jsonrpc: '2.0',
          method: 'auth',
          params: { bearer: token },
          id: requestId,
        }));

        // this.parent.log('Paradex WebSocket authentication sent', LogSeverity.Debug);
      } else {
        // this.parent.log('Cannot authenticate WebSocket: JWT manager not initialized', LogSeverity.Debug);
      }
    } catch (error) {
      this.parent.log(`Failed to authenticate WebSocket: ${error}`, LogSeverity.Error);
    }
  };

  private subscribeToPrivateChannels = () => {
    this.send(JSON.stringify({
      jsonrpc: '2.0', method: 'subscribe', params: { channel: 'account' }, id: this.messageId++,
    }));
    // Subscribe to orders for ALL markets using JSON-RPC 2.0 format
    // Channel format: "orders.ALL" subscribes to all order updates across all markets
    this.send(JSON.stringify({
      jsonrpc: '2.0',
      method: 'subscribe',
      params: { channel: 'orders.ALL' },
      id: this.messageId++,
    }));

    // this.parent.log('Subscribed to orders.ALL for order updates', LogSeverity.Debug);

    this.send(JSON.stringify({
      jsonrpc: '2.0', method: 'subscribe', params: { channel: 'fills.ALL' }, id: this.messageId++,
    }));

    // Subscribe to positions
    this.send(JSON.stringify({
      jsonrpc: '2.0',
      method: 'subscribe',
      params: { channel: 'positions' },
      id: this.messageId++,
    }));

    // this.parent.log('Subscribed to positions for position updates', LogSeverity.Debug);

    // Subscribe to balance events
    // Note: Using balance_events instead of balances for account updates
    this.send(JSON.stringify({
      jsonrpc: '2.0',
      method: 'subscribe',
      params: { channel: 'balance_events' },
      id: this.messageId++,
    }));
  };

  private mapOrderStatus = (status: string): OrderStatus => {
    const mapping: Record<string, OrderStatus> = {
      NEW: OrderStatus.Open,
      OPEN: OrderStatus.Open,
      PENDING: OrderStatus.Open,
      UNTRIGGERED: OrderStatus.Open,
      FILLED: OrderStatus.Closed,
      CANCELED: OrderStatus.Canceled,
      REJECTED: OrderStatus.Canceled,
    };
    return mapping[status] || OrderStatus.Open;
  };

  // The upstream uses RFC6455 control pings, answered by the socket library.
  protected treatAnyMessageAsPong = true;
  ping = () => undefined;
  getWatchdogStaleThresholdMs = () => 90_000;

  dispose = () => {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
      this.ordersThrottleTimeout = undefined;
    }
    super.dispose();
  };

  private throttledFetchOrders = () => {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
    }
    this.ordersThrottleTimeout = setTimeout(this.fetchThrottledOrders, 1000);
  };

  private fetchThrottledOrders = async () => {
    this.ordersThrottleTimeout = undefined;
    if (!this.isDisposed) {
      try {
        const orders = await this.parent.fetchOrders();
        this.parent.store.update({
          orders,
          loaded: {
            ...this.parent.store.loaded,
            orders: true,
          },
        });
      } catch {
        // Silent failure, will retry on next event
      }
    }
  };
}
