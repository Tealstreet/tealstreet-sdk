import type { BitgetExchange } from './bitget.exchange';
import { resolveExchangeRouting } from '../../routing/resolveExchangeRouting';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_PWS_LOG, ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { OrderStatus, OrderType } from '../../types';
import { subtract } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { ORDER_STATUS } from './bitget.types';


export class BitgetPrivateWebsocket extends BaseWebSocket<BitgetExchange> {
  private messageCount = 0;
  private useFallbackUrl = false;
  private lastMessageCount = 0;
  private connectionAttempts = 0;
  private readonly MAX_ATTEMPTS = 2;
  private lastPositionFetchTime = 0;
  private positionFetchThrottleMs = 500;
  private positionFetchScheduled = false;
  private hasPendingPositionEvents = false;
  private lastOrdersFetchTime = 0;
  private ordersFetchThrottleMs = 500;
  private ordersFetchScheduled = false;
  private hasPendingOrderEvents = false;
  private ordersFetchRequestSeq = 0;
  private terminalOrderTombstones = new Map<string, number>();
  private recentWsOpenOrders = new Map<string, number>();
  private readonly orderTombstoneTtlMs = 10_000;
  private readonly wsOpenOrderGraceMs = 5_000;
  private lastAlgoOrdersFetchTime = 0;
  private algoOrdersFetchThrottleMs = 500;
  private algoOrdersFetchScheduled = false;
  private hasPendingAlgoOrderEvents = false;
  private lastBalanceFetchTime = 0;
  private balanceFetchThrottleMs = 300;
  private balanceFetchScheduled = false;
  private hasPendingBalanceEvents = false;

  private toFiniteNumber = (value: unknown, fallback = 0): number => {
    if (typeof value === 'number') {
      return Number.isFinite(value) ? value : fallback;
    }
    if (typeof value === 'string') {
      if (value.trim() === '') return fallback;
      const parsed = Number.parseFloat(value);
      return Number.isFinite(parsed) ? parsed : fallback;
    }
    return fallback;
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      // Check if we need to switch to fallback URL
      if (this.lastMessageCount === 0 && this.messageCount === 0) {
        this.connectionAttempts++;
        if (this.connectionAttempts >= this.MAX_ATTEMPTS) {
          this.useFallbackUrl = true;
        }
      }
      this.lastMessageCount = this.messageCount;
      this.messageCount = 0; // Reset message counter on new connection

      const url = this.useFallbackUrl ? resolveExchangeRouting(this.parent.name, this.parent.options).wsPrivate.fallback! : 'wss://ws.bitget.com/v2/ws/private';
      if (this.useFallbackUrl) {
        try {
          this.parent.emitter.emit('grafana-info', 'Using tealstreet bitget proxy (private)');
        } catch (_e) {}
      }
      this.ws = this.createWebsocket(url);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.auth();
      this.ping();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        // Keep reconnect snapshot on the same reconciliation path as WS-triggered fetches.
        void this.executeOrdersFetch();
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendWithLog('ping');
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.messageCount++; // Increment message counter

      if (ENABLE_PWS_LOG) {
        this.parent.emitter.emit('grafana-pws', `${data}`);
      }

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

    }

    const json = jsonParse<Record<string, any>>(data);

    if (json?.event === 'login') {
      if (`${json.code}` === '0') {
        this.subscribe();
      } else if (json.msg || json.code) {
        this.emitAndLogError(`Bitget private WS login failed: ${json.msg || json.code}`);
      }
      return;
    }

    if (json?.arg?.channel === 'orders' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('order');
      this.handleOrderTopic(json);
      return;
    }

    if (json?.arg?.channel === 'orders-algo' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('order-algo');
      this.handleAlgoOrdersTopic(json);
      return;
    }

    if (json?.arg?.channel === 'positions' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('position');
      this.handlePositionsTopic(json);
      return;
    }

    if (json?.arg?.channel === 'account' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('account');
      this.handleAccountTopic(json);
      return;
    }
  };

  handleOrderTopic = (data: Record<string, any>) => {
    const rawOrders = data.data;

    for (const rawOrder of rawOrders) {
      const normalizedStatus = String(rawOrder.status || '').toLowerCase();
      const isPartialFill = normalizedStatus === 'partial-fill' || normalizedStatus === 'partial_fill';
      const isFullFill =
        normalizedStatus === 'filled' || normalizedStatus === 'full-fill' || normalizedStatus === 'full_fill';

      const orders = this.parent.mapOrder({
        ...rawOrder,
        // invert side for hedge mode because bitget developers are ...
        side:
          rawOrder.posMode === 'hedge_mode' && rawOrder.reduceOnly === 'yes'
            ? rawOrder.side === 'sell'
              ? 'buy'
              : 'sell'
            : rawOrder.side,
      });
      const parentOrder = orders[0];

      if (isFullFill || isPartialFill) {
        this.emitter.emit('fill', {
          id: parentOrder.id,
          side: parentOrder.side,
          symbol: parentOrder.symbol,
          price: rawOrder.priceAvg || parentOrder.price,
          amount: parseFloat(rawOrder.fillSz || rawOrder.size || 0),
        });
        this.throttledFetchBalance();
      }

      if (isFullFill) {
        // Closing/filling events can leave available margin stale in WS account payloads.
        // Kick off fast REST reconciliations for both positions and balance.
        this.throttledFetchPositions();
        this.throttledFetchBalance();
      }

      if (rawOrder.status === 'cancel' || rawOrder.status === 'canceled' || rawOrder.status === 'cancelled') {
        orders.forEach((order) => {
          this.store.updateOrderStatus(order, OrderStatus.Canceled);
          this.markOrderAsTerminal(order.id);
        });
      }

      if (isFullFill) {
        orders.forEach((order) => {
          this.store.updateOrderStatus(order, OrderStatus.Closed);
          this.markOrderAsTerminal(order.id);
        });
      }

      if (rawOrder.status === 'new' || isPartialFill || rawOrder.status === 'live') {
        orders.forEach((order) => {
          this.store.addOrUpdateOrder(order);
          this.markOrderAsOpenFromWs(order.id);
        });
      }
    }
    this.hasPendingOrderEvents = true;
    this.throttledFetchOrders();
  };

  handleAlgoOrdersTopic = (data: Record<string, any>) => {
    const rawOrders = data.data;

    for (const rawOrder of rawOrders) {
      const orders = this.parent.mapOrder({
        ...rawOrder,
        // invert side for hedge mode because bitget developers are ...
        side: rawOrder.posMode === 'hedge_mode' ? (rawOrder.side === 'sell' ? 'buy' : 'sell') : rawOrder.side,
      });
      const parentOrder = orders[0];

      // Update order status based on the status field
      if (rawOrder.status === 'live') {
        this.store.addOrUpdateOrder(parentOrder);
        this.markOrderAsOpenFromWs(parentOrder.id);
      } else if (rawOrder.status === 'cancel' || rawOrder.status === 'canceled' || rawOrder.status === 'cancelled') {
        this.store.updateOrderStatus(parentOrder, OrderStatus.Canceled);
        this.markOrderAsTerminal(parentOrder.id);
      } else if (rawOrder.status === 'triggered' || rawOrder.status === 'filled') {
        this.store.updateOrderStatus(parentOrder, OrderStatus.Closed);
        this.markOrderAsTerminal(parentOrder.id);
      } else if (ORDER_STATUS[rawOrder.status]) {
        const mappedStatus = ORDER_STATUS[rawOrder.status];
        this.store.updateOrderStatus(parentOrder, mappedStatus);
        if (mappedStatus === OrderStatus.Open) {
          this.markOrderAsOpenFromWs(parentOrder.id);
        } else if (mappedStatus === OrderStatus.Canceled || mappedStatus === OrderStatus.Closed) {
          this.markOrderAsTerminal(parentOrder.id);
        }
      }

      // Emit fill event if the order was triggered
      if ((rawOrder.status === 'triggered' || rawOrder.status === 'filled') && rawOrder.actualSize) {
        this.emitter.emit('fill', {
          id: parentOrder.id,
          side: parentOrder.side,
          symbol: parentOrder.symbol,
          price: parseFloat(rawOrder.triggerPrice),
          amount: parseFloat(rawOrder.actualSize),
        });
        this.throttledFetchBalance();
      }

      if (rawOrder.status === 'triggered' || rawOrder.status === 'filled') {
        this.throttledFetchPositions();
        this.throttledFetchBalance();
      }
    }

    this.hasPendingOrderEvents = true;
    this.throttledFetchOrders();
  };

  handlePositionsTopic = (data: Record<string, any>) => {
    const action = typeof data?.action === 'string' ? data.action : undefined;
    const rawList = Array.isArray(data?.data) ? (data.data as Array<Record<string, any>>) : [];
    const validItems = rawList.filter((p) => p && typeof p === 'object');

    if (validItems.length === 0) {
      // Bitget can send an empty snapshot when there are no open positions.
      // Apply immediately so UI does not wait on periodic tick reconciliation.
      if (action === 'snapshot') {
        this.store.addOrUpdatePositionsFull([]);
      }
      this.hasPendingPositionEvents = true;
      this.throttledFetchPositions();
      this.throttledFetchBalance();
      return;
    }

    const positions = validItems.map((rawPosition) => this.parent.mapPosition(rawPosition));

    // Snapshot should be treated as full state to clear stale positions that are omitted.
    if (action === 'snapshot') {
      this.store.addOrUpdatePositionsFull(positions);
    } else {
      for (const position of positions) {
        this.store.addOrUpdatePosition(position);
      }
    }

    this.hasPendingPositionEvents = true;
    this.throttledFetchPositions();
    this.throttledFetchBalance();
  };

  handleAccountTopic = (data: Record<string, any>) => {
    const rawList = Array.isArray(data?.data) ? (data.data as Array<Record<string, any>>) : [];
    const acct = rawList.find((b) => b.marginCoin === this.parent.apiMarginCoin);
    if (!acct) return;

    const unionTotalMargin = this.toFiniteNumber(acct.unionTotalMargin, NaN);
    const unionAvailable = this.toFiniteNumber(acct.unionAvailable, NaN);
    const apiUpnl = this.toFiniteNumber(
      acct.unrealizedPL,
      this.toFiniteNumber(acct.crossedUnrealizedPL) + this.toFiniteNumber(acct.isolatedUnrealizedPL),
    );

    if (isNaN(unionTotalMargin) || isNaN(unionAvailable)) return;

    // Bitget account WS unrealizedPL can be stale/empty; derive from positions + last price when possible.
    const upnl = this.parent.calculateAccountUpnl(apiUpnl);

    this.store.update({
      balance: {
        used: subtract(unionTotalMargin, unionAvailable),
        free: unionAvailable,
        total: subtract(unionTotalMargin, upnl),
        upnl,
        info: acct,
      },
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

  private throttledFetchBalance = () => {
    const now = Date.now();

    if (this.isDisposed) {
      return;
    }

    this.hasPendingBalanceEvents = true;

    if (this.balanceFetchScheduled) {
      return;
    }

    const timeSinceLastFetch = now - this.lastBalanceFetchTime;
    if (timeSinceLastFetch >= this.balanceFetchThrottleMs) {
      this.executeBalanceFetch();
    } else {
      this.balanceFetchScheduled = true;
      setTimeout(() => {
        this.balanceFetchScheduled = false;
        if (!this.isDisposed && this.hasPendingBalanceEvents) {
          this.executeBalanceFetch();
        }
      }, this.balanceFetchThrottleMs - timeSinceLastFetch);
    }
  };

  private executeBalanceFetch = async () => {
    try {
      this.lastBalanceFetchTime = Date.now();
      this.hasPendingBalanceEvents = false;
      const balance = await this.parent.fetchBalance({ preferFast: true });
      if (!this.isDisposed) {
        this.store.update({
          balance,
          loaded: {
            ...this.store.loaded,
            balance: true,
          },
        });
      }
    } catch {
      // Silent failure, will retry on next event
    }
  };

  private throttledFetchOrders = () => {
    const now = Date.now();

    if (this.isDisposed) {
      return;
    }

    if (this.ordersFetchScheduled) {
      return;
    }

    const timeSinceLastFetch = now - this.lastOrdersFetchTime;

    if (timeSinceLastFetch >= this.ordersFetchThrottleMs) {
      this.executeOrdersFetch();
    } else {
      this.ordersFetchScheduled = true;
      setTimeout(() => {
        this.ordersFetchScheduled = false;
        if (!this.isDisposed && this.hasPendingOrderEvents) {
          this.executeOrdersFetch();
        }
      }, this.ordersFetchThrottleMs - timeSinceLastFetch);
    }
  };

  private executeOrdersFetch = async () => {
    try {
      const fetchSeq = ++this.ordersFetchRequestSeq;
      this.lastOrdersFetchTime = Date.now();
      this.hasPendingOrderEvents = false;
      const fetchedOrders = await this.parent.fetchOrders();
      if (this.isDisposed || fetchSeq !== this.ordersFetchRequestSeq) {
        return;
      }

      const now = Date.now();
      this.pruneOrderLifecycleCaches(now);
      const currentOrdersById = new Map(this.store.orders.map((order) => [order.id, order]));

      // Bitget REST can lag WS by a few seconds. Prevent stale REST "open"
      // rows from resurrecting orders already terminal in the local WS state.
      const reconciledFetchedOrders = fetchedOrders.filter((order) => {
        const isFetchedOpenLike = order.status === OrderStatus.Open;
        if (isFetchedOpenLike) {
          const tombstoneUntil = this.terminalOrderTombstones.get(order.id);
          if (tombstoneUntil && tombstoneUntil > now) {
            return false;
          }
        }

        const currentOrder = currentOrdersById.get(order.id);
        if (!currentOrder) {
          return true;
        }

        const isCurrentTerminal =
          currentOrder.status === OrderStatus.Canceled ||
          currentOrder.status === OrderStatus.Closed ||
          currentOrder.status === OrderStatus.Canceling;

        if (isCurrentTerminal && isFetchedOpenLike) {
          return false;
        }

        return true;
      });

      // Preserve WS-delivered algo orders that REST doesn't return.
      // Bitget's orders-plan-pending doesn't always return TPSL orders
      // placed via place-tpsl-order (especially sized stops in hedge mode).
      const fetchedIds = new Set(reconciledFetchedOrders.map((o) => o.id));
      // Also preserve recently WS-opened non-algo orders for a short window;
      // Bitget REST can briefly omit just-created limits.
      const preservedOrders = this.store.orders.filter(
        (o) =>
          !fetchedIds.has(o.id) &&
          o.status === OrderStatus.Open &&
          (o.type === OrderType.StopLoss ||
            o.type === OrderType.TakeProfit ||
            o.type === OrderType.TrailingStopLoss ||
            (this.recentWsOpenOrders.get(o.id) ?? 0) > now),
      );

      this.store.addOrUpdateOrdersFull([...reconciledFetchedOrders, ...preservedOrders]);
    } catch {
      // Silent failure, will retry on next event
    }
  };

  private pruneOrderLifecycleCaches = (now: number) => {
    for (const [orderId, expiresAt] of this.terminalOrderTombstones) {
      if (expiresAt <= now) {
        this.terminalOrderTombstones.delete(orderId);
      }
    }
    for (const [orderId, expiresAt] of this.recentWsOpenOrders) {
      if (expiresAt <= now) {
        this.recentWsOpenOrders.delete(orderId);
      }
    }
  };

  private markOrderAsTerminal = (orderId: string) => {
    const now = Date.now();
    this.terminalOrderTombstones.set(orderId, now + this.orderTombstoneTtlMs);
    this.recentWsOpenOrders.delete(orderId);
    if (this.terminalOrderTombstones.size > 2000) {
      this.pruneOrderLifecycleCaches(now);
    }
  };

  private markOrderAsOpenFromWs = (orderId: string) => {
    const now = Date.now();
    this.recentWsOpenOrders.set(orderId, now + this.wsOpenOrderGraceMs);
    if (this.recentWsOpenOrders.size > 2000) {
      this.pruneOrderLifecycleCaches(now);
    }
  };

  private auth = async () => {
    const timestamp = virtualClock.getCurrentTime().unix();

    if (this.parent.options.remoteSigner) {
      const result = await this.parent.options.remoteSigner.sign({
        id: nanoid(),
        type: 'ws-auth',
        exchangeName: 'bitget',
        accountName: this.parent.options.remoteSigner.accountName,
        wsAuth: {
          toSign: `${timestamp}GET/user/verify`,
          outputFormat: 'base64',
          extraData: { timestamp },
        },
      });

      if (result.error) {
        this.emitAndLogError(`Remote WS auth failed: ${result.error}`);
        return;
      }

      this.sendWithLog(
        JSON.stringify({
          op: 'login',
          args: [
            {
              apiKey: this.parent.options.key,
              passphrase: this.parent.options.passphrase,
              timestamp,
              sign: result.wsAuthSignature,
            },
          ],
        }),
      );
      return;
    }

    const sign = createHmac('sha256', this.parent.options.secret)
      .update(`${timestamp}GET/user/verify`)
      .digest('base64');

    const payload = {
      op: 'login',
      args: [
        {
          apiKey: this.parent.options.key,
          passphrase: this.parent.options.passphrase,
          timestamp,
          sign,
        },
      ],
    };

    this.sendWithLog(JSON.stringify(payload));
  };

  private subscribe = () => {
    const payload = {
      op: 'subscribe',
      args: [
        {
          instType: this.parent.apiProductType.toUpperCase(),
          channel: 'account',
          coin: 'default',
        },
        {
          instType: this.parent.apiProductType.toUpperCase(),
          channel: 'positions',
          instId: 'default',
        },
        {
          instType: this.parent.apiProductType.toUpperCase(),
          channel: 'orders',
          instId: 'default',
        },
        {
          instType: this.parent.apiProductType.toUpperCase(),
          channel: 'orders-algo',
          instId: 'default',
        },
      ],
    };

    this.sendWithLog(JSON.stringify(payload));
  };
}
