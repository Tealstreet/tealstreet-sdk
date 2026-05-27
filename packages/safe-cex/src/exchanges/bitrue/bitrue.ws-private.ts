import type { BitrueExchange } from './bitrue.exchange';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_PWS_LOG } from '../../config/config';
import { Order, OrderSide, OrderStatus, OrderType } from '../../types';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_URL, ENDPOINTS, ORDER_SIDE, ORDER_STATUS, ORDER_TYPE, WS_ORDER_TRADE_UPDATE } from './bitrue.types';
import { reverseWsSymbol } from './bitrue.utils';

export class BitruePrivateWebsocket extends BaseWebSocket<BitrueExchange> {
  private listenKeyRefreshTimeoutId: NodeJS.Timeout | undefined;
  private listenKeyEndpoint = ENDPOINTS.LISTEN_KEY;

  connectAndSubscribe = async () => {
    if (!this.isDisposed) {
      const { listenKey, endpoint, wsBaseUrl } = await this.fetchListenKey();
      this.listenKeyEndpoint = endpoint;
      const url = `${wsBaseUrl}?listenKey=${listenKey}`;

      this.ws = this.createWebsocket(url);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
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
      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }
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
    const multiplier = Number(market?.info?.multiplier || 1);

    const side = this.parseOrderSide(update.S);
    const type = this.parseOrderType(update.o);
    const status = this.parseOrderStatus(update.x, update.X);

    const rawQty = Number(update.q ?? update.origQty ?? 0);
    const rawVolume = Number(update.volume ?? 0);
    const rawFilled = Number(update.z ?? update.executedQty ?? 0);
    const rawRemaining = Number(update.l ?? 0);

    const amountFromVolume = rawVolume > 0 ? rawVolume * multiplier : 0;
    const amount = rawQty > 0 ? rawQty : amountFromVolume;
    const scaledFilled = rawFilled * multiplier;
    const filled = amount > 0 && rawFilled <= amount ? rawFilled : scaledFilled;
    const remaining = rawRemaining > 0 ? rawRemaining * multiplier : Math.max(amount - filled, 0);
    const price = Number(update.p ?? update.ap ?? update.L ?? 0);

    const orderId = update.i ?? update.orderId ?? update.c ?? update.clientOrderId;
    if (orderId === undefined || orderId === null) {
      console.error('Order update without id', update);
      return;
    }

    if (`${update.x || ''}`.toUpperCase() === 'TRADE') {
      const fillAmountRaw = Number(update.z ?? 0);
      const fillAmount = amount > 0 && fillAmountRaw <= amount ? fillAmountRaw : fillAmountRaw * multiplier;
      if (fillAmount > 0) {
        this.emitter.emit('fill', {
          id: String(orderId),
          side,
          symbol,
          price,
          amount: fillAmount,
        });
      }
    }

    const order: Order = {
      id: String(orderId),
      status,
      symbol,
      type,
      side,
      price,
      amount,
      filled,
      remaining,
      reduceOnly: Boolean(update.R ?? update.open === 'CLOSE'),
      lastUpdate: Number((json as any).E ?? update.E ?? Date.now()),
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
      this.send(JSON.stringify({ event: 'pong' }));
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
    const candidates = [
      { endpoint: ENDPOINTS.LISTEN_KEY, wsBaseUrl: BASE_WS_URL.private },
      { endpoint: ENDPOINTS.LISTEN_KEY_OPEN, wsBaseUrl: BASE_WS_URL.privateFallback },
    ];

    let lastError: unknown;
    for (const candidate of candidates) {
      try {
        const { data } = await this.parent.xhr.post(candidate.endpoint);
        const listenKey = data?.data?.listenKey || data?.listenKey;
        const isOk = this.isSuccessfulCode(data?.code) || data?.status === 'ok' || data?.msg === 'succ';
        if (listenKey && (isOk || data?.code === undefined)) {
          this.scheduleListenKeyRefresh(listenKey);
          return { listenKey, endpoint: candidate.endpoint, wsBaseUrl: candidate.wsBaseUrl };
        }
      } catch (error) {
        lastError = error;
      }
    }

    this.emitAndLogError(lastError instanceof Error ? lastError : String(lastError || 'Failed to fetch listen key'));
    throw new Error('Failed to fetch listen key');
  };

  private updateListenKey = async (listenKey: string) => {
    if (!this.isDisposed) {
      const endpoints = [this.listenKeyEndpoint, ENDPOINTS.LISTEN_KEY, ENDPOINTS.LISTEN_KEY_OPEN].filter(Boolean);
      for (const endpoint of endpoints) {
        try {
          const { data } = await this.parent.xhr.put(`${endpoint}/${listenKey}`);
          const isOk = this.isSuccessfulCode(data?.code) || data?.status === 'ok' || data?.msg === 'succ';
          if (isOk) {
            this.scheduleListenKeyRefresh(listenKey);
            return;
          }
        } catch (_error) {
          // try next endpoint form
        }

        try {
          const { data } = await this.parent.xhr.put(endpoint, { listenKey });
          const isOk = this.isSuccessfulCode(data?.code) || data?.status === 'ok' || data?.msg === 'succ';
          if (isOk) {
            this.scheduleListenKeyRefresh(listenKey);
            return;
          }
        } catch (_error) {
          // try next endpoint
        }
      }

      this.emitAndLogError('Failed to refresh listen key');
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
