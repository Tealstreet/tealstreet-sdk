import type { BinancePMExchange } from './binancepm.exchange';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { LogSeverity, OrderStatus, OrderType, PositionSide } from '../../../types';
import { v } from '../../../utils/get-key';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import { BASE_WS_URL, ENDPOINTS, ORDER_SIDE, ORDER_TRIGGER_TYPE, ORDER_TYPE } from '../../../exchanges/binance/binancepm.types';

// {
//   "e": "ORDER_TRADE_UPDATE",
//   "T": 1745184285796,
//   "E": 1745184285800,
//   "fs": "UM",
//   "o": {
//     "s": "BTCUSDT",
//     "c": "x-s3vFFRQpFchtbx2ykfTgF7sisb8SO",
//     "S": "BUY",
//     "o": "MARKET",
//     "f": "GTC",
//     "q": "0.002",
//     "p": "0",
//     "ap": "0",
//     "sp": "0",
//     "x": "NEW",
//     "X": "NEW",
//     "i": 664154815417,
//     "l": "0",
//     "z": "0",
//     "L": "0",
//     "n": "0",
//     "N": "USDT",
//     "T": 1745184285796,
//     "t": 0,
//     "b": "0",
//     "a": "0",
//     "m": false,
//     "R": false,
//     "ps": "BOTH",
//     "rp": "0",
//     "V": "EXPIRE_MAKER",
//     "pm": "PM_NONE",
//     "gtd": 0
//   }
// }

// {
//   "e": "ORDER_TRADE_UPDATE",
//   "T": 1745184285796,
//   "E": 1745184285800,
//   "fs": "UM",
//   "o": {
//     "s": "BTCUSDT",
//     "c": "x-s3vFFRQpFchtbx2ykfTgF7sisb8SO",
//     "S": "BUY",
//     "o": "MARKET",
//     "f": "GTC",
//     "q": "0.002",
//     "p": "0",
//     "ap": "84903.90000",
//     "sp": "0",
//     "x": "TRADE",
//     "X": "FILLED",
//     "i": 664154815417,
//     "l": "0.002",
//     "z": "0.002",
//     "L": "84903.90",
//     "n": "0.08490390",
//     "N": "USDT",
//     "T": 1745184285796,
//     "t": 6232985189,
//     "b": "0",
//     "a": "0",
//     "m": false,
//     "R": false,
//     "ps": "BOTH",
//     "rp": "0",
//     "V": "EXPIRE_MAKER",
//     "pm": "PM_NONE",
//     "gtd": 0
//   }
// }

// Binance expires a listenKey 60 minutes after the last keepalive.
const LISTEN_KEY_REFRESH_MS = 30 * 60 * 1000;
const LISTEN_KEY_RETRY_MIN_MS = 5_000;
const LISTEN_KEY_RETRY_MAX_MS = 60_000;

export class BinancePMPrivateWebsocket extends BaseWebSocket<BinancePMExchange> {
  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 500;
  private positionFetchScheduled = false;
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

      const url = `${base}/${listenKey}`;

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

      const json = jsonParse(data);

      if (json?.e === 'ACCOUNT_UPDATE') {
        this.recordMessageHandlerStats('position');
        this.handleAccountEvents([json]);
      }

      if (json?.e === 'ORDER_TRADE_UPDATE') {
        this.recordMessageHandlerStats('order');
        this.handleOrderEvents([json]);
      }

      if (json?.e === 'CONDITIONAL_ORDER_TRADE_UPDATE') {
        this.recordMessageHandlerStats('conditional-order');
        this.handleConditionalOrderEvents([json]);
      }

      if (json?.id === 42) {
        this.handlePongEvent();
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ id: 42, method: 'LIST_SUBSCRIPTIONS' }));
    }
  };

  // {
  //   "e": "ORDER_TRADE_UPDATE",
  //   "T": 1745264277328,
  //   "E": 1745264277335,
  //   "fs": "UM",
  //   "o": {
  //     "s": "BTCUSDT",
  //     "c": "x-s3vFFRQpD9iT7CDVq2bwzGv94GcZ",
  //     "S": "BUY",
  //     "o": "LIMIT",
  //     "f": "GTC",
  //     "q": "0.002",
  //     "p": "81723.30",
  //     "ap": "0",
  //     "sp": "0",
  //     "x": "CANCELED",
  //     "X": "CANCELED",
  //     "i": 665103327274,
  //     "l": "0",
  //     "z": "0",
  //     "L": "0",
  //     "n": "0",
  //     "N": "USDT",
  //     "T": 1745264277328,
  //     "t": 0,
  //     "b": "0",
  //     "a": "0",
  //     "m": false,
  //     "R": false,
  //     "ps": "BOTH",
  //     "rp": "0",
  //     "V": "EXPIRE_MAKER",
  //     "pm": "PM_NONE",
  //     "gtd": 0
  //   }
  // }

  handleOrderEvents = (events: Array<Record<string, any>>) => {
    events.forEach(({ o: data }) => {
      if (data.X === 'PARTIALLY_FILLED' || data.X === 'FILLED') {
        this.parent.emitter.emit('fill', {
          id: data.c,
          side: ORDER_SIDE[data.S],
          symbol: data.s,
          price: parseFloat(data.ap),
          amount: parseFloat(data.l),
        });

        // Mirror cumulative fill (`z`) onto the store row so the chaser
        // supervisor sees the up-to-date `filled` / `remaining` before
        // its next cancel/replace decision. See the matching block in
        // `binance.ws-private.ts` for the futures-variant rationale.
        const filled = parseFloat(data.z);
        const amount = parseFloat(data.q);
        this.store.updateOrder(
          { id: data.c },
          {
            filled,
            remaining: amount - filled,
            lastUpdate: virtualClock.getCurrentTimeMs(),
          },
        );
      }

      if (data.X === 'NEW') {
        this.store.addOrUpdateOrder({
          id: data.c,
          status: OrderStatus.Open,
          symbol: data.s,
          type: ORDER_TYPE[data.o],
          side: ORDER_SIDE[data.S],
          price: parseFloat(data.p) || parseFloat(data.sp),
          amount: parseFloat(data.q),
          filled: parseFloat(data.z),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: parseFloat(data.q) - parseFloat(data.z),
          reduceOnly: data.R || false,
          info: data,
          ...(ORDER_TYPE[data.o] === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(data, 'wt')] }),
          ...(ORDER_TYPE[data.o] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(data, 'wt')] }),
        });
      }

      if (data.X === 'CANCELED' || data.X === 'EXPIRED' || data.X === 'CANCELLED') {
        this.store.updateOrderStatus({ id: data.c }, OrderStatus.Canceled);
      }

      if (data.X === 'FILLED') {
        this.store.updateOrderStatus({ id: data.c }, OrderStatus.Closed);
      }
    });

    // Refresh orders after any order event processing to clean up
    // orphaned SL/TP orders that the WebSocket may not report as cancelled
    // (e.g. when a TP fills, Binance cancels the linked SL server-side
    // but doesn't always emit a CANCELED event for it).
    this.throttledFetchOrders();
  };

  // {
  //   "e": "CONDITIONAL_ORDER_TRADE_UPDATE",
  //   "T": 1745264309202,
  //   "E": 1745264309204,
  //   "fs": "UM",
  //   "so": {
  //     "s": "BTCUSDT",
  //     "c": "8m8Agr2UMOqWOp9cNw2902",
  //     "si": 60684811,
  //     "S": "SELL",
  //     "st": "TAKE_PROFIT_MARKET",
  //     "f": "GTC",
  //     "q": "0.002",
  //     "p": "0",
  //     "sp": "91565.90",
  //     "os": "CANCELLED",
  //     "T": 1745257188395,
  //     "ut": 1745264309202,
  //     "R": false,
  //     "wt": "CONTRACT_PRICE",
  //     "ps": "BOTH",
  //     "cp": false,
  //     "AP": "0",
  //     "cr": "0.0",
  //     "i": 0,
  //     "V": "EXPIRE_MAKER",
  //     "gtd": 0
  //   }
  // }

  // {
  //   "e": "CONDITIONAL_ORDER_TRADE_UPDATE",
  //   "T": 1745267036801,
  //   "E": 1745267036802,
  //   "fs": "UM",
  //   "so": {
  //     "s": "BTCUSDT",
  //     "c": "K1zBlG0XEJXsmRALM66466",
  //     "si": 60685744,
  //     "S": "SELL",
  //     "st": "TAKE_PROFIT_MARKET",
  //     "f": "GTC",
  //     "q": "0.002",
  //     "p": "0",
  //     "sp": "90972.60",
  //     "os": "CANCELLED",
  //     "T": 1745260922458,
  //     "ut": 1745267036801,
  //     "R": false,
  //     "wt": "CONTRACT_PRICE",
  //     "ps": "BOTH",
  //     "cp": false,
  //     "AP": "0",
  //     "cr": "0.0",
  //     "i": 0,
  //     "V": "EXPIRE_MAKER",
  //     "gtd": 0
  //   }
  // }
  handleConditionalOrderEvents = (events: Array<Record<string, any>>) => {
    events.forEach(({ so: data }) => {
      if (data.os === 'PARTIALLY_FILLED' || data.os === 'FILLED') {
        this.parent.emitter.emit('fill', {
          id: data.c,
          side: ORDER_SIDE[data.S],
          symbol: data.s,
          price: parseFloat(data.ap),
          amount: parseFloat(data.l),
        });
      }

      if (data.os === 'NEW') {
        this.store.addOrUpdateOrder({
          id: data.c,
          status: OrderStatus.Open,
          symbol: data.s,
          type: ORDER_TYPE[data.st],
          side: ORDER_SIDE[data.S],
          price: parseFloat(data.p) || parseFloat(data.sp),
          amount: parseFloat(data.q),
          filled: parseFloat(data.z) || 0,
          lastUpdate: virtualClock.getCurrentTimeMs(),
          remaining: parseFloat(data.q) - parseFloat(data.z) || 0,
          reduceOnly: data.R || false,
          info: data,
          ...(ORDER_TYPE[data.st] === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(data, 'wt')] }),
          ...(ORDER_TYPE[data.st] === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(data, 'wt')] }),
        });
      }

      if (data.os === 'CANCELED' || data.os === 'EXPIRED' || data.os === 'CANCELLED') {
        this.store.updateOrderStatus({ id: data.c }, OrderStatus.Canceled);
      }

      if (data.os === 'FILLED') {
        this.store.updateOrderStatus({ id: data.c }, OrderStatus.Closed);
      }
    });

    // Refresh orders after any conditional order event processing
    this.throttledFetchOrders();
  };

  handleAccountEvents = (events: Array<Record<string, any>>) => {
    // {
    //   "e": "ACCOUNT_UPDATE",
    //   "T": 1745184285796,
    //   "E": 1745184285800,
    //   "fs": "UM",
    //   "a": {
    //     "B": [
    //       {
    //         "a": "USDT",
    //         "wb": "62.25045499",
    //         "cw": "62.25045499",
    //         "bc": "0"
    //       }
    //     ],
    //     "P": [
    //       {
    //         "s": "BTCUSDT",
    //         "pa": "0.006",
    //         "ep": "84803.13333333",
    //         "cr": "898.47079984",
    //         "up": "0.60400000",
    //         "ps": "BOTH",
    //         "bep": 84845.5349
    //       }
    //     ],
    //     "m": "ORDER"
    //   }
    // }
    events.forEach((event) => {
      if (event.a?.P) {
        this.hasPendingPositionEvents = true;
        this.throttledFetchPositions();
      }
      event.a?.P?.forEach((p: Record<string, any>) => {
        const symbol = p.s;
        const side = p.pa > 0 ? PositionSide.Long : PositionSide.Short;
        const isOneWay = p.ps === 'BOTH';

        const position = this.parent.store.positions.find(
          (p2) => p2.symbol === symbol && (isOneWay ? true : p2.side === side),
        );

        if (position) {
          const entryPrice = parseFloat(p.ep);
          const contracts = Math.abs(parseFloat(p.pa));
          const upnl = parseFloat(p.up);

          this.store.updatePosition(position, {
            entryPrice,
            contracts,
            notional: contracts * entryPrice,
            unrealizedPnl: upnl,
            info: position,
            ...(isOneWay && { side }),
          });
        }
      });

      // event.a.B.forEach((p: Record<string, any>) => {
      //   const symbol = p.s;
      //   const side = POSITION_SIDE[p.ps];

      //   const position = this.parent.store.positions.find((p2) => p2.symbol === symbol && p2.side === side);

      //   if (position) {
      //     const entryPrice = parseFloat(p.ep);
      //     const contracts = parseFloat(p.pa);
      //     const upnl = parseFloat(p.up);

      //     this.store.updatePosition(position, {
      //       entryPrice,
      //       contracts,
      //       notional: contracts * entryPrice,
      //       unrealizedPnl: upnl,
      //       info: position,
      //     });
      //   }
      // });
    });
  };

  private throttledFetchPositions = () => {
    const now = Date.now();

    if (this.isDisposed) {
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
    try {
      this.lastPositionFetchTime = Date.now();
      this.hasPendingPositionEvents = false;
      const positions = await this.parent.fetchPositions();
      if (!this.isDisposed) {
        this.store.addOrUpdatePositionsFull(positions);
      }
    } catch {
      // Silent failure, will retry on next event
    }
  };

  private fetchListenKey = async () => {
    const { data } = await this.parent.papiXhr.post(ENDPOINTS.LISTEN_KEY);
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
      await this.parent.papiXhr.put(ENDPOINTS.LISTEN_KEY);
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
