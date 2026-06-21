import type { CoinbaseExchange } from './coinbase.exchange';

import { formatOrderState, formatOrderType, sign } from '@tealstreet/safe-cex/src/exchanges/coinbase/coinbase.utils';
import { Order, OrderSide } from '@tealstreet/safe-cex/src/types';
import { ENABLE_PWS_LOG, ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { resolveExchangeRouting } from '../../routing/resolveExchangeRouting';
import { multiply } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, ORDER_EVENT } from './coinbase.types';

export class CoinbasePrivateWebsocket extends BaseWebSocket<CoinbaseExchange> {
  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const route = resolveExchangeRouting(this.parent.name, this.parent.options).wsPrivate;
      this.ws = this.createWebsocket(
        route.tunnelled ? route.host! : BASE_WS_URL.private[this.parent.options.testnet ? 'testnet' : 'livenet'],
      );
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.subscribe();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.parent.fetchAndSetPrivateSnapshot();
      }
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    this.trackLastReceived();
    if (!this.isDisposed) {
      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }

      const json = JSON.parse(data);
      if (json.channel === 'user') {
        this.handleUserEvent(json);
      } else if (json.channel === 'heartbeats') {
        this.handlePongEvent();
      }
    }
  };

  // no ping for coinbase, only heartbeats
  ping = (): void => {};

  handleUserEvent = (data: Record<string, any>) => {
    const events = data?.events;
    if (events) {
      events.forEach((event: Record<string, any>) => {
        if (event.orders) {
          this.handleUserEventOrders(event as ORDER_EVENT);
        }
      });
    }
  };

  handleUserEventOrders = (data: ORDER_EVENT) => {
    const orders: Order[] = data.orders.map((order) => {
      const filled = parseFloat(order.cumulative_quantity);
      const remainingNative = parseFloat(order.leaves_quantity);
      const amount = filled + remainingNative;
      const market = this.store.markets.find((m) => m.id === order.product_id);
      const contractSize = market?.info.contractSize || 1;
      const remaining = multiply(remainingNative, contractSize);
      return {
        id: order.order_id,
        parentId: order.client_order_id,
        status: formatOrderState(order.status),
        symbol: order.product_id,
        type: formatOrderType(order.order_type),
        side: order.order_side === 'BUY' ? OrderSide.Buy : OrderSide.Sell,
        price: parseFloat(order.stop_price || order.limit_price || '0'),
        amount,
        filled,
        remaining,
        extra: {
          remainingNative,
        },
        reduceOnly: false,
        info: order,
        lastUpdate: virtualClock.getCurrentTimeMs(),
      };
    });
    if (data.type === 'snapshot') {
      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });
    } else if (data.type === 'update') {
      this.store.addOrUpdateOrders(orders);
      // emit fills
      orders.forEach((order) => {
        if (order.info.status === 'FILLED') {
          this.emitter.emit('fill', {
            id: order.id,
            side: order.side,
            symbol: order.symbol,
            price: order.info.avg_price || 'market',
            amount: order.filled,
          });
        }
      });
    }
  };

  handlePositionTopic = (_data: Array<Record<string, any>>) => {};

  private subscribe = async () => {
    for (const channel of ['user', 'heartbeats']) {
      const jwt = await sign(this.parent.options.key, this.parent.options.secret);
      this.sendWithLog(
        JSON.stringify({
          type: 'subscribe',
          channel,
          jwt,
        }),
      );
    }
  };
}
