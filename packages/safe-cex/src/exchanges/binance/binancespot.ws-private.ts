import { ENABLE_PWS_LOG } from '../../config/config';
import { OrderStatus, OrderType, PositionSide } from '../../types';
import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import type { BinanceSpotExchange } from './binancespot.exchange';
import { BASE_WS_URL, ENDPOINTS, ORDER_SIDE } from './binancespot.types';

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
      if (data.X === 'TRADE') {
        this.parent.emitter.emit('fill', {
          id: data.c,
          side: ORDER_SIDE[data.S],
          symbol: data.s,
          price: parseFloat(data.p),
          amount: parseFloat(data.l),
        });
      }

      if (data.X === 'NEW') {
        this.store.addOrUpdateOrder({
          id: data.c,
          status: OrderStatus.Open,
          symbol: data.s,
          //type: ORDER_TYPE[data.ot],
          type: OrderType.Limit, // TODO XXX
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
