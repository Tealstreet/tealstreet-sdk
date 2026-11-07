import type { KucoinExchange } from './kucoin.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { OrderStatus } from '../../../types';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { ENDPOINTS } from './kucoin.types';

export class KucoinPrivateWebsocket extends BaseWebSocket<KucoinExchange> {
  token: string | null = null;
  socketEndpoint: string | null = null;
  private subscribed = false;
  private snapshotRefreshTimer?: ReturnType<typeof setTimeout>;

  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
      try {
        // Get WebSocket token for private connection
        const { data } = await this.parent.xhr.post(ENDPOINTS.GET_PRIVATE_WS_TOKEN);

        if (data.code !== '200000' || !data.data) {
          this.parent.emitAndLogError('Failed to get private WebSocket token');
          return;
        }

        this.token = data.data.token;

        // Construct WebSocket endpoint from the response
        const { instanceServers } = data.data;
        if (!instanceServers || !instanceServers.length) {
          this.parent.emitAndLogError('No private WebSocket servers available');
          return;
        }

        // Use the first server in the list
        const server = instanceServers[0];
        this.socketEndpoint = `${server.endpoint}?token=${this.token}&connectId=${Date.now()}`;

        // Create WebSocket connection through proxy if needed
        const endpoint = this.socketEndpoint;
        this.ws = this.createWebsocket(endpoint);
        this.ws.addEventListener('open', this.onOpen);
        this.ws.addEventListener('message', this.onMessage);
        this.ws.addEventListener('close', this.onClose);
      } catch (err: any) {
        this.retryFailedConnect(err?.response?.data?.msg || err?.message || 'Failed to connect to private WebSocket');
      }
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.subscribed = false;
      this.subscribe();
      this.ping();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.parent.fetchAndSetPrivateSnapshot();
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ id: Date.now(), type: 'ping' }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);

      const json = jsonParse<Record<string, any>>(data);
      if (!json) return;

      if (json.type === 'pong') {
        this.handlePongEvent();
        return;
      }

      if (json.type === 'welcome') {
        this.subscribe();
        return;
      }

      // Handle order updates
      if (json.topic && json.topic.startsWith('/contractMarket/tradeOrders')) {
        this.recordMessageHandlerStats('order');
        this.handleOrderTopic(json);
        return;
      }

      // Handle algo order updates
      if (json.topic && json.topic.startsWith('/contractMarket/advancedOrders')) {
        this.recordMessageHandlerStats('orderAlgo');
        this.handleAlgoOrdersTopic(json);
        return;
      }

      if (
        json.topic?.startsWith('/contract/position') ||
        json.topic === '/contractAccount/wallet' ||
        json.topic === '/contract/marginMode' ||
        json.topic === '/contract/crossLeverage'
      ) {
        this.recordMessageHandlerStats('account');
        this.refreshSnapshotSoon();
      }
    }
  };

  private refreshSnapshotSoon = () => {
    if (this.snapshotRefreshTimer) clearTimeout(this.snapshotRefreshTimer);
    this.snapshotRefreshTimer = setTimeout(() => {
      this.snapshotRefreshTimer = undefined;
      if (!this.isDisposed) void this.parent.fetchAndSetPrivateSnapshot();
    }, 500);
  };

  reconnect = async () => {
    if (this.isDisposed) return;
    const previousSocket = this.ws;
    previousSocket?.removeEventListener?.('open', this.onOpen);
    previousSocket?.removeEventListener?.('message', this.onMessage);
    previousSocket?.removeEventListener?.('close', this.onClose);
    this.ws = undefined;
    previousSocket?.close?.();
    this.token = null;
    this.socketEndpoint = null;
    this.subscribed = false;
    await this.connectAndSubscribe();
  };

  dispose = () => {
    if (this.snapshotRefreshTimer) {
      clearTimeout(this.snapshotRefreshTimer);
      this.snapshotRefreshTimer = undefined;
    }
    super.dispose();
  };

  handleOrderTopic = (data: Record<string, any>) => {
    if (!data.data) return;

    const rawOrder = data.data;
    const orders = this.parent.mapOrder(rawOrder);
    const parentOrder = orders[0];

    if (!parentOrder) return;

    const eventType = rawOrder.type;
    const status = rawOrder.status;
    // Emit a fill ONLY on the incremental `matchSize` a fill frame carries.
    // `matchSize` is present only on match frames; the terminal `filled` frame
    // carries cumulative `filledSize` and no `matchSize`. The old
    // `matchSize || filledSize` (+ the `eventType === 'filled'` branch) double-
    // fired and over-counted on the match→filled pair. KuCoin reports fill size
    // in native contracts; emit in BASE units (see exchange.ts contractSizeFor).
    const fillAmountNative = parseFloat(rawOrder.matchSize || '0');
    const contractSize = this.parent.store.marketsBySymbol[parentOrder.symbol]?.contractSize || 1;

    if (fillAmountNative > 0) {
      this.emitter.emit('fill', {
        id: parentOrder.id,
        side: parentOrder.side,
        symbol: parentOrder.symbol,
        price: parseFloat(rawOrder.matchPrice || rawOrder.price || '0'),
        amount: fillAmountNative * contractSize,
      });
    }

    if (eventType === 'canceled' || eventType === 'cancel' || status === 'canceled' || status === 'cancelled') {
      orders.forEach((order) => this.store.updateOrderStatus(order, OrderStatus.Canceled));
      return;
    }

    if (eventType === 'filled' || (status === 'done' && parseFloat(rawOrder.remainSize || '0') === 0)) {
      orders.forEach((order) => this.store.updateOrderStatus(order, OrderStatus.Closed));
      return;
    }

    if (
      status === 'open' ||
      status === 'match' ||
      eventType === 'open' ||
      eventType === 'update' ||
      eventType === 'match'
    ) {
      orders.forEach((order) => this.store.addOrUpdateOrder(order));
    }
  };

  handleAlgoOrdersTopic = (data: Record<string, any>) => {
    if (!data.data) return;

    const rawOrder = data.data;
    const orders = this.parent.mapOrder(rawOrder);
    const parentOrder = orders[0];
    const eventType = rawOrder.type || rawOrder.status;

    if (!parentOrder) return;

    if (eventType === 'cancel') {
      this.store.updateOrderStatus(parentOrder, OrderStatus.Canceled);
    }

    if (eventType === 'triggered' || eventType === 'active' || eventType === 'open') {
      this.store.addOrUpdateOrder(parentOrder);
    }
  };

  private subscribe = () => {
    if (this.subscribed) return;
    this.subscribed = true;

    // Subscribe to private trade channels
    const topics = [
      `/contractMarket/tradeOrders`,
      `/contractMarket/advancedOrders`,
      `/contract/positionAll`,
      `/contractAccount/wallet`,
      `/contract/marginMode`,
      `/contract/crossLeverage`,
    ];

    for (const topic of topics) {
      const payload = {
        id: Date.now(),
        type: 'subscribe',
        topic,
        privateChannel: true,
        response: true,
      };

      this.sendWithLog(JSON.stringify(payload));
    }
  };
}
