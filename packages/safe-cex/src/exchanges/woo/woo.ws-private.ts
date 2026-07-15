import type { WOOXExchange } from './woo.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_PWS_LOG, ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { OrderStatus } from '../../types';
import { v } from '../../utils/get-key';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, ORDER_SIDE } from './woo.types';
import { normalizeSymbol } from './woo.utils';

export class WooPrivateWebscoket extends BaseWebSocket<WOOXExchange> {
  private parseTimestampToSeconds = (raw: unknown): number | undefined => {
    if (raw === undefined || raw === null || raw === '') {
      return undefined;
    }

    const parsed = Number(raw);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      return undefined;
    }

    return parsed >= 1_000_000_000_000 ? Math.floor(parsed / 1000) : Math.floor(parsed);
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const baseURL = BASE_WS_URL.private[this.parent.options.testnet ? 'testnet' : 'livenet'];

      this.ws = this.createWebsocket(`${baseURL}${this.parent.options.applicationId}`);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = async () => {
    if (!this.isDisposed) {
      const timestamp = virtualClock.getCurrentTimeMs();

      if (this.parent.options.remoteSigner) {
        const result = await this.parent.options.remoteSigner.sign({
          id: nanoid(),
          type: 'ws-auth',
          exchangeName: 'woo',
          accountName: this.parent.options.remoteSigner.accountName,
          wsAuth: {
            toSign: `|${timestamp}`,
            outputFormat: 'hex',
            extraData: { timestamp },
          },
        });

        if (result.error) {
          this.emitAndLogError(`Remote WS auth failed: ${result.error}`);
          return;
        }

        this.sendWithLog(
          JSON.stringify({
            event: 'auth',
            params: {
              apikey: this.parent.options.key,
              sign: result.wsAuthSignature,
              timestamp,
            },
          }),
        );

        if (ENABLE_RECONNECTING_WEBSOCKET) {
          this.parent.fetchAndSetPrivateSnapshot();
        }
        return;
      }

      const signature = createHmac('sha256', this.parent.options.secret).update(`|${timestamp}`).digest('hex');

      this.sendWithLog(
        JSON.stringify({
          event: 'auth',
          params: {
            apikey: this.parent.options.key,
            sign: signature,
            timestamp,
          },
        }),
      );

      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.parent.fetchAndSetPrivateSnapshot();
      }
    }
  };

  subscribe = () => {
    if (!this.isDisposed) {
      this.sendWithLog(JSON.stringify({ topic: 'executionreport', event: 'subscribe' }));
      this.sendWithLog(JSON.stringify({ topic: 'algoexecutionreportv2', event: 'subscribe' }));
      this.ping();
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ event: 'ping' }));
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }

      const json = jsonParse(data);

      if (json?.event === 'ping') {
        this.sendWithLog(JSON.stringify({ event: 'pong' }));
        return;
      }

      if (json?.event === 'pong') {
        this.handlePongEvent();
        return;
      }

      if (json?.event === 'auth') {
        if (json?.success) {
          this.subscribe();
        } else {
          this.emitAndLogError(
            json?.message || json?.error || 'WOO X private websocket authentication failed',
          );
        }
        return;
      }

      if (json?.topic === 'executionreport') {
        this.recordMessageHandlerStats('order');
        this.handleExecutionReport(json?.data);
        return;
      }

      if (json?.topic === 'algoexecutionreportv2') {
        this.recordMessageHandlerStats('orderAlgo');
        this.handleAlgoExecutionReport(json?.data);
      }
    }
  };

  handleExecutionReport = (data: Record<string, any>) => {
    const updatedOrder = this.parent.mapLimitOrder(data);
    const status = v(data, 'status');
    const isPartialFillStatus = status === 'PARTIAL_FILLED' || status === 'PARTIALLY_FILLED';

    const price = v(data, 'executedPrice');
    const amount = v(data, 'executedQuantity');
    const orderId = `${v(data, 'orderId') || v(data, 'order_id') || ''}`;
    const timestamp = this.parseTimestampToSeconds(
      v(data, 'executedTimestamp') ||
        v(data, 'tradeTime') ||
        v(data, 'updatedTime') ||
        v(data, 'updated_time') ||
        v(data, 'timestamp'),
    );

    if (status === 'REPLACED' || status === 'NEW' || isPartialFillStatus) {
      if (updatedOrder) {
        this.store.addOrUpdateOrder(updatedOrder);
      }
    }

    if ((status === 'CANCELLED' || status === 'FILLED') && orderId) {
      this.store.updateOrderStatus(
        { id: orderId },
        status === 'CANCELLED' ? OrderStatus.Canceled : OrderStatus.Closed,
      );
    }

    if (status === 'FILLED' || isPartialFillStatus) {
      const side = updatedOrder?.side || ORDER_SIDE[v(data, 'side')];
      const symbol = updatedOrder?.symbol || (v(data, 'symbol') ? normalizeSymbol(v(data, 'symbol')) : undefined);
      if (!side || !symbol) return;

      this.parent.emitter.emit('fill', {
        id: orderId || v(data, 'id') || v(data, 'orderId') || v(data, 'order_id'),
        orderId: orderId || undefined,
        side,
        symbol,
        price,
        amount,
        ...(timestamp ? { timestamp } : {}),
      });
    }
  };

  handleAlgoExecutionReport = (data: Array<Record<string, any>> | Record<string, any>) => {
    const rows = Array.isArray(data) ? data : data ? [data] : [];
    rows.forEach((row) => {
      const status = v(row, 'algoStatus');

      if (status === 'NEW' || status === 'REPLACED' || status === 'PARTIAL_FILLED' || status === 'PARTIALLY_FILLED') {
        const [updatedOrder] = this.parent.mapAlgoOrder(row);

        if (updatedOrder) {
          this.store.addOrUpdateOrder(updatedOrder);
        }
      }

      if (status === 'FILLED') {
        const orderId = `${v(row, 'orderId') || v(row, 'algoOrderId') || v(row, 'id') || ''}`;
        const timestamp = this.parseTimestampToSeconds(
          v(row, 'executedTimestamp') ||
            v(row, 'tradeTime') ||
            v(row, 'updatedTime') ||
            v(row, 'updated_time') ||
            v(row, 'timestamp'),
        );
        this.parent.emitter.emit('fill', {
          id: orderId || v(row, 'orderId') || v(row, 'algoOrderId') || v(row, 'id'),
          orderId: orderId || undefined,
          side: ORDER_SIDE[row.side],
          symbol: normalizeSymbol(row.symbol),
          price: row.averageExecutedPrice,
          amount: row.totalExecutedQuantity,
          ...(timestamp ? { timestamp } : {}),
        });
      }

      if (status === 'CANCELLED' || status === 'FILLED') {
        const orderId = `${v(row, 'algoOrderId') || v(row, 'orderId') || v(row, 'id') || ''}`;
        if (!orderId) return;

        this.store.updateOrderStatus(
          { id: orderId },
          status === 'CANCELLED' ? OrderStatus.Canceled : OrderStatus.Closed,
        );
      }
    });
  };
}
