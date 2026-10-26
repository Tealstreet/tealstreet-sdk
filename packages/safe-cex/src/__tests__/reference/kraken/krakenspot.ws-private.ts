import type { KrakenSpotExchange } from './krakenspot.exchange';

import { LogSeverity, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../../types';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ORDER_TYPE } from './krakenspot.types';

// Kraken Spot WS v2 private channels use token-based auth:
// 1. Call POST /0/private/GetWebSocketsToken via REST to get a session token
// 2. Include token in every private channel subscription params
// Token must be used within 15 minutes; stays valid while any subscription is active.

export class KrakenSpotPrivateWebsocket extends BaseWebSocket<KrakenSpotExchange> {
  authenticated = false;
  private wsToken: string | undefined;
  private ordersThrottleTimeout?: NodeJS.Timeout;

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const url = BASE_WS_URL.private.livenet;
      this.ws = this.createWebsocket(url);

      const handleClose = () => this.onClose();
      const handleError = () => {
        try {
          if (this.ws && this.ws.readyState !== WebSocket.CLOSED) this.ws.close();
        } catch (_e) {}
        handleClose();
      };

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage as any);
      this.ws.addEventListener('close', handleClose);
      this.ws.addEventListener('error', handleError);
    }
  };

  setToken = (token: string) => {
    this.wsToken = token;
  };

  onOpen = async () => {
    if (!this.isDisposed) {
      // Tokens must be used within 15 minutes of creation. A token cached by a
      // long-lived socket can therefore be stale when that socket reconnects;
      // fetch a new one for every connection before subscribing.
      this.authenticated = false;
      this.wsToken = undefined;
      try {
        const token = await this.parent.fetchWsToken();
        if (this.isDisposed) return;
        this.wsToken = token;
      } catch {
        this.parent.log('Failed to fetch WebSocket token', LogSeverity.Error);
        return;
      }

      if (!this.wsToken) {
        this.parent.log('No WebSocket token available', LogSeverity.Error);
        return;
      }

      this.authenticated = true;

      // Subscribe to private channels with token
      this.send(
        JSON.stringify({
          method: 'subscribe',
          params: {
            channel: 'executions',
            token: this.wsToken,
            snap_orders: true,
            snap_trades: false,
            order_status: true,
          },
        }),
      );

      this.send(
        JSON.stringify({
          method: 'subscribe',
          params: {
            channel: 'balances',
            token: this.wsToken,
            snapshot: true,
          },
        }),
      );
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      try {
        const json = typeof data === 'string' ? JSON.parse(data) : JSON.parse((data as any).toString?.() || '{}');

        // Handle subscription confirmations
        if (json?.method) {
          if (json.method === 'subscribe' && !json.success) {
            this.parent.log(`WS subscription error: ${json.error}`, LogSeverity.Error);
            if (json.error?.includes('token')) {
              this.authenticated = false;
              this.wsToken = undefined;
            }
          }
          return;
        }

        // Heartbeat
        if (json?.channel === 'heartbeat') {
          return;
        }

        const channel = json?.channel;
        if (!channel) return;

        if (json.type !== 'snapshot') {
          this.recordMessageHandlerStats(channel);
        }

        switch (channel) {
          case 'executions':
            this.handleExecutions(json);
            break;
          case 'balances':
            this.handleBalances(json);
            break;
        }
      } catch (e) {
        console.error('krakenspot.ws-private parse error', e, 'Raw data:', data);
      }
    }
  };

  private handleExecutions = (msg: any) => {
    // WS v2 format: { channel: "executions", type: "snapshot"|"update", data: [{ order_id, symbol, side, order_type, limit_price, order_qty, cum_qty, avg_price, exec_type, fees, timestamp, ... }] }
    const items = msg.data;
    if (!Array.isArray(items)) return;

    if (msg.type === 'snapshot') {
      // Full snapshot of open orders — replace all
      const orders: Order[] = [];
      for (const item of items) {
        const order = this.parseOrder(item);
        if (order) orders.push(order);
      }
      this.store.addOrUpdateOrdersFull(orders);
      return;
    }

    // Incremental updates
    for (const item of items) {
      const execType = item.exec_type;

      if (execType === 'canceled' || execType === 'expired') {
        const orderId = item.order_id;
        if (orderId) {
          const existingOrder = this.store.orders.find((o) => o.id === orderId);
          if (existingOrder) {
            this.store.addOrUpdateOrder({
              ...existingOrder,
              status: OrderStatus.Canceled,
            });
          }
        }
        this.throttledFetchOrders();
        continue;
      }

      if (execType === 'filled' || execType === 'trade') {
        // Fill event — update order quantities
        const orderId = item.order_id;
        if (orderId) {
          const existingOrder = this.store.orders.find((o) => o.id === orderId);
          if (existingOrder) {
            const cumQty = parseFloat(item.cum_qty || '0');
            const orderQty = parseFloat(item.order_qty || existingOrder.amount.toString());
            const remaining = Math.max(0, orderQty - cumQty);

            this.store.addOrUpdateOrder({
              ...existingOrder,
              filled: cumQty,
              remaining: remaining,
              status: remaining === 0 ? OrderStatus.Closed : OrderStatus.Open,
            });

            // Emit fill event
            this.parent.emitter.emit('fill', {
              side: item.side === 'buy' ? OrderSide.Buy : OrderSide.Sell,
              symbol: item.symbol,
              price: parseFloat(item.avg_price || item.limit_price || '0'),
              amount: parseFloat(item.last_qty || item.cum_qty || '0'),
            });
          }
        }
        this.throttledFetchOrders();
        continue;
      }

      // New order or update (pending_new, new, etc.)
      const order = this.parseOrder(item);
      if (order) {
        this.store.addOrUpdateOrder(order);
      }
    }
  };

  private parseOrder = (item: any): Order | null => {
    const orderId = item.order_id;
    const wireSymbol = item.symbol;
    if (!orderId || !wireSymbol) return null;

    const market = this.store.marketsBySymbol[wireSymbol] ?? this.store.marketsById[wireSymbol];
    if (!market) return null;
    const symbol = market.symbol;

    const orderType = ORDER_TYPE[item.order_type] || OrderType.Limit;
    const side = item.side === 'buy' ? OrderSide.Buy : OrderSide.Sell;

    const orderQty = parseFloat(item.order_qty || '0');
    const cumQty = parseFloat(item.cum_qty || '0');
    const remaining = Math.max(0, orderQty - cumQty);

    let status = OrderStatus.Open;
    const execType = item.exec_type;
    if (execType === 'canceled' || execType === 'expired') {
      status = OrderStatus.Canceled;
    } else if (execType === 'filled' || remaining === 0) {
      status = OrderStatus.Closed;
    }

    // Parse time in force from order flags
    let timeInForce = OrderTimeInForce.GoodTillCancel;
    if (item.time_in_force === 'IOC') {
      timeInForce = OrderTimeInForce.ImmediateOrCancel;
    }
    if (item.post_only === true) {
      timeInForce = OrderTimeInForce.PostOnly;
    }

    const trigger = item.triggers?.actual_price ??
      (item.triggers?.price_type === 'static' ? item.triggers?.price : undefined);
    const price = parseFloat(trigger ?? item.limit_price ?? item.stop_price ?? item.trigger_price ?? '0');
    const timestamp = item.timestamp ? new Date(item.timestamp).getTime() : Date.now();

    return {
      id: orderId,
      status,
      symbol,
      type: orderType,
      side,
      price,
      amount: orderQty,
      filled: cumQty,
      remaining,
      reduceOnly: false, // Spot doesn't have reduce-only
      info: item,
      timeInForce,
      lastUpdate: timestamp,
    };
  };

  private handleBalances = (msg: any) => {
    // WS v2 format: { channel: "balances", type: "snapshot"|"update", data: [{ asset, balance, wallets: [{type, id, balance}] }] }
    const items = msg.data;
    if (!Array.isArray(items)) return;

    // Notify parent to refresh balance via REST
    // WS balance updates can be partial; a full REST fetch ensures accuracy
    this.parent.refreshBalance();
  };

  private throttledFetchOrders = () => {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
    }

    this.ordersThrottleTimeout = setTimeout(async () => {
      try {
        await this.parent.fetchAndSetOrders();
      } catch (_e) {
        // Will retry on next event
      }
    }, 1_000);
  };
}
