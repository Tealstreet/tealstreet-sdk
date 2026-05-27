import throttle from 'lodash/throttle';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { Balance, LogSeverity, Order, OrderStatus, Position, PositionSide } from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { ArkhamExchange } from './arkham.exchange';
import { ORDER_SIDE } from './arkham.types';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

export class ArkhamPrivateWebSocket extends BaseWebSocket<ArkhamExchange> {
  // Arkham's private stream is event-driven (orders/positions/balances) and
  // routed through our proxy. Both server-initiated `ping` frames and `pong`
  // echoes already route through handlePongEvent via messageHandlers, but
  // the pong-echo path can be delayed by the proxy. Any inbound traffic on
  // this socket is sufficient proof of liveness.
  protected override treatAnyMessageAsPong = true;

  connectUrl = '';
  private pingInterval?: NodeJS.Timeout;
  private authenticated = false;

  // Throttled fetch properties for orders
  private lastOrderFetchTime = 0;
  private orderFetchThrottleMs = 250;
  private orderFetchScheduled = false;
  private hasPendingOrderEvents = false;

  // Throttled fetch properties for positions
  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 250;
  private positionFetchScheduled = false;
  private hasPendingPositionEvents = false;

  messageHandlers: MessageHandlers = {
    ping: [() => this.handlePongEvent()],
    pong: [() => this.handlePongEvent()],
    auth: [(data: Data) => this.handleAuthResponse(data)],
    order_statuses: [(data: Data) => this.handleOrderUpdate(data)],
    trigger_orders: [(data: Data) => this.handleTriggerOrderUpdate(data)],
    positions: [(data: Data) => this.handlePositionUpdate(data)],
    balances: [(data: Data) => this.handleBalanceUpdate(data)],
  };

  constructor(parent: ArkhamExchange) {
    super(parent);
    // Use the proxy WebSocket server
    this.connectUrl = 'wss://fsjnpy143pbz.jp.tealstreet.io/arkm';
  }

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      // Connect to the proxy WebSocket - auth happens after connection
      this.ws = this.createWebsocket(this.connectUrl);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('error', ((event: Event) => {
        this.parent.log(`WebSocket connection error: ${(event as any)?.message || 'Unknown error'}`, LogSeverity.Error);
      }) as any);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.parent.log('Arkham private WebSocket connected to proxy');

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
      this.subscribeToPrivateChannels();
      return;
    }

    if (data.type === 'error') {
      this.parent.log(`WebSocket proxy error: ${data.error}`, LogSeverity.Error);
      return;
    }

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
    if (this.isConnected) {
      this.send(JSON.stringify({ type: 'ping' }));
    }

    this.pingInterval = setTimeout(() => {
      if (!this.isDisposed && this.isConnected) {
        this.ping();
      }
    }, 30000);
  };

  private handleOrderUpdate = throttle(
    (data: any) => {
      try {
        const order = data;
        const side = ORDER_SIDE[order.side];
        const status = this.mapOrderStatus(order.status);
        const orderId = String(order.orderId);

        this.parent.log(`Order update: ${orderId} ${order.symbol} ${order.status} → ${status}`, LogSeverity.Info);

        const update: Order = {
          id: orderId,
          symbol: order.symbol,
          side,
          type: this.parent['reverseMapOrderType'](order.type),
          status,
          price: order.price ? parseFloat(order.price) : 0,
          amount: parseFloat(order.size),
          filled: parseFloat(order.executedSize || '0'),
          remaining:
            order.size && order.executedSize
              ? parseFloat(order.size) - parseFloat(order.executedSize)
              : parseFloat(order.size || '0'),
          reduceOnly: order.reduceOnly || false,
          timeInForce: undefined,
          info: order,
          lastUpdate: Date.now(),
        };

        // Always update the order with its current status (including Canceled/Closed)
        // This allows users to see their order history
        this.parent.store.addOrUpdateOrder(update);

        // Trigger throttled fetch to get full order state
        this.hasPendingOrderEvents = true;
        this.throttledFetchOrders();
      } catch (err) {
        this.parent.log(`Error handling order update: ${err}`, LogSeverity.Error);
      }
    },
    100,
    { leading: true, trailing: true },
  );

  private handleTriggerOrderUpdate = (data: any) => {
    try {
      const triggerOrder = data;
      const side = ORDER_SIDE[triggerOrder.side];
      const status = this.mapTriggerOrderStatus(triggerOrder.status);
      const orderId = String(triggerOrder.triggerOrderId);

      this.parent.log(
        `Trigger order update: ${orderId} ${triggerOrder.symbol} ${triggerOrder.status} → ${status}`,
        LogSeverity.Info,
      );

      const update: Order = {
        id: orderId,
        symbol: triggerOrder.symbol,
        side,
        type: this.parent['mapTriggerOrderType'](triggerOrder.triggerType, side),
        status,
        price: triggerOrder.triggerPrice ? parseFloat(triggerOrder.triggerPrice) : 0,
        amount: parseFloat(triggerOrder.size),
        filled: 0, // Trigger orders don't have filled amounts until triggered
        remaining: parseFloat(triggerOrder.size),
        reduceOnly: triggerOrder.reduceOnly || false,
        timeInForce: undefined,
        info: triggerOrder,
        lastUpdate: Date.now(),
      };

      // Always update the order with its current status (including Canceled)
      this.parent.store.addOrUpdateOrder(update);

      // Trigger throttled fetch to get full order state
      this.hasPendingOrderEvents = true;
      this.throttledFetchOrders();
    } catch (err) {
      this.parent.log(`Error handling trigger order update: ${err}`, LogSeverity.Error);
    }
  };

  private handlePositionUpdate = throttle(
    (data: any) => {
      try {
        const p = data;

        // Determine side based on base position (positive = long, negative = short)
        const baseAmount = parseFloat(p.base);
        const side = baseAmount >= 0 ? PositionSide.Long : PositionSide.Short;

        if (baseAmount === 0) {
          // Position closed - remove it
          // Note: The store will handle removal of zero positions
        } else {
          // Calculate leverage from initial margin: leverage = value / initialMargin
          const value = Math.abs(parseFloat(p.value));
          const initialMargin = parseFloat(p.initialMargin);
          let leverage = initialMargin > 0 ? value / initialMargin : 1;

          // If leverage is not reasonable, try to get it from fetched leverage
          if (!leverage || leverage === 0 || !isFinite(leverage)) {
            const fetchedLeverage = this.parent.store.fetchedLeverage[p.symbol];
            if (fetchedLeverage && !fetchedLeverage.isFetching) {
              leverage = side === PositionSide.Long ? fetchedLeverage.buyLeverage : fetchedLeverage.sellLeverage;
            } else {
              leverage = 1; // Fallback to default leverage
            }
          }

          // Position update
          const update: Position = {
            id: createPositionId(p.symbol, side),
            symbol: p.symbol,
            side,
            contracts: Math.abs(baseAmount),
            notional: Math.abs(parseFloat(p.quote)),
            unrealizedPnl: parseFloat(p.pnl),
            realizedPnl: 0, // Arkham doesn't provide realized PnL per position
            leverage,
            entryPrice: parseFloat(p.averageEntryPrice),
            liquidationPrice: 0, // Arkham doesn't provide liquidation price directly
            info: p,
          };

          this.parent.store.addOrUpdatePosition(update);
        }

        // Update balance with recalculated used margin from positions
        const positions = this.parent.store.positions;
        const usedMargin = positions.reduce((sum, pos) => {
          const initialMargin = pos.info?.initialMargin ? parseFloat(pos.info.initialMargin) : 0;
          return sum + initialMargin;
        }, 0);

        if (usedMargin > 0) {
          const currentBalance = this.parent.store.balance;
          this.parent.store.update({
            balance: {
              ...currentBalance,
              used: usedMargin,
              free: currentBalance.total - usedMargin,
            },
          });
        }

        // Trigger throttled fetch to get full position state
        this.hasPendingPositionEvents = true;
        this.throttledFetchPositions();
      } catch (err) {
        this.parent.log(`Error handling position update: ${err}`);
      }
    },
    100,
    { leading: true, trailing: true },
  );

  private handleBalanceUpdate = throttle(
    (data: any) => {
      try {
        const balances = Array.isArray(data) ? data : [data];

        // Sum up USDT/USDC balances
        const usdBalances = balances.filter(
          (b: any) => b.symbol === 'USDT' || b.symbol === 'USDC' || b.symbol === 'USD',
        );

        // Only update if we have USD balance updates, otherwise skip
        // (prevents zeroing out balance when receiving non-USD token updates like ARKM)
        if (usdBalances.length === 0) {
          return;
        }

        const total = usdBalances.reduce((sum: number, b: any) => sum + parseFloat(b.balance), 0);
        const free = usdBalances.reduce((sum: number, b: any) => sum + parseFloat(b.free), 0);
        const used = total - free; // Calculate locked as difference

        const balance: Balance = {
          total,
          free,
          used,
          upnl: 0, // Will be calculated from positions
        };

        // Calculate unrealized PnL from positions
        const positions = this.parent.store.positions;
        if (positions.length > 0) {
          // upnl will be calculated elsewhere
        }

        this.parent.store.update({ balance });
      } catch (err) {
        this.parent.log(`Error handling balance update: ${err}`);
      }
    },
    1000,
    { leading: true, trailing: true },
  );

  private handleAuthResponse = (data: any) => {
    if (data.success || data.authenticated) {
      this.authenticated = true;
      this.parent.log('Arkham private WebSocket authenticated');
      this.subscribeToPrivateChannels();
    } else {
      this.parent.log(`Arkham authentication failed: ${data.error}`);
    }
  };

  private authenticate = async () => {
    const crypto = require('crypto');
    const timestamp = virtualClock.getCurrentTimeMs() * 1000; // Convert to microseconds
    const expiry = timestamp + 5 * 1000000; // 5 seconds in the future (in microseconds)
    const method = 'GET';
    const requestPath = '/ws';
    const message = `${this.parent.options.key}${expiry}${method}${requestPath}`;

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: 'arkham',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: message,
          outputFormat: 'arkham-base64',
          extraData: { expiry },
        },
      });

      if (result.error) {
        this.emitAndLogError(`Remote WS auth failed: ${result.error}`);
        return;
      }

      const authMessage = {
        type: 'auth',
        apikey: this.parent.options.key,
        expires: expiry.toString(),
        signature: result.wsAuthSignature,
      };

      this.send(JSON.stringify(authMessage));
      return;
    }

    // Generate proxy auth parameters
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

  private subscribeToPrivateChannels = () => {
    // Subscribe to order updates
    this.send(
      JSON.stringify({
        method: 'subscribe',
        args: {
          channel: 'order_statuses',
        },
      }),
    );

    // Subscribe to trigger order updates
    this.send(
      JSON.stringify({
        method: 'subscribe',
        args: {
          channel: 'trigger_orders',
        },
      }),
    );

    // Subscribe to positions
    this.send(
      JSON.stringify({
        method: 'subscribe',
        args: {
          channel: 'positions',
        },
      }),
    );

    // Subscribe to balances
    this.send(
      JSON.stringify({
        method: 'subscribe',
        args: {
          channel: 'balances',
        },
      }),
    );
  };

  private mapOrderStatus = (status: string): OrderStatus => {
    const mapping: Record<string, OrderStatus> = {
      new: OrderStatus.Open,
      taker: OrderStatus.Open,
      booked: OrderStatus.Open,
      maker: OrderStatus.Open,
      cancelled: OrderStatus.Canceled,
      closed: OrderStatus.Closed,
    };
    return mapping[status] || OrderStatus.Open;
  };

  private mapTriggerOrderStatus = (status: string): OrderStatus => {
    const mapping: Record<string, OrderStatus> = {
      staged: OrderStatus.Open, // Trigger order is waiting to be triggered
      triggered: OrderStatus.Closed, // Trigger order was triggered and converted to regular order
      cancelled: OrderStatus.Canceled, // Trigger order was cancelled
      failed: OrderStatus.Canceled, // Trigger order failed
    };
    return mapping[status] || OrderStatus.Open;
  };

  private throttledFetchOrders = () => {
    const now = Date.now();

    if (this.isDisposed) {
      return;
    }

    // If a fetch is already scheduled, mark as having pending events
    // and let the scheduled fetch handle it
    if (this.orderFetchScheduled) {
      return;
    }

    const timeSinceLastFetch = now - this.lastOrderFetchTime;

    if (timeSinceLastFetch >= this.orderFetchThrottleMs) {
      // Execute immediately if enough time has passed
      this.executeOrderFetch();
    } else {
      // Schedule for later if we need to throttle
      this.orderFetchScheduled = true;
      setTimeout(() => {
        this.orderFetchScheduled = false;
        if (!this.isDisposed) {
          // Always execute if we had pending events
          if (this.hasPendingOrderEvents) {
            this.executeOrderFetch();
          }
        }
      }, this.orderFetchThrottleMs - timeSinceLastFetch);
    }
  };

  private executeOrderFetch = async () => {
    try {
      this.lastOrderFetchTime = Date.now();
      this.hasPendingOrderEvents = false;
      await this.parent.fetchOrders();
    } catch {
      // Silent failure, will retry on next event
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
      await this.parent.fetchPositions();
    } catch {
      // Silent failure, will retry on next event
    }
  };
}
