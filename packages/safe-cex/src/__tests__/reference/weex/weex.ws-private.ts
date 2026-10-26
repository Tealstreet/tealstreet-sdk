import type { WeexExchange } from './weex.exchange';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity, Order, OrderStatus } from '../../../types';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket, HeaderCapableWebSocketUnavailableError } from '../../../exchanges/base.ws';
import { signWeexMessage } from './weex.api';
import { BASE_WS_URL, PRIVATE_WS_REQUEST_PATH } from './weex.types';
import { normalizeOrder, normalizePosition, normalizeTimestampToSeconds, toNumber } from './weex.utils';

const isNodeOrBunRuntime = (): boolean =>
  typeof process !== 'undefined' && !!(process.versions?.node || (process as any).isBun);

export class WeexPrivateWebsocket extends BaseWebSocket<WeexExchange> {
  protected override treatAnyMessageAsPong = true;

  private subscriptionId = 1;
  private hasLoggedUnsupportedTransport = false;
  private refreshOrdersTimeout?: NodeJS.Timeout;
  private refreshPositionsTimeout?: NodeJS.Timeout;
  private refreshBalanceTimeout?: NodeJS.Timeout;

  connectAndSubscribe = () => {
    if (this.isDisposed) return;

    // WEEX V3 authenticates private channels only in the HTTP upgrade headers;
    // browsers cannot set those headers and the exchange offers no login frame.
    // Do not open an unauthenticated socket or put credentials in the URL.
    if (!isNodeOrBunRuntime()) {
      this.logUnsupportedTransport();
      return;
    }

    if (!this.parent.options.key || !this.parent.options.secret || !this.parent.options.passphrase) {
      this.logUnsupportedTransport('credentials are incomplete');
      return;
    }

    const key = this.parent.options.testnet ? 'testnet' : 'livenet';
    try {
      this.ws = this.createWebsocket(BASE_WS_URL.private[key], undefined, {
        // ReconnectingWebSocket evaluates this provider immediately before every
        // Node/Bun connection attempt, keeping WEEX's 30-second auth timestamp fresh.
        headers: () => this.createAuthHeaders(),
        headersRequired: true,
      });
    } catch (error) {
      if (error instanceof HeaderCapableWebSocketUnavailableError) {
        this.logUnsupportedTransport('a header-capable Node/Bun WebSocket transport is unavailable');
        return;
      }
      throw error;
    }
    this.ws.addEventListener('open', this.onOpen);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.onClose);
  };

  onOpen = () => {
    if (this.isDisposed) return;

    this.subscribe();
    this.parent.fetchAndSetPrivateSnapshot(10);
    this.emitWsReconnected();
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ method: 'PING', id: this.subscriptionId++ }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();

    const json = bigIntAsStringJsonParse<Record<string, any>>(String(data));
    if (!json) return;

    if (json.type === 'ping') {
      this.send(JSON.stringify({ method: 'PONG', id: this.subscriptionId++ }));
      this.handlePongEvent(15_000);
      return;
    }

    if (json.result !== undefined) {
      if (json.result === false) {
        this.parent.log(`WEEX private WS subscription failed: ${json.msg || 'unknown error'}`, LogSeverity.Warning);
      }
      return;
    }

    const event = json.e;
    if (!event) return;
    this.recordMessageHandlerStats(event);

    switch (event) {
      case 'account':
        this.throttledFetchBalance();
        break;
      case 'positions':
        this.handlePositions(json);
        break;
      case 'orders':
        this.handleOrders(json);
        break;
      case 'fill':
        this.handleFills(json);
        break;
    }
  };

  dispose = () => {
    clearTimeout(this.refreshOrdersTimeout);
    clearTimeout(this.refreshPositionsTimeout);
    clearTimeout(this.refreshBalanceTimeout);
    super.dispose();
  };

  private subscribe = () => {
    this.sendWithLog(
      JSON.stringify({
        method: 'SUBSCRIBE',
        params: ['account', 'positions', 'orders', 'fill'],
        id: this.subscriptionId++,
      }),
    );
  };

  private createAuthHeaders = (): Record<string, string> => {
    const timestamp = `${virtualClock.getCurrentTimeMs()}`;
    return {
      'User-Agent': 'Tealstreet',
      'ACCESS-KEY': this.parent.options.key!,
      'ACCESS-PASSPHRASE': this.parent.options.passphrase!,
      'ACCESS-TIMESTAMP': timestamp,
      'ACCESS-SIGN': signWeexMessage(this.parent.options.secret!, `${timestamp}${PRIVATE_WS_REQUEST_PATH}`),
    };
  };

  private logUnsupportedTransport = (reason = 'browser WebSocket clients cannot attach WEEX private-auth headers') => {
    if (this.hasLoggedUnsupportedTransport) return;
    this.hasLoggedUnsupportedTransport = true;
    this.parent.log(`WEEX private WebSocket disabled: ${reason}; continuing with the REST safety loop`, LogSeverity.Warning);
  };

  private handleOrders = (json: Record<string, any>) => {
    const orders = (Array.isArray(json.d) ? json.d : [])
      .map((row) => normalizeOrder(row, this.store.marketsBySymbol[row.symbol]))
      .filter((order): order is Order => !!order);

    if (orders.length) {
      this.store.addOrUpdateOrders(orders as any);
      const closed = orders.filter(
        (order: any) => order.status === OrderStatus.Closed || order.status === OrderStatus.Canceled,
      );
      for (const order of closed) {
        this.store.updateOrderStatus({ id: order.id }, order.status);
      }
    }

    this.throttledFetchOrders();
  };

  private handlePositions = (json: Record<string, any>) => {
    const positions = (Array.isArray(json.d) ? json.d : [])
      .map((row) => normalizePosition(row, this.store.marketsBySymbol[row.symbol]))
      .filter(Boolean);
    if (positions.length) {
      this.store.addOrUpdatePositions(positions as any);
    }
    this.throttledFetchPositions();
  };

  private handleFills = (json: Record<string, any>) => {
    const fills = Array.isArray(json.d) ? json.d : [];

    for (const fill of fills) {
      const price =
        toNumber(fill.fillValue) && toNumber(fill.fillSize) ? toNumber(fill.fillValue) / toNumber(fill.fillSize) : 0;
      const amount = Math.abs(toNumber(fill.fillSize));
      const order = this.store.orders.find((item) => item.id === String(fill.orderId));
      const side = order?.side || (String(fill.orderSide).toUpperCase() === 'SELL' ? 'sell' : 'buy');

      if (amount <= 0) continue;

      this.parent.emitter.emit('fill', {
        id: String(fill.id || fill.orderId),
        orderId: String(fill.orderId || ''),
        side,
        symbol: fill.symbol,
        price,
        amount,
        timestamp: normalizeTimestampToSeconds(fill.createdTime || json.E),
        fee: Math.abs(toNumber(fill.fillFee)),
        realizedPnl: toNumber(fill.realizePnl),
      });
    }

    this.throttledFetchOrders();
  };

  private throttledFetchOrders = () => {
    clearTimeout(this.refreshOrdersTimeout);
    this.refreshOrdersTimeout = setTimeout(async () => {
      try {
        const orders = await this.parent.fetchOrders();
        if (!this.isDisposed) {
          this.store.update({ orders });
        }
      } catch (_err) {}
    }, 250);
  };

  private throttledFetchPositions = () => {
    clearTimeout(this.refreshPositionsTimeout);
    this.refreshPositionsTimeout = setTimeout(async () => {
      try {
        const positions = await this.parent.fetchPositions();
        if (!this.isDisposed) {
          this.store.addOrUpdatePositionsFull(positions);
        }
      } catch (_err) {}
    }, 250);
  };

  private throttledFetchBalance = () => {
    clearTimeout(this.refreshBalanceTimeout);
    this.refreshBalanceTimeout = setTimeout(async () => {
      try {
        const balance = await this.parent.fetchBalance();
        if (!this.isDisposed) {
          this.store.update({ balance });
        }
      } catch (_err) {}
    }, 250);
  };
}
