import type { BitrueExchange } from './bitrue.exchange';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../../types';
import { cleanFloat, createIncrementalFills } from '../../../utils/incrementalFills';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ENDPOINTS, ORDER_SIDE, ORDER_STATUS, ORDER_TYPE, WS_ORDER_TRADE_UPDATE } from './bitrue.types';
import { reverseWsSymbol } from './bitrue.utils';

export class BitruePrivateWebsocket extends BaseWebSocket<BitrueExchange> {
  private readonly incrementalFills = createIncrementalFills();
  private readonly orderWatermarks = new Map<
    string,
    { filled: number; amount: number; terminal?: OrderStatus; time: number }
  >();
  private readonly cumulativeValues = new Map<string, number>();
  getWatchdogStaleThresholdMs = () => 20_000;

  private listenKeyRefreshTimeoutId: NodeJS.Timeout | undefined;
  private static readonly LISTEN_KEY_RETRY_DELAYS_MS = [250, 500, 1_000];

  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
      try {
        const listenKey = await this.fetchListenKey();
        if (this.isDisposed) return;
        this.ws = this.createWebsocket(`${BASE_WS_URL.private}?listenKey=${listenKey}`);

        this.ws.addEventListener('open', this.onOpen);
        this.ws.addEventListener('message', this.onMessage);
        this.ws.addEventListener('close', this.onClose);
      } catch (error) {
        this.retryFailedConnect(
          `Bitrue futures private WebSocket was not started: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.subscribe();
      this.ping();
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);
      let json: any;
      try {
        json = bigIntAsStringJsonParse(data);
      } catch (error) {
        console.error('Failed to parse Bitrue private ws message', data, error);
        return;
      }

      // special case
      if (json?.event === 'ping' || json?.ping !== undefined) {
        this.handlePingEvent(json);
        return;
      }

      const event = json?.e || json?.event;
      event && this.recordMessageHandlerStats(event);
      switch (event) {
        case 'BALANCE':
        case 'ACCOUNT_UPDATE':
          this.handleAccountUpdate(json);
          break;
        case 'ORDER':
        case 'ORDER_TRADE_UPDATE':
        case 'CANCEL':
          this.handleOrderTradeUpdate(json as WS_ORDER_TRADE_UPDATE);
          break;
        case 'user_account_update':
        case 'user_order_update':
        case 'user_balance_update':
          if (!(json?.status === 'ok' || this.isSuccessfulCode(json?.code) || json?.msg === 'succ')) {
            console.error(`Failed to subscribe to ${event}`, json);
          }
          break;
        default:
          console.error('Unknown event', json);
          break;
      }
    }
  };

  private parseOrderType = (rawType: unknown): OrderType => {
    const type = typeof rawType === 'string' ? rawType.toUpperCase() : `${rawType || ''}`;
    if (type === '1' || type === 'LIMIT' || type === 'IOC' || type === 'FOK' || type === 'POST_ONLY') {
      return OrderType.Limit;
    }
    if (type === '2' || type === 'MARKET') {
      return OrderType.Market;
    }
    if (type === '3' || type === 'STOP_MARKET') {
      return OrderType.StopLoss;
    }
    if (type === 'TAKE_PROFIT_MARKET') {
      return OrderType.TakeProfit;
    }
    return ORDER_TYPE[type as keyof typeof ORDER_TYPE] || OrderType.Limit;
  };

  private parseOrderSide = (rawSide: unknown): OrderSide => {
    if (typeof rawSide === 'number') {
      return rawSide === 2 ? OrderSide.Sell : OrderSide.Buy;
    }
    const side = `${rawSide || ''}`.toUpperCase();
    return ORDER_SIDE[side as keyof typeof ORDER_SIDE] || (side === '2' ? OrderSide.Sell : OrderSide.Buy);
  };

  private parseOrderStatus = (executionType: unknown, statusType: unknown): OrderStatus => {
    const normalizedStatus = `${statusType || ''}`.toUpperCase();
    const fromStatus = ORDER_STATUS[normalizedStatus];
    if (fromStatus) {
      return fromStatus;
    }

    const normalizedExec = `${executionType || ''}`.toUpperCase();
    if (normalizedExec === 'CANCEL') return OrderStatus.Canceled;
    if (normalizedExec === 'TRADE' && normalizedStatus === 'FILLED') return OrderStatus.Closed;
    if (normalizedExec === 'TRADE') return OrderStatus.Open;
    return OrderStatus.Open;
  };

  handleOrderTradeUpdate = (json: WS_ORDER_TRADE_UPDATE) => {
    // Futures payloads are nested under json.o, while some streams emit flat updates.
    const update: Record<string, any> = (json as any)?.o || (json as any);
    if (!update) return;

    const rawSymbol = `${update.s || ''}`.toUpperCase();
    if (!rawSymbol) return;
    const symbol = rawSymbol.includes('_') ? reverseWsSymbol(rawSymbol) : rawSymbol;

    const market = this.parent.store.markets.find((m) => m.symbol === symbol);
    if (!market) return;
    const multiplier = Number(market.contractSize || market.info?.multiplier || market.precision.amount || 1);

    let side = this.parseOrderSide(update.S);
    let type =
      update.conditionOrder && Number(update.triggerType) === 1
        ? OrderType.StopLoss
        : update.conditionOrder && Number(update.triggerType) === 2
          ? OrderType.TakeProfit
          : this.parseOrderType(update.o);
    const status = this.parseOrderStatus(update.x, update.X);

    const rawQty = Number(update.q ?? update.origQty ?? 0);
    const rawVolume = Number(update.volume ?? 0);
    const rawFilled = Number(update.z ?? update.executedQty ?? 0);
    if (![rawQty, rawVolume, rawFilled].every(Number.isFinite) || rawQty < 0 || rawVolume < 0 || rawFilled < 0) return;
    // q already includes the face value; z and l remain native contracts.
    const amount = rawQty > 0 ? rawQty : cleanFloat(rawVolume * multiplier);
    const filled = cleanFloat(rawFilled * multiplier);
    const orderId = update.i ?? update.orderId ?? update.c ?? update.clientOrderId;
    if (orderId === undefined || orderId === null) {
      console.error('Order update without id', update);
      return;
    }

    const id = String(orderId);
    const existing = this.store.orders.find((order) => order.id === id);
    if (existing && [OrderType.StopLoss, OrderType.TakeProfit].includes(existing.type)) {
      type = existing.type;
      if (existing.parentId) side = existing.side;
    }
    const watermark = this.orderWatermarks.get(id);
    const previous = Math.max(watermark?.filled || 0, existing?.filled || 0);
    if (existing && existing.filled > (watermark?.filled || 0)) {
      if (Number(existing.info?.avgPrice) > 0)
        this.cumulativeValues.set(id, (existing.filled / multiplier) * Number(existing.info.avgPrice));
      else this.cumulativeValues.delete(id);
    }
    const previousTerminal =
      watermark?.terminal ||
      (existing && [OrderStatus.Closed, OrderStatus.Canceled].includes(existing.status) ? existing.status : undefined);
    const terminal = [OrderStatus.Closed, OrderStatus.Canceled].includes(status);
    const time = Number((json as any).E ?? update.E ?? Date.now());
    const total = Math.max(amount, existing?.amount || 0, watermark?.amount || 0, filled);
    const cumulativeValue =
      Number(update.ap) > 0
        ? rawFilled * Number(update.ap)
        : Number(update.L) > 0 && filled > previous && (previous === 0 || this.cumulativeValues.has(id))
          ? (this.cumulativeValues.get(id) || 0) + (rawFilled - previous / multiplier) * Number(update.L)
          : undefined;
    if (`${update.x || ''}`.toUpperCase() === 'TRADE') {
      // A REST snapshot may already include fills that a delayed stream repeats.
      this.incrementalFills(id, previous / multiplier, this.cumulativeValues.get(id));
      const execution = this.incrementalFills(id, rawFilled, cumulativeValue);
      const price =
        Number(update.L) > 0
          ? Number(update.L)
          : (previous === 0 || this.cumulativeValues.has(id)) && cumulativeValue !== undefined && execution.qty > 0
            ? (cumulativeValue - (this.cumulativeValues.get(id) || 0)) / execution.qty
            : 0;
      if (execution.qty > 0 && price > 0)
        this.emitter.emit('fill', {
          id,
          side,
          symbol,
          price: cleanFloat(price),
          amount: cleanFloat(execution.qty * multiplier),
        });
    }
    if (filled >= previous && cumulativeValue !== undefined) this.cumulativeValues.set(id, cumulativeValue);
    this.orderWatermarks.delete(id);
    this.orderWatermarks.set(id, {
      filled: Math.max(previous, filled),
      amount: total,
      terminal: previousTerminal === OrderStatus.Closed ? previousTerminal : terminal ? status : previousTerminal,
      time: Math.max(time, watermark?.time || 0),
    });
    while (this.orderWatermarks.size > 500) {
      const oldest = this.orderWatermarks.keys().next().value!;
      this.orderWatermarks.delete(oldest);
      this.cumulativeValues.delete(oldest);
    }
    if (
      !terminal &&
      (previousTerminal || filled < previous || time < Math.max(existing?.lastUpdate || 0, watermark?.time || 0))
    )
      return;
    if (previousTerminal === OrderStatus.Closed && status === OrderStatus.Canceled) return;
    const conditional = type === OrderType.StopLoss || type === OrderType.TakeProfit;
    const price = conditional
      ? Number(update.triggerPrice) || existing?.price || Number(update.p) || 0
      : Number(update.p) || Number(update.ap) || 0;
    const order: Order = {
      id,
      status,
      symbol,
      type,
      side,
      price,
      amount: total,
      filled: Math.max(previous, filled),
      remaining: Math.max(total - Math.max(previous, filled), 0),
      timeInForce:
        update.o === 'IOC'
          ? OrderTimeInForce.ImmediateOrCancel
          : update.o === 'FOK'
            ? OrderTimeInForce.FillOrKill
            : update.o === 'POST_ONLY'
              ? OrderTimeInForce.PostOnly
              : (existing?.timeInForce ?? OrderTimeInForce.GoodTillCancel),
      reduceOnly:
        Boolean(existing?.parentId) ||
        Boolean(update.R ?? (update.open === undefined ? existing?.reduceOnly : update.open === 'CLOSE')),
      ...(existing?.parentId && { parentId: existing.parentId }),
      ...(existing?.clientOrderId && { clientOrderId: existing.clientOrderId }),
      ...(existing?.tpTriggerType && { tpTriggerType: existing.tpTriggerType }),
      ...(existing?.slTriggerType && { slTriggerType: existing.slTriggerType }),
      ...(existing?.takeProfit !== undefined && { takeProfit: existing.takeProfit }),
      ...(existing?.stopLoss !== undefined && { stopLoss: existing.stopLoss }),
      lastUpdate: time,
      info: update,
    };

    this.store.addOrUpdateOrder(order);
  };

  handleAccountUpdate = (_json: any) => {
    // {
    //   "E": 1723870402183,
    //   "T": 0,
    //   "a": {
    //   "B": [
    //     {
    //       "a": "USDT",
    //       "bc": "-2.3511",
    //       "cw": "250.1373"
    //     }
    //   ],
    //     "m": "NEW"
    // },
    //   "e": "ACCOUNT_UPDATE"
    // }
    this.parent.queueFastPrivateRefresh();
  };

  // reverse uno ping pong
  handlePingEvent = (json?: Record<string, any>) => {
    this.pingAt = performance.now();
    const ping = json?.ping;
    if (ping !== undefined) {
      this.send(JSON.stringify({ pong: ping }));
    } else {
      this.send(JSON.stringify({ event: 'pong', ...(json?.ts === undefined ? {} : { ts: json.ts }) }));
    }
    this.handlePongEvent(20_000);
  };

  // noop bc reversed
  ping = () => {};

  private isSuccessfulCode = (code: unknown) => {
    return code === 0 || code === '0' || code === 200 || code === '200';
  };

  private scheduleListenKeyRefresh = (listenKey: string) => {
    if (this.listenKeyRefreshTimeoutId) {
      clearTimeout(this.listenKeyRefreshTimeoutId);
    }
    this.listenKeyRefreshTimeoutId = setTimeout(() => this.updateListenKey(listenKey), 30 * 60 * 1000);
  };

  private fetchListenKey = async () => {
    let lastError: unknown;
    for (let attempt = 0; attempt <= BitruePrivateWebsocket.LISTEN_KEY_RETRY_DELAYS_MS.length; attempt++) {
      try {
        const { data } = await this.parent.xhr.post(ENDPOINTS.LISTEN_KEY);
        const listenKey = data?.data?.listenKey || data?.listenKey;
        const isOk = this.isSuccessfulCode(data?.code) || data?.status === 'ok' || data?.msg === 'succ';
        if (listenKey && (isOk || data?.code === undefined)) {
          this.scheduleListenKeyRefresh(listenKey);
          return listenKey;
        }
        lastError = new Error(`Bitrue futures listen-key response was invalid (code: ${String(data?.code)})`);
      } catch (error) {
        lastError = error;
      }

      const delay = BitruePrivateWebsocket.LISTEN_KEY_RETRY_DELAYS_MS[attempt];
      if (delay !== undefined) {
        this.parent.log(
          `Bitrue futures listen-key attempt ${attempt + 1} failed; retrying in ${delay}ms.`,
          LogSeverity.Warning,
        );
        await new Promise<void>((resolve) => setTimeout(resolve, delay));
      }
    }

    throw new Error(
      `Failed to acquire Bitrue futures listen key after ${BitruePrivateWebsocket.LISTEN_KEY_RETRY_DELAYS_MS.length + 1} attempts: ${
        lastError instanceof Error ? lastError.message : String(lastError || 'unknown error')
      }`,
    );
  };

  private updateListenKey = async (listenKey: string) => {
    if (!this.isDisposed) {
      try {
        const { data } = await this.parent.xhr.put(`${ENDPOINTS.LISTEN_KEY}/${listenKey}`);
        const isOk = this.isSuccessfulCode(data?.code) || data?.status === 'ok' || data?.msg === 'succ';
        if (isOk) {
          this.scheduleListenKeyRefresh(listenKey);
          return;
        }
      } catch (_error) {
        // The documented futures endpoint is temporarily unavailable. Do not
        // substitute a spot listen key for an active futures account.
      }

      this.emitAndLogError('Failed to refresh Bitrue futures listen key; private updates may stop when it expires.');
    }
  };

  // private auth = () => {
  //   const expires = new Date().getTime() + 10_000;
  //   const signature = createHmac('sha256', this.parent.options.secret).update(`GET/realtime${expires}`).digest('hex');
  //
  //   const payload = {
  //     op: 'auth',
  //     args: [this.parent.options.key, expires.toFixed(0), signature],
  //   };
  //
  //   this.sendWithLog(JSON.stringify(payload));
  // };

  private subscribe = () => {
    const channels = ['user_account_update', 'user_order_update', 'user_balance_update'];
    channels.forEach((channel) => {
      const payload = { event: 'sub', params: { channel } };
      this.sendWithLog(JSON.stringify(payload));
    });
  };

  dispose() {
    if (this.listenKeyRefreshTimeoutId) {
      clearTimeout(this.listenKeyRefreshTimeoutId);
      this.listenKeyRefreshTimeoutId = undefined;
    }
    super.dispose();
  }
}
