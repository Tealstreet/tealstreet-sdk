import type { BinanceExchange } from './binance.exchange';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';
import {
  LogSeverity,
  MarginType,
  OrderRejectReason,
  OrderStatus,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import {
  BASE_WS_URL,
  ENDPOINTS,
  ORDER_SIDE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
  TIME_IN_FORCE,
} from '../../../exchanges/binance/binance.types';

// Binance expires a listenKey 60 minutes after the last keepalive.
const LISTEN_KEY_REFRESH_MS = 30 * 60 * 1000;
const LISTEN_KEY_RETRY_MIN_MS = 5_000;
const LISTEN_KEY_RETRY_MAX_MS = 60_000;

export class BinancePrivateWebsocket extends BaseWebSocket<BinanceExchange> {
  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 10_000;
  private positionFetchScheduled = false;
  private positionFetchInFlight = false;
  private hasPendingPositionEvents = false;
  private ordersThrottleTimeout?: NodeJS.Timeout;
  private listenKeyRefreshTimer?: ReturnType<typeof setTimeout>;
  private listenKeyRetryDelayMs = LISTEN_KEY_RETRY_MIN_MS;
  private listenKeyRetryTimer?: ReturnType<typeof setTimeout>;
  // A pending retry, a native reconnect and forceOpen() can all call
  // connectAndSubscribe; overlapping calls each built a socket and leaked one.
  private connecting = false;
  private resyncOrdersOnOpen = false;
  private hasOpened = false;
  private keepaliveFailing = false;

  dispose() {
    clearTimeout(this.listenKeyRetryTimer);
    clearTimeout(this.listenKeyRefreshTimer);
    super.dispose();
  }

  connectAndSubscribe = async () => {
    // payloadSigner signs REST only; auth here needs the local secret.
    if (this.parent.options.payloadSigner) return;
    if (!this.isDisposed && !this.connecting) {
      clearTimeout(this.listenKeyRetryTimer);
      this.connecting = true;
      let listenKey: string;
      try {
        listenKey = await this.fetchListenKey();
      } catch (error) {
        // In hosted mode this request is signed by the browser over a link that can
        // drop. It used to reject un-awaited and leave the private stream unbuilt with
        // nothing retrying, so orders and positions stopped updating silently.
        const message = `Binance user data stream unavailable: ${error instanceof Error ? error.message : String(error)}`;
        // Toast once per failure streak; a bad key would otherwise toast every minute forever.
        if (this.listenKeyRetryDelayMs === LISTEN_KEY_RETRY_MIN_MS) this.emitAndLogError(message);
        else this.parent.log(message, LogSeverity.Warning);
        this.resyncOrdersOnOpen = true;
        const delay = this.listenKeyRetryDelayMs;
        this.listenKeyRetryDelayMs = Math.min(delay * 2, LISTEN_KEY_RETRY_MAX_MS);
        this.listenKeyRetryTimer = setTimeout(() => {
          if (!this.isDisposed && !this.isForceClosed) void this.connectAndSubscribe();
        }, delay);
        return;
      } finally {
        this.connecting = false;
      }
      this.listenKeyRetryDelayMs = LISTEN_KEY_RETRY_MIN_MS;
      if (this.isDisposed || this.isForceClosed) return;

      const key = this.parent.options.testnet ? 'testnet' : 'livenet';
      const base = BASE_WS_URL.private[key];

      // Post-2026-04-23 user-data URL format: /private/ws carries the listenKey
      // as a query param. `events` is optional — omitting it streams all event
      // types for the listenKey, matching the old `/ws/{listenKey}` behavior.
      const url = `${base}?listenKey=${encodeURIComponent(listenKey)}`;

      this.detachSocket();
      this.ws = this.createWebsocket(url);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('open', this.onOpen);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.ping();
      // Order events while no stream was open (a failed listenKey, or any reconnect)
      // were never delivered. The base house-keep resync cannot cover it: it attaches
      // to `this.ws` while this async connect is still awaiting the listenKey.
      if (this.resyncOrdersOnOpen || this.hasOpened) {
        this.resyncOrdersOnOpen = false;
        this.throttledFetchOrders();
      }
      this.hasOpened = true;
    }
  };

  /** forceOpen() on a live native socket used to build a second one and leak the first. */
  private detachSocket = () => {
    const previous = this.ws;
    if (!previous) return;
    previous.removeEventListener?.('message', this.onMessage);
    previous.removeEventListener?.('close', this.onClose);
    previous.removeEventListener?.('open', this.onOpen);
    if (previous.readyState === WebSocket.OPEN || previous.readyState === WebSocket.CONNECTING) previous.close();
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
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

      // Handle algo order updates (stop/TP/trailing orders as of 2025-12-09)
      if (json?.e === 'ALGO_UPDATE') {
        this.recordMessageHandlerStats('algo_order');
        this.handleAlgoOrderEvents([json]);
      }

      if (json?.id === 42) {
        this.handlePongEvent();
      }
    }
  };

  private getOrderRejectReason = (data: Record<string, any>): OrderRejectReason | undefined => {
    const rejectCode = String(data?.r || '').toUpperCase();
    if (rejectCode.includes('POST_ONLY') || rejectCode.includes('GTX') || rejectCode.includes('MAKER')) {
      return OrderRejectReason.PostOnly;
    }
    return undefined;
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ id: 42, method: 'LIST_SUBSCRIPTIONS' }));
    }
  };

  handleOrderEvents = (events: Array<Record<string, any>>) => {
    events.forEach(({ o: data }) => {
      if (data.X === 'PARTIALLY_FILLED' || data.X === 'FILLED') {
        this.parent.emitter.emit('fill', {
          id: data.i?.toString(),
          side: ORDER_SIDE[data.S],
          symbol: data.s,
          price: parseFloat(data.ap),
          amount: parseFloat(data.l),
        });

        // Reflect the cumulative fill (`z`) onto the existing store row.
        // Without this the chaser supervisor reads `filled: 0` (the value
        // set by the NEW event) until the 10s `throttledFetchOrders`
        // debounce fires, and re-places a moved-away order at the FULL
        // original size instead of the remaining size.
        const orderId = data.i?.toString();
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
      }

      if (data.X === 'NEW') {
        this.store.addOrUpdateOrder({
          id: data.i?.toString(),
          status: OrderStatus.Open,
          symbol: data.s,
          type: ORDER_TYPE[data.ot],
          timeInForce: TIME_IN_FORCE[data.f],
          side: ORDER_SIDE[data.S],
          price: parseFloat(data.p) || parseFloat(data.sp),
          amount: parseFloat(data.q),
          filled: parseFloat(data.z),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: parseFloat(data.q) - parseFloat(data.z),
          reduceOnly: data.R || false,
          info: { ...data, orderId: data.i?.toString() },
          ...(ORDER_TYPE[data.ot] === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(data, 'wt')] }),
          ...(ORDER_TYPE[data.ot] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(data, 'wt')] }),
        });
      }

      if (data.X === 'CANCELED' || data.X === 'EXPIRED' || data.X === 'CANCELLED') {
        const orderId = data.i?.toString();
        this.store.updateOrderStatus({ id: orderId }, OrderStatus.Canceled);

        const rejectReason = this.getOrderRejectReason(data);
        if (rejectReason) {
          const existingOrder = this.store.orders.find((order) => order.id === orderId);
          this.store.updateOrder(
            { id: orderId },
            {
              extra: {
                ...existingOrder?.extra,
                rejectReason,
              },
            },
          );
        }
      }

      if (data.X === 'FILLED') {
        this.store.updateOrderStatus({ id: data.i?.toString() }, OrderStatus.Closed);
      }
    });

    // Refresh orders after any order event processing
    this.throttledFetchOrders();
  };

  // Handle algo order updates (stop/TP/trailing orders as of 2025-12-09)
  // Event structure: { e: "ALGO_UPDATE", o: { caid, aid, o (orderType), s, S, X (status), ... } }
  handleAlgoOrderEvents = (events: Array<Record<string, any>>) => {
    events.forEach((event) => {
      // Order data is nested inside event.o
      const data = event.o;
      if (!data) return;

      // Always use aid (algoId) - Binance's order ID is reliable for all orders
      // clientAlgoId (caid) from website-placed orders is unusable for cancellation
      const orderId = data.aid?.toString();
      const algoStatus = data.X;
      const orderType = ORDER_TYPE[data.o];

      // TRIGGERED or TRIGGERING means the order has been activated
      if (algoStatus === 'TRIGGERED' || algoStatus === 'TRIGGERING') {
        // this.parent.emitter.emit('fill', {
        //   id: orderId,
        //   side: ORDER_SIDE[data.S],
        //   symbol: data.s,
        //   price: parseFloat(data.ap) || parseFloat(data.tp) || 0,
        //   amount: parseFloat(data.aq) || parseFloat(data.q) || 0,
        // });
        this.store.updateOrderStatus({ id: orderId }, OrderStatus.Closed);
      }

      if (algoStatus === 'NEW') {
        this.store.addOrUpdateOrder({
          id: orderId,
          status: OrderStatus.Open,
          symbol: data.s,
          type: orderType,
          side: ORDER_SIDE[data.S],
          price: parseFloat(data.tp) || 0,
          amount: parseFloat(data.q) || 0,
          filled: 0,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: parseFloat(data.q) || 0,
          reduceOnly: data.R || false,
          close: data.cp || false,
          info: { algoId: data.aid, ...data },
          ...(orderType === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[data.wt] }),
          ...(orderType === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[data.wt] }),
        });
      }

      if (
        algoStatus === 'CANCELED' ||
        algoStatus === 'CANCELLED' ||
        algoStatus === 'EXPIRED' ||
        algoStatus === 'REJECTED'
      ) {
        this.store.updateOrderStatus({ id: orderId }, OrderStatus.Canceled);
      }

      if (algoStatus === 'FINISHED') {
        this.store.updateOrderStatus({ id: orderId }, OrderStatus.Closed);
      }
    });

    // Refresh orders after any algo order event processing
    this.throttledFetchOrders();
  };

  handleAccountEvents = (events: Array<Record<string, any>>) => {
    events.forEach((event) => {
      if (event.a?.P?.length) {
        this.parent.markPositionStateUpdatedByWebsocket();
        this.hasPendingPositionEvents = true;
        this.throttledFetchPositions();
      }

      event.a.P?.forEach((p: Record<string, any>) => {
        const symbol = p.s;
        const signedContracts = parseFloat(p.pa);
        const positionMode = p.ps === 'BOTH' ? PositionMode.OneWay : PositionMode.Hedged;
        const eventSide = POSITION_SIDE[p.ps] || (signedContracts >= 0 ? PositionSide.Long : PositionSide.Short);
        const id = createPositionId(symbol, eventSide, positionMode);
        const position = this.parent.store.positions.find((candidate) => candidate.id === id);
        const side = signedContracts === 0 && position ? position.side : eventSide;

        // WS updates omit leverage, margin type and liquidation price. Preserve
        // those fields until the immediate REST repair fills them in. Unknown
        // zero rows are flat-symbol noise, not positions that need creating.
        if (!position && signedContracts === 0) return;

        const entryPrice = parseFloat(p.ep);
        const contracts = Math.abs(signedContracts);
        const upnl = parseFloat(p.up);

        this.store.addOrUpdatePosition({
          ...position,
          id,
          symbol,
          side,
          entryPrice,
          contracts,
          notional: Math.abs(signedContracts * entryPrice + upnl),
          unrealizedPnl: upnl,
          leverage: position?.leverage ?? 0,
          liquidationPrice: position?.liquidationPrice ?? 0,
          marginType: position?.marginType ?? MarginType.Cross,
          positionMode,
          info: {
            ...position?.info,
            ...p,
          },
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

    // If a fetch is already scheduled, mark as having pending events
    // and let the scheduled fetch handle it
    if (this.positionFetchScheduled) {
      return;
    }

    const timeSinceLastFetch = now - this.lastPositionFetchTime;

    if (timeSinceLastFetch >= this.positionFetchThrottleMs) {
      // Execute immediately if enough time has passed
      this.executePositionFetch();
    } else {
      // Schedule for later if we need to throttle
      this.positionFetchScheduled = true;
      setTimeout(() => {
        this.positionFetchScheduled = false;
        if (!this.isDisposed) {
          // Always execute if we had pending events
          if (this.hasPendingPositionEvents) {
            this.executePositionFetch();
          }
        }
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
      // Silent failure, will retry on next event
    } finally {
      this.positionFetchInFlight = false;

      // A newer WS event makes this response stale. Keep its immediate store
      // update and schedule a fresh full snapshot instead of overwriting it.
      if (!this.isDisposed && this.hasPendingPositionEvents) {
        this.throttledFetchPositions();
      }
    }
  };

  private fetchListenKey = async () => {
    const { data } = await this.parent.xhr.post(ENDPOINTS.LISTEN_KEY);
    this.scheduleListenKeyRefresh(LISTEN_KEY_REFRESH_MS);
    return data.listenKey;
  };

  /** One keepalive chain per socket: every reconnect used to start another. */
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
      // A failed keepalive must not end the chain; the key outlives it by 30 minutes.
      const message = `Binance user data stream keepalive failed: ${error instanceof Error ? error.message : String(error)}`;
      if (this.keepaliveFailing) this.parent.log(message, LogSeverity.Warning);
      else this.emitAndLogError(message);
      this.keepaliveFailing = true;
      this.scheduleListenKeyRefresh(LISTEN_KEY_RETRY_MAX_MS);
    }
  };

  private throttledFetchOrders = () => {
    if (this.ordersThrottleTimeout) {
      clearTimeout(this.ordersThrottleTimeout);
    }
    this.ordersThrottleTimeout = setTimeout(this.fetchThrottledOrders, 10000);
  };

  private fetchThrottledOrders = async () => {
    this.ordersThrottleTimeout = undefined;
    if (!this.isDisposed) {
      try {
        const orders = await this.parent.fetchOrders();
        // A failed fetch returns the current store; do not mark it freshly loaded.
        if (this.isDisposed || this.parent.ordersSnapshotStale) return;
        this.parent.store.update({
          orders,
          loaded: {
            ...this.parent.store.loaded,
            orders: true,
          },
        });
      } catch {
        // Silent failure, will retry on next event
      }
    }
  };
}
