import type { BinanceSpotExchange } from './binancespot.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_PWS_LOG } from '../../config/config';
import { OrderStatus, OrderType, PositionSide } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, ENDPOINTS, ORDER_SIDE, ORDER_TYPE_SPOT } from './binancespot.types';

export class BinancePrivateWebsocket extends BaseWebSocket<BinanceSpotExchange> {
  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
      const listenKey = await this.fetchListenKey();

      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const base = BASE_WS_URL.private[key];

      const url = `${base}/${listenKey}`;

      this.ws = this.createWebsocket(url);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('open', this.onOpen);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.ping();
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }

      const json = jsonParse(data);

      if (json?.e === 'outboundAccountPosition') {
        this.recordMessageHandlerStats('position');
        this.handleAccountEvents([json]);
      }
      if (json?.e === 'executionReport') {
        this.recordMessageHandlerStats('order');
        this.handleOrderEvents([json]);
      }

      if (json?.id === 42) {
        this.handlePongEvent();
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ id: 42, method: 'LIST_SUBSCRIPTIONS' }));
    }
  };

  handleOrderEvents = (events: Array<Record<string, any>>) => {
    events.forEach(({ o: data }) => {
      // Spot `executionReport`: `x` is the execution type (NEW/TRADE/CANCELED/…),
      // `X` is the order status (NEW/PARTIALLY_FILLED/FILLED/CANCELED/…). Fills
      // arrive with `x === 'TRADE'`.
      if (data.x === 'TRADE') {
        this.parent.emitter.emit('fill', {
          id: data.c,
          side: ORDER_SIDE[data.S],
          symbol: data.s,
          price: parseFloat(data.p),
          amount: parseFloat(data.l),
        });

        // Mirror the cumulative fill (`z`) onto the stored order so a partially
        // or fully filled order reflects its real filled/remaining before the
        // next REST fetchOrders — same reason as futures (chaser reads
        // order.filled from the store). See binance.ws-private.ts.
        const orderId = data.c;
        const filled = parseFloat(data.z);
        const amount = parseFloat(data.q);
        this.store.updateOrder(
          { id: orderId },
          {
            filled,
            remaining: amount - filled,
            lastUpdate: virtualClock.getCurrentTimeMs(),
          },
        );

        // Fully filled (`z === q`, or order status FILLED) → transition out of Open.
        if (data.X === 'FILLED' || filled >= amount) {
          this.store.updateOrderStatus({ id: orderId }, OrderStatus.Closed);
        }
      }

      if (data.X === 'NEW') {
        this.store.addOrUpdateOrder({
          id: data.c,
          status: OrderStatus.Open,
          symbol: data.s,
          // Spot reports the order type in `o` (LIMIT/STOP_LOSS_LIMIT/LIMIT_MAKER/…),
          // NOT the futures `ot` field. Spot has no reduceOnly concept.
          type: ORDER_TYPE_SPOT[data.o] || OrderType.Limit,
          side: ORDER_SIDE[data.S],
          price: parseFloat(data.p) || parseFloat(data.sp),
          amount: parseFloat(data.q),
          filled: parseFloat(data.z),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: parseFloat(data.q) - parseFloat(data.z),
          reduceOnly: false,
          info: data,
        });
      }

      if (data.X === 'CANCELED' || data.X === 'REJECTED' || data.X === 'EXPIRED') {
        this.store.updateOrderStatus({ id: data.c }, OrderStatus.Canceled);
      }
    });
  };

  handleAccountEvents = (events: Array<Record<string, any>>) => {
    events.forEach((event) =>
      event.a.B.forEach((p: Record<string, any>) => {
        const symbol = p.a;
        const side = PositionSide.Long;

        const position = this.parent.store.positions.find((p2) => p2.symbol === symbol && p2.side === side);

        if (position) {
          //const entryPrice = parseFloat(p.ep);
          const contracts = parseFloat(p.f);
          //const upnl = parseFloat(p.up);

          this.store.updatePosition(position, {
            //entryPrice,
            contracts,
            //notional: contracts * entryPrice + upnl,
            notional: (this.parent.store.tickers.find((t) => t.symbol === symbol)?.last || 0) * contracts,
            unrealizedPnl: 0,
            info: position,
          });
        }
      }),
    );
  };

  private fetchListenKey = async () => {
    const { data } = await this.parent.xhr.post(ENDPOINTS.LISTEN_KEY);
    setTimeout(() => this.updateListenKey(), 30 * 60 * 1000);
    return data.listenKey;
  };

  private updateListenKey = async () => {
    if (!this.isDisposed) {
      await this.parent.xhr.put(ENDPOINTS.LISTEN_KEY);
      setTimeout(() => this.updateListenKey(), 30 * 60 * 1000);
    }
  };
}
