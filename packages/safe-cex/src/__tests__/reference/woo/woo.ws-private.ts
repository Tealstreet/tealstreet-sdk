import type { WOOXExchange } from './woo.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../../config/config';
import { OrderStatus } from '../../../types';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ORDER_SIDE } from './woo.types';
import { normalizeSymbol } from './woo.utils';

export class WooPrivateWebscoket extends BaseWebSocket<WOOXExchange> {
  private executionIds = new Map<string, number>();
  private cumulativeFills = new Map<string, { amount: number; at: number }>();

  private executionRow = (row: Record<string, any>, orderId: string, previous: number) => {
    const tradeId = row.tradeId ?? row.trade_id;
    const status = row.status ?? row.algoStatus;
    if (
      row.totalExecutedQuantity === undefined &&
      row.executed === undefined &&
      tradeId !== undefined &&
      !this.executionIds.has(`${orderId}:${tradeId}`) &&
      ['FILLED', 'PARTIAL_FILLED', 'PARTIALLY_FILLED'].includes(status)
    ) {
      const amount = Number(row.executedQuantity ?? row.executed_quantity);
      if (amount > 0 && Number.isFinite(amount)) return { ...row, totalExecutedQuantity: previous + amount };
    }
    return row;
  };

  private emitExecution = (row: Record<string, any>, orderId: string, previous: number) => {
    const now = virtualClock.getCurrentTimeMs();
    for (const [id, value] of this.cumulativeFills) if (now - value.at > 300000) this.cumulativeFills.delete(id);
    const cumulative = Number(row.totalExecutedQuantity ?? row.executed ?? 0);
    const known = Math.max(previous, this.cumulativeFills.get(orderId)?.amount || 0);
    this.cumulativeFills.set(orderId, { amount: Math.max(cumulative, known), at: now });
    const tradeId = row.tradeId ?? row.trade_id;
    for (const [id, at] of this.executionIds) if (now - at > 300000) this.executionIds.delete(id);
    const key = tradeId === undefined || tradeId === null ? undefined : `${orderId}:${tradeId}`;
    if (key && this.executionIds.has(key)) return;
    const amount = key
      ? Number(row.executedQuantity ?? row.executed_quantity ?? Math.max(0, cumulative - known))
      : Math.max(0, cumulative - known);
    const price = Number(
      row.executedPrice ??
        row.executed_price ??
        row.averageExecutedPrice ??
        row.average_executed_price ??
        this.store.orders.find((o) => o.id === orderId)?.price,
    );
    const side = ORDER_SIDE[row.side];
    const symbol = row.symbol ? normalizeSymbol(row.symbol) : this.store.orders.find((o) => o.id === orderId)?.symbol;
    if (!(amount > 0) || !(price > 0) || !Number.isFinite(amount) || !Number.isFinite(price) || !side || !symbol)
      return;
    if (key) this.executionIds.set(key, now);
    const timestamp = this.parseTimestampToSeconds(
      row.executedTimestamp ?? row.tradeTime ?? row.updatedTime ?? row.updated_time ?? row.timestamp,
    );
    this.parent.emitOrderExecution(
      {
        id: tradeId === undefined || tradeId === null ? orderId : String(tradeId),
        orderId,
        side,
        symbol,
        price,
        amount,
        ...(timestamp ? { timestamp } : {}),
      },
      Math.min(amount, Math.max(0, cumulative - known)),
    );
  };

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
          // NOT "auth failed": `health/errorPatterns.ts` matches that literal under
          // `auth_invalid`, which red-toasts "API key rejected" and makes
          // `noteTerminalExchangeFailure` suppress automatic account reloads for five
          // minutes — the very reload that would re-pair the browser signer. A
          // backgrounded tab or a 15s signer timeout must not look like a bad key.
          // Mirrors the bybit fix in `bybit/bybit.ws-private.ts` → auth.
          this.emitAndLogError(`Remote WS signing unavailable: ${result.error}`);
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
      this.emitWsDiagnosticReceive(data);

      const json = bigIntAsStringJsonParse(data);

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
          this.emitAndLogError(json?.message || json?.error || 'WOO X private websocket authentication failed');
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
    if (!data || typeof data !== 'object') return;
    const orderId = String(data.orderId ?? data.order_id ?? data.id ?? '');
    if (!orderId) return;
    const previous = this.store.orders.find((o) => o.id === orderId)?.filled || 0;
    const normalized = this.executionRow(data, orderId, previous);
    const order = this.parent.mapLimitOrder(normalized);
    if (order) this.parent.acceptOrder(order);
    if (['FILLED', 'PARTIAL_FILLED', 'PARTIALLY_FILLED'].includes(data.status))
      this.emitExecution(normalized, orderId, previous);
  };

  handleAlgoExecutionReport = (data: Array<Record<string, any>> | Record<string, any>) => {
    for (const raw of Array.isArray(data) ? data : data ? [data] : []) {
      const rootId = String(raw.algoOrderId ?? raw.orderId ?? raw.id ?? '');
      if (!Array.isArray(raw.childOrders) && ['CANCELLED', 'CANCELED', 'REJECTED', 'FILLED'].includes(raw.algoStatus)) {
        // WOO can acknowledge a parent cancellation without resending its leaves.
        for (const order of this.store.orders.filter((order) => order.parentId === rootId && order.id !== rootId)) {
          this.parent.acceptOrder({
            ...order,
            status: raw.algoStatus === 'FILLED' ? OrderStatus.Closed : OrderStatus.Canceled,
          });
        }
      }
      const rows = Array.isArray(raw.childOrders)
        ? raw.childOrders.map((child: Record<string, any>) => ({
            ...child,
            symbol: child.symbol ?? raw.symbol,
            side: child.side ?? raw.side,
            reduceOnly: child.reduceOnly ?? raw.reduceOnly,
            rootAlgoOrderId: child.rootAlgoOrderId ?? raw.rootAlgoOrderId ?? rootId,
            algoStatus: child.algoStatus ?? raw.algoStatus,
          }))
        : [raw];
      for (const leaf of rows) {
        const id = String(leaf.algoOrderId ?? leaf.orderId ?? leaf.id ?? '');
        const previous = this.store.orders.find((o) => o.id === id)?.filled || 0;
        const row = this.executionRow(leaf, id, previous);
        const mapped = this.parent.mapAlgoOrder(row);
        for (const order of mapped) {
          this.parent.acceptOrder(order);
          if (['FILLED', 'PARTIAL_FILLED', 'PARTIALLY_FILLED'].includes(row.algoStatus)) {
            this.emitExecution(
              { ...row, side: row.side || order.info?.side, symbol: row.symbol || order.symbol },
              order.id,
              previous,
            );
          }
        }
      }
    }
  };
}
