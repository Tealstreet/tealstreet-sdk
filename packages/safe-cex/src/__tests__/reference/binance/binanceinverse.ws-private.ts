import type { BinanceInverseExchange } from './binanceinverse.exchange';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity, MarginType, OrderRejectReason, OrderStatus, OrderType, PositionMode, PositionSide } from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ENDPOINTS, ORDER_SIDE, ORDER_TRIGGER_TYPE, ORDER_TYPE, POSITION_SIDE } from '../../../exchanges/binance/binanceinverse.types';

const LISTEN_KEY_REFRESH_MS = 30 * 60 * 1000;
const LISTEN_KEY_RETRY_MIN_MS = 5_000;
const LISTEN_KEY_RETRY_MAX_MS = 60_000;

export class BinanceInversePrivateWebsocket extends BaseWebSocket<BinanceInverseExchange> {
  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 10_000;
  private positionFetchScheduled = false;
  private positionFetchInFlight = false;
  private hasPendingPositionEvents = false;
  private ordersThrottleTimeout?: NodeJS.Timeout;
  private listenKeyRefreshTimer?: ReturnType<typeof setTimeout>;
  private listenKeyRetryDelayMs = LISTEN_KEY_RETRY_MIN_MS;
  private listenKeyRetryTimer?: ReturnType<typeof setTimeout>;
  private connecting = false;
  private resyncOrdersOnOpen = false;
  private hasOpened = false;
  private keepaliveFailing = false;

  dispose() {
    clearTimeout(this.listenKeyRetryTimer);
    clearTimeout(this.listenKeyRefreshTimer);
    clearTimeout(this.ordersThrottleTimeout);
    super.dispose();
  }

  connectAndSubscribe = async () => {
    if (this.isDisposed || this.connecting) return;

    this.connecting = true;
    let listenKey: string;
    try {
      listenKey = await this.fetchListenKey();
    } catch (error) {
      const message = `Binance COIN-M user data stream unavailable: ${
        error instanceof Error ? error.message : String(error)
      }`;
      if (this.listenKeyRetryDelayMs === LISTEN_KEY_RETRY_MIN_MS) this.emitAndLogError(message);
      else this.parent.log(message, LogSeverity.Warning);
      this.resyncOrdersOnOpen = true;
      const delay = this.listenKeyRetryDelayMs;
      this.listenKeyRetryDelayMs = Math.min(delay * 2, LISTEN_KEY_RETRY_MAX_MS);
      this.listenKeyRetryTimer = setTimeout(() => {
        if (!this.isDisposed && !this.isForceClosed) void this.connectAndSubscribe();
      }, delay);
      this.connecting = false;
      return;
    }
    this.connecting = false;
    this.listenKeyRetryDelayMs = LISTEN_KEY_RETRY_MIN_MS;
    if (this.isDisposed || this.isForceClosed) return;

    const base = BASE_WS_URL[this.parent.options.testnet ? 'testnet' : 'livenet'];
    this.detachSocket();
    this.ws = this.createWebsocket(`${base}/${encodeURIComponent(listenKey)}`);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.onClose);
    this.ws.addEventListener('open', this.onOpen);
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.ping();
      if (this.resyncOrdersOnOpen || this.hasOpened) {
        this.resyncOrdersOnOpen = false;
        this.throttledFetchOrders();
      }
      this.hasOpened = true;
    }
  };

  private detachSocket = () => {
    const previous = this.ws;
    if (!previous) return;
    previous.removeEventListener?.('message', this.onMessage);
    previous.removeEventListener?.('close', this.onClose);
    previous.removeEventListener?.('open', this.onOpen);
    if (previous.readyState === WebSocket.OPEN || previous.readyState === WebSocket.CONNECTING) previous.close();
  };

  onMessage = ({ data }: MessageEvent) => {
    if (this.isDisposed) return;
    this.trackLastReceived();
    this.emitWsDiagnosticReceive(data);

    const json = bigIntAsStringJsonParse(data);
    if (json?.e === 'ACCOUNT_UPDATE') {
      this.recordMessageHandlerStats('position');
      this.handleAccountEvents([json]);
    }
    if (json?.e === 'ORDER_TRADE_UPDATE') {
      this.recordMessageHandlerStats('order');
      this.handleOrderEvents([json]);
    }
    if (json?.id === 42) {
      this.handlePongEvent();
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ id: 42, method: 'LIST_SUBSCRIPTIONS' }));
    }
  };

  private getOrderRejectReason = (data: Record<string, any>): OrderRejectReason | undefined => {
    const rejectCode = String(data?.r || '').toUpperCase();
    if (rejectCode.includes('POST_ONLY') || rejectCode.includes('GTX') || rejectCode.includes('MAKER')) {
      return OrderRejectReason.PostOnly;
    }
    return undefined;
  };

  handleOrderEvents = (events: Array<Record<string, any>>) => {
    events.forEach(({ o: data }) => {
      const orderId = data.i?.toString();
      const amount = parseFloat(data.q);
      const filled = parseFloat(data.z);

      if (data.X === 'PARTIALLY_FILLED' || data.X === 'FILLED') {
        this.parent.emitter.emit('fill', {
          id: orderId,
          side: ORDER_SIDE[data.S],
          symbol: data.s,
          price: parseFloat(data.ap),
          amount: parseFloat(data.l),
        });
        this.store.updateOrder({ id: orderId }, { filled, remaining: amount - filled, lastUpdate: virtualClock.getCurrentTimeMs() });
      }

      if (data.X === 'NEW') {
        this.store.addOrUpdateOrder({
          id: orderId,
          status: OrderStatus.Open,
          symbol: data.s,
          type: ORDER_TYPE[data.ot],
          side: ORDER_SIDE[data.S],
          price: parseFloat(data.p) || parseFloat(data.sp),
          amount,
          filled,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: amount - filled,
          reduceOnly: data.R || false,
          close: data.cp || false,
          info: { ...data, orderId },
          ...(ORDER_TYPE[data.ot] === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(data, 'wt')] }),
          ...(ORDER_TYPE[data.ot] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(data, 'wt')] }),
        });
      }

      if (data.X === 'CANCELED' || data.X === 'EXPIRED' || data.X === 'CANCELLED') {
        this.store.updateOrderStatus({ id: orderId }, OrderStatus.Canceled);
        const rejectReason = this.getOrderRejectReason(data);
        if (rejectReason) {
          const existingOrder = this.store.orders.find((order) => order.id === orderId);
          this.store.updateOrder({ id: orderId }, { extra: { ...existingOrder?.extra, rejectReason } });
        }
      }

      if (data.X === 'FILLED') {
        this.store.updateOrderStatus({ id: orderId }, OrderStatus.Closed);
      }
    });
    this.throttledFetchOrders();
  };

  handleAccountEvents = (events: Array<Record<string, any>>) => {
    events.forEach((event) => {
      if (event.a?.P?.length) {
        this.parent.markPositionStateUpdatedByWebsocket();
        this.hasPendingPositionEvents = true;
        this.throttledFetchPositions();
      }

      event.a?.P?.forEach((p: Record<string, any>) => {
        const symbol = p.s;
        const signedNativeContracts = parseFloat(p.pa);
        const positionMode = p.ps === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
        const eventSide = POSITION_SIDE[p.ps] || (signedNativeContracts >= 0 ? PositionSide.Long : PositionSide.Short);
        const id = createPositionId(symbol, eventSide, positionMode);
        const position = this.parent.store.positions.find((candidate) => candidate.id === id);
        if (!position && signedNativeContracts === 0) return;

        const market = this.parent.store.marketsBySymbol[symbol];
        const ticker = this.parent.store.tickersBySymbol[symbol];
        const entryPrice = parseFloat(p.ep);
        const upnl = parseFloat(p.up);
        const referencePrice = ticker?.mark || ticker?.last || entryPrice;
        const nativeContracts = Math.abs(signedNativeContracts);
        const contractSize = market?.contractSize || 1;
        const baseContracts = referencePrice > 0 ? nativeContracts * contractSize / referencePrice : position?.contracts || 0;
        const side = signedNativeContracts === 0 && position ? position.side : eventSide;

        this.store.addOrUpdatePosition({
          ...position,
          id,
          symbol,
          side,
          entryPrice,
          contracts: baseContracts,
          notional: nativeContracts * contractSize,
          unrealizedPnl: upnl,
          leverage: position?.leverage ?? 0,
          liquidationPrice: position?.liquidationPrice ?? 0,
          marginType: position?.marginType ?? MarginType.Cross,
          positionMode,
          info: { ...position?.info, ...p, contractsNative: nativeContracts },
        });
      });
    });
  };

  private throttledFetchPositions = () => {
    const now = Date.now();
    if (this.isDisposed || this.parent.isBackgroundReconciliationPaused()) {
      this.hasPendingPositionEvents = false;
      return;
    }
    if (this.positionFetchScheduled) return;
    const timeSinceLastFetch = now - this.lastPositionFetchTime;
    if (timeSinceLastFetch >= this.positionFetchThrottleMs) {
      this.executePositionFetch();
    } else {
      this.positionFetchScheduled = true;
      setTimeout(() => {
        this.positionFetchScheduled = false;
        if (!this.isDisposed && this.hasPendingPositionEvents) this.executePositionFetch();
      }, this.positionFetchThrottleMs - timeSinceLastFetch);
    }
  };

  private executePositionFetch = async () => {
    if (this.positionFetchInFlight || this.isDisposed || this.parent.isBackgroundReconciliationPaused()) return;
    const eventVersionAtStart = this.parent.getPositionStateVersion();
    this.positionFetchInFlight = true;
    this.lastPositionFetchTime = Date.now();
    this.hasPendingPositionEvents = false;
    try {
      const positions = await this.parent.fetchPositions();
      if (!this.isDisposed && eventVersionAtStart === this.parent.getPositionStateVersion()) {
        this.store.addOrUpdatePositionsFull(positions);
      }
    } catch {
      // Retry on the next account event.
    } finally {
      this.positionFetchInFlight = false;
      if (!this.isDisposed && this.hasPendingPositionEvents) this.throttledFetchPositions();
    }
  };

  private fetchListenKey = async () => {
    const { data } = await this.parent.xhr.post(ENDPOINTS.LISTEN_KEY);
    this.scheduleListenKeyRefresh(LISTEN_KEY_REFRESH_MS);
    return data.listenKey;
  };

  private scheduleListenKeyRefresh = (delayMs: number) => {
    clearTimeout(this.listenKeyRefreshTimer);
    this.listenKeyRefreshTimer = setTimeout(() => void this.updateListenKey(), delayMs);
  };

  private updateListenKey = async () => {
    if (this.isDisposed) return;
    try {
      await this.parent.xhr.put(ENDPOINTS.LISTEN_KEY);
      this.scheduleListenKeyRefresh(LISTEN_KEY_REFRESH_MS);
      this.keepaliveFailing = false;
    } catch (error) {
      const message = `Binance COIN-M user data stream keepalive failed: ${
        error instanceof Error ? error.message : String(error)
      }`;
      if (this.keepaliveFailing) this.parent.log(message, LogSeverity.Warning);
      else this.emitAndLogError(message);
      this.keepaliveFailing = true;
      this.scheduleListenKeyRefresh(LISTEN_KEY_RETRY_MAX_MS);
    }
  };

  private throttledFetchOrders = () => {
    if (this.ordersThrottleTimeout) clearTimeout(this.ordersThrottleTimeout);
    this.ordersThrottleTimeout = setTimeout(this.fetchThrottledOrders, 10_000);
  };

  private fetchThrottledOrders = async () => {
    this.ordersThrottleTimeout = undefined;
    if (this.isDisposed) return;
    try {
      const orders = await this.parent.fetchOrders();
      if (this.isDisposed || this.parent.ordersSnapshotStale) return;
      this.parent.store.update({ orders, loaded: { ...this.parent.store.loaded, orders: true } });
    } catch {
      // Fetch failures are retried on the next order event.
    }
  };
}
