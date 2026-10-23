import type { Position } from '../../types';
import type { BitgetExchange } from './bitget.exchange';

import createHmac from 'create-hmac';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET } from '../../config/config';
import { resolveTransportRouting } from '../../transport/registry';
import { ReconnectingWebSocket } from '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket';
import { WsDirectFallback } from '../../utils/wsDirectFallback';
import { LogSeverity, OrderStatus, OrderType } from '../../types';
import { subtract } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { BASE_WS_PRIVATE_URL, BASE_WS_PRIVATE_UTA_URL, ORDER_STATUS } from './bitget.types';

/** Login refusals about where the connection comes from: 40018 key bound to other IPs, 40024 restricted location. */
const BITGET_EGRESS_REFUSAL_CODES = new Set(['40018', '40024']);

export class BitgetPrivateWebsocket extends BaseWebSocket<BitgetExchange> {
  protected directFallback = new WsDirectFallback();
  /** Tunnel sockets that opened but closed before a successful login, in a row. */
  private unauthenticatedTunnelCloses = 0;
  private readonly MAX_UNAUTHENTICATED_TUNNEL_CLOSES = 2;
  /** Tunnel connects that failed before opening, in a row: an outage also does this, so the bar is higher. */
  private failedTunnelConnects = 0;
  private readonly MAX_FAILED_TUNNEL_CONNECTS = 6;
  private openedThisConnection = false;
  /** The venue answered this connection's login; its close says nothing about the route. */
  private loginAnsweredThisConnection = false;
  private routeHasTealstreetEgress = false;
  private lastPrivateStreamRecoveryAt = 0;
  private readonly privateStreamRecoveryCooldownMs = 30_000;
  private loggedIn = false;
  private hasReportedErrorEvent = false;
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
  private lastWsPositionSnapshotAt = 0;
  private lastWsPositionSnapshotIds = new Set<string>();
  private wsPositionUpdatedAtById = new Map<string, number>();
  private readonly positionRestStaleGuardMs = 5_000;

  private normalizeOrderStatus = (status: unknown): string =>
    String(status ?? '')
      .toLowerCase()
      .replace(/-/g, '_');

  private getRawOrderStatus = (order: Record<string, any>): string =>
    this.normalizeOrderStatus(order.status ?? order.state ?? order.planStatus ?? order.orderStatus);

  private normalizeChannel = (channel: unknown): string =>
    String(channel ?? '')
      .toLowerCase()
      .replace(/[-_]/g, '');

  /**
   * The shared `ORDER_STATUS` map treats several strings as `OrderStatus.Closed`,
   * including `success` and `executed`. Those two are ambiguous on Bitget WS —
   * they appear on non-fill acknowledgements (successful cancel/modify frames)
   * for some topics — and firing `emit('fill', ...)` plus `throttledFetchPositions`
   * on them causes phantom fills and unnecessary REST reconciliation storms.
   * Only the explicit fill statuses trigger fill emission and position/balance
   * refetches; other closed statuses still flow through `updateOrderStatus` +
   * `markOrderAsTerminal` so the local order row transitions correctly.
   */
  private static readonly BITGET_TERMINAL_FILL_STATUSES: ReadonlySet<string> = new Set([
    'triggered',
    'filled',
    'full_fill',
  ]);

  private isTerminalFillStatus = (normalizedStatus: string): boolean =>
    BitgetPrivateWebsocket.BITGET_TERMINAL_FILL_STATUSES.has(normalizedStatus);

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

  private getPositionPayloadUpdateTime = (position: Position): number | undefined => {
    const raw =
      position.info?.uTime ??
      position.info?.updatedTime ??
      position.info?.utime ??
      position.info?.cTime ??
      position.info?.createdTime;
    const parsed = this.toFiniteNumber(raw, NaN);
    return Number.isFinite(parsed) ? parsed : undefined;
  };

  private markWsPositions = (positions: Position[], action?: string) => {
    const now = virtualClock.getCurrentTimeMs();
    if (action === 'snapshot') {
      this.lastWsPositionSnapshotAt = now;
      this.lastWsPositionSnapshotIds = new Set(positions.map((position) => position.id));
    }

    for (const position of positions) {
      this.wsPositionUpdatedAtById.set(position.id, now);
    }
  };

  private mergeRestPositionsWithFreshWsState = (restPositions: Position[], requestStartedAt: number): Position[] => {
    const currentById = new Map(this.store.positions.map((position) => [position.id, position]));
    const merged: Position[] = [];
    const usedIds = new Set<string>();
    const snapshotIsFresh =
      this.lastWsPositionSnapshotAt > 0 &&
      requestStartedAt - this.lastWsPositionSnapshotAt <= this.positionRestStaleGuardMs;

    for (const restPosition of restPositions) {
      if (snapshotIsFresh && !this.lastWsPositionSnapshotIds.has(restPosition.id)) {
        continue;
      }

      const currentPosition = currentById.get(restPosition.id);
      const wsUpdatedAt = this.wsPositionUpdatedAtById.get(restPosition.id) ?? 0;
      const wsIsFresh = wsUpdatedAt > 0 && requestStartedAt - wsUpdatedAt <= this.positionRestStaleGuardMs;

      if (currentPosition && wsIsFresh) {
        const currentUpdateTime = this.getPositionPayloadUpdateTime(currentPosition);
        const restUpdateTime = this.getPositionPayloadUpdateTime(restPosition);

        if (!restUpdateTime || (currentUpdateTime && currentUpdateTime > restUpdateTime)) {
          merged.push(currentPosition);
          usedIds.add(currentPosition.id);
          continue;
        }
      }

      merged.push(restPosition);
      usedIds.add(restPosition.id);
    }

    for (const currentPosition of this.store.positions) {
      if (usedIds.has(currentPosition.id)) continue;

      const wsUpdatedAt = this.wsPositionUpdatedAtById.get(currentPosition.id) ?? 0;
      if (wsUpdatedAt > 0 && requestStartedAt - wsUpdatedAt <= this.positionRestStaleGuardMs) {
        merged.push(currentPosition);
        usedIds.add(currentPosition.id);
      }
    }

    return merged;
  };

  connectAndSubscribe = () => {
    if (!this.isDisposed) {
      const useUtaRouting = this.parent.usesUtaRouting;
      const routing = resolveTransportRouting(this.parent.name, this.parent.options);
      // A UTA account never takes the v2 route; without a declared UTA route it dials v3 direct.
      const route = (useUtaRouting ? routing.wsPrivateUta : routing.wsPrivate) ?? { tunnelled: false };
      const directUrl = useUtaRouting ? BASE_WS_PRIVATE_UTA_URL : BASE_WS_PRIVATE_URL;
      const primary = route.tunnelled && route.host ? route.host : directUrl;
      // The relay is not gated on `isWsRelayEnabled`: Bitget checks the key's bound
      // IP on login (40018), and the relay is the egress a user binds for REST.
      const url = this.directFallback.resolveUrl(
        primary,
        route.relay,
        route.fallback !== primary ? route.fallback : undefined,
      );
      if (url !== directUrl) {
        try {
          this.parent.emitter.emit(
            'grafana-info',
            `Using tealstreet bitget private ${url === route.relay ? 'relay' : 'proxy'}`,
          );
        } catch (_e) {}
      }
      this.routeHasTealstreetEgress = route.tunnelled || !!route.relay || !!route.fallback;
      this.ws = this.createWebsocket(url);
      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);
      this.ws.addEventListener('close', this.onTunnelClose);
    }
  };

  /**
   * A tunnel socket (ws-proxy or relay) reconnects inside ReconnectingWebSocket, so
   * `connectAndSubscribe` — and the route choice — never runs again on its own. A
   * route that keeps closing before login is unusable: move to the next one.
   */
  private onTunnelClose = () => {
    const wasLoggedIn = this.loggedIn;
    const opened = this.openedThisConnection;
    const loginAnswered = this.loginAnsweredThisConnection;
    this.loggedIn = false;
    this.openedThisConnection = false;
    this.loginAnsweredThisConnection = false;
    if (this.isDisposed || !(this.ws instanceof ReconnectingWebSocket)) return;
    if (wasLoggedIn) {
      this.unauthenticatedTunnelCloses = 0;
      this.failedTunnelConnects = 0;
      return;
    }
    if (opened) {
      this.failedTunnelConnects = 0;
      // A login Bitget rejected (bad key, skew) reached the venue: the route works.
      if (loginAnswered) return;
      if (++this.unauthenticatedTunnelCloses < this.MAX_UNAUTHENTICATED_TUNNEL_CLOSES) return;
    } else if (++this.failedTunnelConnects < this.MAX_FAILED_TUNNEL_CONNECTS) {
      return;
    }
    if (this.directFallback.rejectCurrent()) {
      this.parent.log('Bitget private WS route keeps closing before login; trying the next route', LogSeverity.Warning);
      this.switchRoute();
    } else {
      this.unauthenticatedTunnelCloses = 0;
      this.failedTunnelConnects = 0;
    }
  };

  /** Replace the socket on the route `directFallback` now points at. */
  private switchRoute = () => {
    if (this.isDisposed) return;
    if (!(this.ws instanceof ReconnectingWebSocket)) {
      // A native socket's own close path reconnects through connectAndSubscribe.
      this.reconnect();
      return;
    }
    if (this.pingTimeoutId) {
      clearTimeout(this.pingTimeoutId);
      this.pingTimeoutId = undefined;
    }
    if (this.pongMissTimeoutId) {
      clearTimeout(this.pongMissTimeoutId);
      this.pongMissTimeoutId = undefined;
    }
    const old = this.ws;
    old.removeEventListener('open', this.onOpen);
    old.removeEventListener('message', this.onMessage);
    old.removeEventListener('close', this.onClose);
    old.removeEventListener('close', this.onTunnelClose);
    old.close();
    this.ws = undefined;
    this.loggedIn = false;
    this.openedThisConnection = false;
    this.loginAnsweredThisConnection = false;
    this.unauthenticatedTunnelCloses = 0;
    this.failedTunnelConnects = 0;
    this.connectAndSubscribe();
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.loggedIn = false;
      this.openedThisConnection = true;
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

  recoverMissingPrivateStream = (reason: string) => {
    if (this.isDisposed) {
      return;
    }

    const now = Date.now();
    if (now - this.lastPrivateStreamRecoveryAt < this.privateStreamRecoveryCooldownMs) {
      return;
    }
    this.lastPrivateStreamRecoveryAt = now;

    // Route changes are driven by evidence about the route (40018, closes before
    // login, silent connects); a quiet stream after a write only reconnects.
    this.parent.log(
      `Bitget private WS received no authenticated stream payloads after a write; reconnecting current route. Reason: ${reason}`,
      LogSeverity.Warning,
    );

    this.reconnect();
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();

      this.emitWsDiagnosticReceive(data);

      if (data === 'pong') {
        this.handlePongEvent();
        return;
      }

    }
    this.directFallback.onMessage();

    const json = jsonParse<Record<string, any>>(data);
    if (!json) return;

    const channel = this.normalizeChannel(json?.arg?.channel);

    if (json?.event === 'login') {
      if (`${json.code}` === '0') {
        this.loggedIn = true;
        this.unauthenticatedTunnelCloses = 0;
        this.hasReportedErrorEvent = false;
        this.subscribe();
      } else if (json.msg || json.code) {
        this.emitAndLogError(`Bitget private WS login failed: ${json.msg || json.code}`);
      }
      return;
    }

    // Bitget reports a failed login (and a refused subscribe) as `event: 'error'`,
    // never `event: 'login'`. Unhandled, the socket kept answering pings with no
    // subscriptions: healthy by every liveness check, and silent.
    if (json?.event === 'error') {
      this.handleErrorEvent(json);
      return;
    }

    if (json?.arg?.topic === 'order' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('order');
      this.handleUtaOrderTopic(json);
      return;
    }

    if (json?.arg?.topic === 'position' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('position');
      this.handleUtaPositionsTopic(json);
      return;
    }

    if (json?.arg?.topic === 'account' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('account');
      this.handleUtaAccountTopic(json);
      return;
    }

    if (channel === 'orders' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('order');
      this.handleOrderTopic(json);
      return;
    }

    if (channel === 'ordersalgo' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('order-algo');
      this.handleAlgoOrdersTopic(json);
      return;
    }

    if (channel === 'positions' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('position');
      this.handlePositionsTopic(json);
      return;
    }

    if (channel === 'account' && json?.event !== 'subscribe') {
      this.recordMessageHandlerStats('account');
      this.handleAccountTopic(json);
      return;
    }
  };

  private handleErrorEvent = (json: Record<string, any>) => {
    const failedLogin = !this.loggedIn;
    if (failedLogin) this.loginAnsweredThisConnection = true;
    // 40018: the key is bound to other IPs. Try the next egress before reporting;
    // report (and stop reconnecting) only once every route has been refused.
    if (failedLogin && BITGET_EGRESS_REFUSAL_CODES.has(`${json.code}`)) {
      if (this.directFallback.rejectCurrent()) {
        this.parent.log(
          'Bitget private WS login refused this egress IP (40018); trying the next route',
          LogSeverity.Warning,
        );
        this.switchRoute();
        return;
      }
      if (!this.hasReportedErrorEvent) {
        this.hasReportedErrorEvent = true;
        const detail = [json.code, json.msg].filter((part) => part !== undefined && part !== '').join(' ');
        const scope = this.routeHasTealstreetEgress ? "none of Tealstreet's WebSocket egress IPs" : 'this IP';
        this.emitAndLogError(`Bitget private WS login rejected: ${detail} — the API key's IP whitelist allows ${scope}`);
      }
      return;
    }
    // Once per streak: a rejected login repeats on every reconnect.
    if (!this.hasReportedErrorEvent) {
      this.hasReportedErrorEvent = true;
      const detail = [json.code, json.msg].filter((part) => part !== undefined && part !== '').join(' ');
      this.emitAndLogError(`Bitget private WS ${failedLogin ? 'login' : 'subscription'} rejected: ${detail}`);
    }
    if (failedLogin) {
      this.reconnect();
    }
  };

  handleOrderTopic = (data: Record<string, any>) => {
    this.parent.markPrivateStreamMessageReceived();
    const rawOrders = data.data;

    for (const rawOrder of rawOrders) {
      const normalizedStatus = this.getRawOrderStatus(rawOrder);
      const mappedStatus = ORDER_STATUS[normalizedStatus];
      const isPartialFill = normalizedStatus === 'partial_fill' || normalizedStatus === 'partially_filled';
      // Only explicit fill statuses count as fills for emission and reconciliation.
      // `success` / `executed` may map to Closed but are frequently non-fill acks.
      const isFullFill = this.isTerminalFillStatus(normalizedStatus);

      const orders = this.parent.mapOrder({
        ...rawOrder,
        status: normalizedStatus || rawOrder.status,
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
          amount: parseFloat(rawOrder.baseVolume || rawOrder.accBaseVolume || 0),
        });
        this.throttledFetchBalance();
      }

      if (isFullFill) {
        // Closing/filling events can leave available margin stale in WS account payloads.
        // Kick off fast REST reconciliations for both positions and balance.
        this.throttledFetchPositions();
        this.throttledFetchBalance();
      }

      if (mappedStatus === OrderStatus.Canceled) {
        orders.forEach((order) => {
          this.store.updateOrderStatus(order, OrderStatus.Canceled);
          this.markOrderAsTerminal(order.id);
        });
      }

      if (mappedStatus === OrderStatus.Closed) {
        // Terminal Closed transition applies to both explicit fills and
        // ambiguous closed statuses (e.g. `success` / `executed`); only the
        // explicit fill statuses above emit fills and refetch.
        orders.forEach((order) => {
          this.store.updateOrderStatus(order, OrderStatus.Closed);
          this.markOrderAsTerminal(order.id);
        });
      }

      if (mappedStatus === OrderStatus.Open) {
        orders.forEach((order) => {
          this.store.addOrUpdateOrder(order);
          this.markOrderAsOpenFromWs(order.id);
        });
      }
    }
    this.hasPendingOrderEvents = true;
    this.throttledFetchOrders();
  };

  handleUtaOrderTopic = (data: Record<string, any>) => {
    const rawOrders = Array.isArray(data?.data) ? data.data : [];
    this.handleOrderTopic({
      ...data,
      data: rawOrders.map((rawOrder: Record<string, any>) => ({
        ...rawOrder,
        status: rawOrder.orderStatus,
        baseVolume: rawOrder.baseVolume ?? rawOrder.cumExecQty,
        priceAvg: rawOrder.avgPrice,
      })),
    });
  };

  handleAlgoOrdersTopic = (data: Record<string, any>) => {
    this.parent.markPrivateStreamMessageReceived();
    const rawOrders = data.data;

    for (const rawOrder of rawOrders) {
      const normalizedStatus = this.getRawOrderStatus(rawOrder);
      const mappedStatus = ORDER_STATUS[normalizedStatus];
      const orders = this.parent.mapOrder({
        ...rawOrder,
        status: normalizedStatus || rawOrder.status,
        // invert side for hedge mode because bitget developers are ...
        side: rawOrder.posMode === 'hedge_mode' ? (rawOrder.side === 'sell' ? 'buy' : 'sell') : rawOrder.side,
      });
      const parentOrder = orders[0];

      const isTerminalFill = this.isTerminalFillStatus(normalizedStatus);

      if (mappedStatus === OrderStatus.Open) {
        this.store.addOrUpdateOrder(parentOrder);
        this.markOrderAsOpenFromWs(parentOrder.id);
      } else if (mappedStatus === OrderStatus.Canceled) {
        this.store.updateOrderStatus(parentOrder, OrderStatus.Canceled);
        this.markOrderAsTerminal(parentOrder.id);
      } else if (mappedStatus === OrderStatus.Closed) {
        this.store.updateOrderStatus(parentOrder, OrderStatus.Closed);
        this.markOrderAsTerminal(parentOrder.id);
      }

      // Only emit a fill and reconcile positions/balance when the algo order
      // reached an explicit fill status; ambiguous Closed statuses transition
      // the store above but do not trigger side effects.
      if (isTerminalFill && rawOrder.actualSize) {
        this.emitter.emit('fill', {
          id: parentOrder.id,
          side: parentOrder.side,
          symbol: parentOrder.symbol,
          price: parseFloat(rawOrder.triggerPrice),
          amount: parseFloat(rawOrder.actualSize),
        });
        this.throttledFetchBalance();
      }

      if (isTerminalFill) {
        this.throttledFetchPositions();
        this.throttledFetchBalance();
      }
    }

    this.hasPendingOrderEvents = true;
    this.throttledFetchOrders();
  };

  handlePositionsTopic = (data: Record<string, any>) => {
    this.parent.markPrivateStreamMessageReceived();
    const action = typeof data?.action === 'string' ? data.action : undefined;
    const rawList = Array.isArray(data?.data) ? (data.data as Array<Record<string, any>>) : [];
    const validItems = rawList.filter((p) => p && typeof p === 'object');

    if (validItems.length === 0) {
      // Bitget can send an empty snapshot when there are no open positions.
      // Apply immediately so UI does not wait on periodic tick reconciliation.
      if (action === 'snapshot') {
        this.markWsPositions([], action);
        this.store.addOrUpdatePositionsFull([]);
      }
      this.hasPendingPositionEvents = true;
      this.throttledFetchPositions();
      this.throttledFetchBalance();
      return;
    }

    const positions = validItems.map((rawPosition) => this.parent.mapPosition(rawPosition));
    this.markWsPositions(positions, action);

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

  handleUtaPositionsTopic = (data: Record<string, any>) => {
    this.parent.markPrivateStreamMessageReceived();
    const action = typeof data?.action === 'string' ? data.action : undefined;
    const rawList = Array.isArray(data?.data) ? (data.data as Array<Record<string, any>>) : [];
    const validItems = rawList.filter((p) => p && typeof p === 'object');

    if (validItems.length === 0) {
      if (action === 'snapshot') {
        this.markWsPositions([], action);
        this.store.addOrUpdatePositionsFull([]);
      }
      this.hasPendingPositionEvents = true;
      this.throttledFetchPositions();
      this.throttledFetchBalance();
      return;
    }

    const positions = validItems.map((rawPosition) => this.parent.mapUtaPosition(rawPosition));
    this.markWsPositions(positions, action);
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
    this.parent.markPrivateStreamMessageReceived();
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

  handleUtaAccountTopic = (data: Record<string, any>) => {
    this.parent.markPrivateStreamMessageReceived();
    const raw = Array.isArray(data?.data) ? data.data[0] : data?.data;
    if (!raw || typeof raw !== 'object') {
      this.throttledFetchBalance();
      return;
    }

    const accountEquity = this.toFiniteNumber(raw.accountEquity, NaN);
    const available = this.toFiniteNumber(raw.effEquity, NaN);
    const apiUpnl = this.toFiniteNumber(raw.unrealisedPnl, this.toFiniteNumber(raw.usdtUnrealisedPnl));

    if (isNaN(accountEquity) || isNaN(available)) {
      this.throttledFetchBalance();
      return;
    }

    const upnl = this.parent.calculateAccountUpnl(apiUpnl);
    this.store.update({
      balance: {
        used: subtract(accountEquity, available),
        free: available,
        total: subtract(accountEquity, upnl),
        upnl,
        info: raw,
      },
    });
    this.hasPendingBalanceEvents = true;
    this.throttledFetchBalance();
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
      const requestStartedAt = virtualClock.getCurrentTimeMs();
      this.hasPendingPositionEvents = false;
      const positions = await this.parent.fetchPositions();
      if (!this.isDisposed) {
        this.store.addOrUpdatePositionsFull(this.mergeRestPositionsWithFreshWsState(positions, requestStartedAt));
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
    if (this.parent.usesUtaRouting) {
      this.sendWithLog(
        JSON.stringify({
          op: 'subscribe',
          args: [
            {
              instType: 'UTA',
              topic: 'account',
            },
            {
              instType: 'UTA',
              topic: 'position',
            },
            {
              instType: 'UTA',
              topic: 'order',
            },
          ],
        }),
      );
      return;
    }

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
