import type { BinanceSpotExchange } from './binancespot.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_PWS_LOG } from '../../config/config';
import { OrderStatus, OrderType } from '../../types';
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
    events.forEach((data) => {
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
    events.forEach((event) => {
      const currencies = { ...(this.store.balance.currencies || {}) };
      const coins = { ...(this.store.balance.coins || {}) };

      for (const assetBalance of event.B || []) {
        const asset = assetBalance.a;
        const free = parseFloat(assetBalance.f || '0');
        const used = parseFloat(assetBalance.l || '0');
        const total = free + used;
        const price = this.parent.getAssetUsdPrice(asset);

        currencies[asset] = { free, used, total, upnl: 0, info: assetBalance };
        coins[asset] = { base: asset, amount: total, notional: total * price };
      }

      let free = 0;
      let used = 0;
      let total = 0;
      for (const [asset, balance] of Object.entries(currencies)) {
        const price = this.parent.getAssetUsdPrice(asset);
        free += balance.free * price;
        used += balance.used * price;
        total += balance.total * price;
      }

      this.store.update({
        balance: { free, used, total, upnl: 0, currencies, coins, info: event },
      });
    });
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
