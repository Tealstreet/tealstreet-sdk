import type { MexcExchange } from './mexc.exchange';

import { nanoid } from 'nanoid';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity, Order, OrderStatus } from '../../types';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { signMexcWsLogin } from './mexc.api';
import { BASE_WS_URL } from './mexc.types';
import { fromMexcSymbol, normalizeExecution, normalizeOrder, normalizePosition } from './mexc.utils';

export class MexcPrivateWebsocket extends BaseWebSocket<MexcExchange> {
  protected override treatAnyMessageAsPong = true;

  private refreshOrdersTimeout?: NodeJS.Timeout;
  private refreshPositionsTimeout?: NodeJS.Timeout;
  private refreshBalanceTimeout?: NodeJS.Timeout;

  connectAndSubscribe = () => {
    if (this.isDisposed || this.parent.isPublicDataOnly) return;
    if (!this.parent.options.key) return;

    const key = this.parent.options.testnet ? 'testnet' : 'livenet';
    this.ws = this.createWebsocket(BASE_WS_URL.private[key], undefined, { headers: { 'User-Agent': 'Tealstreet' } });
    this.ws.addEventListener('open', this.onOpen);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.onClose);
  };

  onOpen = () => {
    if (this.isDisposed) return;
    this.sendLogin();
    this.parent.fetchAndSetPrivateSnapshot(10);
    this.emitWsReconnected();
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ method: 'ping' }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();

    const json = bigIntAsStringJsonParse<Record<string, any>>(String(data));
    if (!json) return;

    if (json.channel === 'pong' || json.msg === 'pong' || json.method === 'pong') {
      this.handlePongEvent(15_000);
      return;
    }

    if (json.channel === 'rs.login') {
      if (json.data === 'success' || json.success === true) {
        this.subscribePrivateFeeds();
        this.parent.emitter.emit('grafana-pws', '[mexc login] success');
      } else {
        this.parent.emitter.emit('grafana-pws', `[mexc login] failed ${JSON.stringify(json)}`);
        this.parent.emitAndLogError('MEXC private ws login failed, double check your key and secret');
      }
      return;
    }

    if (json.success === false) {
      this.parent.log(`MEXC private WS request failed: ${json.message || json.msg || 'unknown error'}`, LogSeverity.Warning);
      return;
    }

    const channel = String(json.channel || '');
    if (!channel) return;
    this.recordMessageHandlerStats(channel);

    switch (channel) {
      case 'push.personal.order':
        this.handleOrder(json);
        break;
      case 'push.personal.order.deal':
        this.handleFill(json);
        break;
      case 'push.personal.position':
        this.handlePosition(json);
        break;
      case 'push.personal.asset':
        this.throttledFetchBalance();
        break;
    }
  };

  dispose = () => {
    clearTimeout(this.refreshOrdersTimeout);
    clearTimeout(this.refreshPositionsTimeout);
    clearTimeout(this.refreshBalanceTimeout);
    super.dispose();
  };

  private sendLogin = async () => {
    const apiKey = this.parent.options.key.trim();
    const reqTime = `${virtualClock.getCurrentTimeMs()}`;
    const signature = await this.createLoginSignature(apiKey, reqTime);
    if (!signature) return;

    this.sendWithLog(
      JSON.stringify({
        method: 'login',
        param: {
          apiKey,
          reqTime,
          signature,
        },
      }),
    );
  };

  private createLoginSignature = async (apiKey: string, reqTime: string): Promise<string | undefined> => {
    const remoteSigner = this.parent.options.remoteSigner;
    if (!remoteSigner) {
      return signMexcWsLogin(this.parent.options.secret || '', apiKey, reqTime);
    }

    const result = await remoteSigner.sign({
      id: nanoid(),
      type: 'ws-auth',
      exchangeName: 'mexc',
      accountName: remoteSigner.accountName,
      wsAuth: {
        toSign: `${apiKey}${reqTime}`,
        outputFormat: 'hex',
        extraData: { reqTime },
      },
    });

    if (result.error || !result.wsAuthSignature) {
      this.parent.emitAndLogError(`MEXC remote WS auth failed: ${result.error ?? 'no signature returned'}`);
      return undefined;
    }
    return result.wsAuthSignature;
  };

  private subscribePrivateFeeds = () => {
    this.sendWithLog(
      JSON.stringify({
        method: 'personal.filter',
        param: {
          filters: [
            { filter: 'order' },
            { filter: 'order.deal' },
            { filter: 'position' },
            { filter: 'asset' },
          ],
        },
      }),
    );
  };

  private handleOrder = (json: Record<string, any>) => {
    const data = json.data || json;
    const rows = Array.isArray(data) ? data : [data];
    const orders = rows
      .map((row) => normalizeOrder(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
      .filter((order): order is Order => !!order);

    if (orders.length) {
      this.store.addOrUpdateOrders(orders as any);
      for (const order of orders) {
        if (order.status === OrderStatus.Closed || order.status === OrderStatus.Canceled) {
          this.store.updateOrderStatus({ id: order.id }, order.status);
        }
      }
    }
    this.throttledFetchOrders();
  };

  private handleFill = (json: Record<string, any>) => {
    const data = json.data || json;
    const rows = Array.isArray(data) ? data : [data];
    for (const row of rows) {
      const fill = normalizeExecution(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]);
      if (!fill) continue;
      this.parent.emitter.emit('fill', {
        id: fill.id || fill.orderId || `${fill.symbol}:${fill.timestamp}`,
        orderId: fill.orderId,
        symbol: fill.symbol,
        side: fill.side,
        price: fill.price,
        amount: fill.qty,
        fee: fill.commission,
        realizedPnl: fill.realizedPnl,
      });
    }
    this.throttledFetchOrders();
    this.throttledFetchPositions();
  };

  private handlePosition = (json: Record<string, any>) => {
    const data = json.data || json;
    const rows = Array.isArray(data) ? data : [data];
    const positions = rows
      .map((row) => normalizePosition(row, this.store.marketsBySymbol[fromMexcSymbol(row.symbol)]))
      .filter(Boolean);
    if (positions.length) {
      this.store.addOrUpdatePositions(positions as any);
    }
    this.throttledFetchPositions();
  };

  private throttledFetchOrders = () => {
    clearTimeout(this.refreshOrdersTimeout);
    this.refreshOrdersTimeout = setTimeout(async () => {
      try {
        const orders = await this.parent.fetchOrders();
        if (!this.isDisposed) this.store.update({ orders });
      } catch (_err) {}
    }, 1000);
  };

  private throttledFetchPositions = () => {
    clearTimeout(this.refreshPositionsTimeout);
    this.refreshPositionsTimeout = setTimeout(async () => {
      try {
        const positions = await this.parent.fetchPositions();
        if (!this.isDisposed) this.store.addOrUpdatePositionsFull(positions);
      } catch (_err) {}
    }, 1000);
  };

  private throttledFetchBalance = () => {
    clearTimeout(this.refreshBalanceTimeout);
    this.refreshBalanceTimeout = setTimeout(async () => {
      try {
        const balance = await this.parent.fetchBalance();
        if (!this.isDisposed) this.store.update({ balance });
      } catch (_err) {}
    }, 1000);
  };
}
