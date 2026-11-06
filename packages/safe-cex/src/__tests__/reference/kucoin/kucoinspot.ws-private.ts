import type { KucoinSpotExchange } from './kucoinspot.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { OrderStatus } from '../../../types';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { KUCOIN_SPOT_ENDPOINTS } from './kucoinspot.types';

type Data = Record<string, any>;

const normalizeMillis = (timestamp: unknown): number => {
  const parsed = parseInt(`${timestamp ?? Date.now()}`, 10);
  if (parsed > 100_000_000_000_000_000) return Math.floor(parsed / 1_000_000);
  if (parsed > 100_000_000_000_000) return Math.floor(parsed / 1_000);
  if (parsed < 10_000_000_000) return parsed * 1000;
  return parsed;
};

export class KucoinSpotPrivateWebsocket extends BaseWebSocket<KucoinSpotExchange> {
  token: string | null = null;
  socketEndpoint: string | null = null;
  pingInterval = 5_000;
  private orderRefreshTimeout?: NodeJS.Timeout;
  private balanceRefreshTimeout?: NodeJS.Timeout;

  connectAndSubscribe = async () => {
    if (this.isDisposed) return;

    try {
      const { data } = await this.parent.xhr.post(KUCOIN_SPOT_ENDPOINTS.GET_PRIVATE_WS_TOKEN);

      if (data.code !== '200000' || !data.data) {
        this.parent.emitAndLogError(data.msg || 'Failed to get KuCoin Spot private WebSocket token');
        return;
      }

      const server = data.data.instanceServers?.[0];
      if (!server?.endpoint) {
        this.parent.emitAndLogError('No KuCoin Spot private WebSocket servers available');
        return;
      }

      this.token = data.data.token;
      this.pingInterval = server.pingInterval || this.pingInterval;
      this.socketEndpoint = `${server.endpoint}?token=${this.token}&connectId=${Date.now()}`;

      this.ws = this.createWebsocket(this.socketEndpoint);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    } catch (err: any) {
      this.retryFailedConnect(
        err?.response?.data?.msg || err?.message || 'Failed to connect KuCoin Spot private WebSocket',
      );
    }
  };

  onOpen = () => {
    if (this.isDisposed) return;

    this.subscribe();
    this.ping();

    if (ENABLE_RECONNECTING_WEBSOCKET) {
      this.parent.fetchAndSetPrivateSnapshot(10);
    }
  };

  ping = () => {
    if (this.isDisposed) return;

    this.pingAt = performance.now();
    this.sendWithLog(JSON.stringify({ id: Date.now(), type: 'ping' }));
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;

    this.trackLastReceived();
    this.emitWsDiagnosticReceive(data);

    const json = jsonParse<Data>(data);
    if (!json) return;

    if (json.type === 'pong') {
      this.handlePongEvent(this.pingInterval);
      return;
    }

    if (json.topic?.startsWith('/spotMarket/tradeOrdersV2')) {
      this.recordMessageHandlerStats('order');
      this.handleOrderTopic(json);
      return;
    }

    if (json.topic?.startsWith('/account/balance')) {
      this.recordMessageHandlerStats('balance');
      this.refreshBalanceSoon();
    }
  };

  handleOrderTopic = (message: Data) => {
    const rawOrder = message.data;
    if (!rawOrder?.orderId) {
      this.refreshOrdersSoon();
      return;
    }

    // KuCoin Spot tradeOrdersV2 carries the EVENT in `type`
    // (received | open | match | filled | canceled | update) and the order's
    // lifecycle state in `status` (new | open | match | done). Branch on the
    // event `type` — collapsing to `status || type` mislabels a cancel
    // (`type:canceled, status:done`) as Closed instead of Canceled (the REST
    // heal can't fix it; see fetchOrders symbol requirement).
    const eventType = rawOrder.type;
    const order = this.parent.mapOrder({
      ...rawOrder,
      id: rawOrder.orderId,
      orderId: rawOrder.orderId,
      status: rawOrder.status,
      type: rawOrder.orderType || rawOrder.orderTypeV2,
      price: rawOrder.price || rawOrder.orderPrice || rawOrder.matchPrice,
      size: rawOrder.size || rawOrder.orderSize,
      dealSize: rawOrder.dealSize || rawOrder.filledSize || rawOrder.matchSize,
      lastUpdatedAt: normalizeMillis(rawOrder.ts || rawOrder.orderTime || rawOrder.createdAt),
    });

    if (eventType === 'match') {
      this.emitter.emit('fill', {
        id: order.id,
        side: order.side,
        symbol: order.symbol,
        price: parseFloat(rawOrder.matchPrice || rawOrder.price || '0'),
        amount: parseFloat(rawOrder.matchSize || rawOrder.filledSize || '0'),
      });
    }

    if (eventType === 'canceled' || eventType === 'cancelled') {
      this.store.updateOrderStatus(order, OrderStatus.Canceled);
    } else if (eventType === 'filled') {
      this.store.updateOrderStatus(order, OrderStatus.Closed);
    } else {
      this.store.addOrUpdateOrder(order);
    }

    this.refreshOrdersSoon();
  };

  private subscribe = () => {
    for (const topic of ['/spotMarket/tradeOrdersV2', '/account/balance']) {
      this.sendWithLog(
        JSON.stringify({
          id: Date.now(),
          type: 'subscribe',
          topic,
          privateChannel: true,
          response: true,
        }),
      );
    }
  };

  private refreshOrdersSoon = () => {
    if (this.orderRefreshTimeout || this.isDisposed) return;

    this.orderRefreshTimeout = setTimeout(async () => {
      this.orderRefreshTimeout = undefined;
      if (this.isDisposed) return;
      await this.parent.fetchAndSetOrders();
    }, 250);
  };

  private refreshBalanceSoon = () => {
    if (this.balanceRefreshTimeout || this.isDisposed) return;

    this.balanceRefreshTimeout = setTimeout(async () => {
      this.balanceRefreshTimeout = undefined;
      if (this.isDisposed) return;

      const balance = await this.parent.fetchBalance();
      if (this.isDisposed) return;

      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    }, 250);
  };

  dispose() {
    if (this.orderRefreshTimeout) {
      clearTimeout(this.orderRefreshTimeout);
      this.orderRefreshTimeout = undefined;
    }

    if (this.balanceRefreshTimeout) {
      clearTimeout(this.balanceRefreshTimeout);
      this.balanceRefreshTimeout = undefined;
    }

    super.dispose();
  }
}
