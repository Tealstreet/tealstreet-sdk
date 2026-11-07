import type { LighterExchange } from './lighter.exchange';
import type { LighterPrivateCloseClassification } from './lighter.ws-policy';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { ENABLE_RECONNECTING_WEBSOCKET, WS_MIN_RECONNECTION_DELAY } from '../../../config/config';
import { resolveTransportRouting } from '../../../transport/registry';
import { LogSeverity, OrderStatus, Position, PositionSide } from '../../../types';
import { v } from '../../../utils/get-key';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseWebSocket } from '../../../exchanges/base.ws';
import {
  calculateLighterPrivateReconnectDelay,
  classifyLighterPrivateClose,
  LIGHTER_POLICY_REASON_RE,
} from './lighter.ws-policy';

const LIGHTER_GEORESTRICTED_DEGRADED_CONNECTIVITY_MESSAGE =
  'Lighter georestrictions detected; connectivity status is degraded. Private updates may rely on read-only/REST fallback from this IP. Use a VPN or Tealstreet WS proxy; account tier changes will not fix this.';

type WsAccountMessage = {
  channel: string;
  type: 'snapshot' | 'update';
  data: {
    account?: any;
    orders?: ReadonlyArray<{
      orderId: string;
      clientOrderId?: string;
      marketId: string;
      symbol: string;
      side: 'BUY' | 'SELL';
      isBuy: boolean;
      type: string;
      status: string;
      price: string;
      amount: string;
      size: string;
      filledAmount?: string;
      filledSize?: string;
      remainingAmount?: string;
      averagePrice?: string;
      fee?: string;
      postOnly: boolean;
      reduceOnly: boolean;
      createdAt: number;
      updatedAt: number;
      expiry?: number;
      cancelReason?: string;
    }>;
    positions?: ReadonlyArray<{
      marketId: string;
      symbol: string;
      side: 'LONG' | 'SHORT';
      size: string;
      entryPrice: string;
      avgEntryPrice?: string;
      markPrice?: string;
      liquidationPrice: string;
      unrealizedPnl: string;
      realizedPnl: string;
      leverage: string;
      margin: string;
      updatedAt: number;
    }>;
    fills?: ReadonlyArray<{
      fillId: string;
      orderId: string;
      tradeId: string;
      symbol: string;
      side: 'BUY' | 'SELL';
      price: string;
      size: string;
      fee: string;
      isMaker: boolean;
      timestamp: number;
    }>;
    balances?: any;
  };
  timestamp: number;
};

type ThrottleTimeoutKey = 'ordersThrottleTimeout' | 'positionsThrottleTimeout' | 'balanceThrottleTimeout';

export class LighterPrivateWebsocket extends BaseWebSocket<LighterExchange> {
  // Runs its own close loop; online recovery falls back to an account reload.
  protected readonly recoversAfterOffline = false;

  // Lighter's pong echoes are unreliable — any inbound traffic (account,
  // position, order-book updates, server pings) is sufficient proof of
  // liveness. The existing `shouldEnforcePongMissReconnect = false` already
  // prevents arming the miss timer on our own pong events; this flag also
  // clears any prior-armed miss timer on any inbound message for extra
  // defense-in-depth.
  protected override treatAnyMessageAsPong = true;
  protected shouldEnforcePongMissReconnect = (): boolean => false;
  private static readonly READONLY_FALLBACK_WARNING_COOLDOWN_MS = 30_000;
  private static readonly readonlyFallbackWarningAtByAccount = new Map<string, number>();
  private static readonly STANDARD_PING_INTERVAL_MS = 10_000;
  private static readonly DEGRADED_PING_INTERVAL_MS = 20_000;
  private static readonly STANDARD_IDLE_PING_THRESHOLD_MS = 20_000;
  private static readonly DEGRADED_IDLE_PING_THRESHOLD_MS = 45_000;
  private static readonly STANDARD_IDLE_RECONNECT_THRESHOLD_MS = 180_000;
  private static readonly DEGRADED_IDLE_RECONNECT_THRESHOLD_MS = 180_000;
  private static readonly RATE_LIMIT_RECONNECT_DELAY_MS = 30_000;
  private static readonly RATE_LIMIT_COOLDOWN_MS = 60_000;
  private static readonly RATE_LIMIT_FORCE_TOAST_AFTER_MS = LighterPrivateWebsocket.RATE_LIMIT_COOLDOWN_MS;
  private static readonly RATE_LIMIT_RESUBSCRIBE_BUFFER_MS = 2_000;
  private static readonly RATE_LIMIT_PING_INTERVAL_MS = 20_000;
  private static readonly RATE_LIMIT_PING_SKIP_LOG_COOLDOWN_MS = 15_000;
  private static readonly PRIVATE_STREAM_WARNING_AFTER_MS = 90_000;
  private static readonly PRIVATE_STREAM_WARNING_COOLDOWN_MS = 60_000;
  private static readonly NO_STREAM_RECOVERY_AFTER_MS = 180_000;
  private static readonly STALE_STREAM_RECOVERY_AFTER_MS = 120_000;
  private static readonly RECOVERY_ATTEMPT_COOLDOWN_MS = 180_000;
  private static readonly RECOVERY_RECONNECT_GRACE_MS = 20_000;
  private static readonly EARLY_CLOSE_WINDOW_MS = 10_000;

  lastConnectTime = 0;
  ordersThrottleTimeout: NodeJS.Timeout | undefined;
  positionsThrottleTimeout: NodeJS.Timeout | undefined;
  balanceThrottleTimeout: NodeJS.Timeout | undefined;
  diagnosticsInterval: NodeJS.Timeout | undefined;
  private openedSinceLastConnect = false;
  private useReadonlyFallback = false;
  private failedHandshakes = 0;
  private hasWarnedReadonlyFallback = false;
  private hasWarnedNoAccountStream = false;
  private hasWarnedNoAccountStreamToast = false;
  private hasWarnedProxyStreamErrorToast = false;
  private hasWarnedOrdersChannelUnsupported = false;
  private lastStaleAccountStreamWarningAt = 0;
  private accountStreamMessageCount = 0;
  private lastAccountStreamMessageAt = 0;
  private accountAllOrdersChannelEnabled = true;
  private hasPendingAccountAllOrdersSubscribe = false;
  private hasReceivedAccountAllOrdersAck = false;
  private wsRateLimitedUntil = 0;
  private lastStreamRecoveryAttemptAt = 0;
  private readonlyFallbackFailures = 0;
  private streamRecoveryReconnectTimeout: NodeJS.Timeout | undefined;
  private rateLimitResubscribeTimeout: NodeJS.Timeout | undefined;
  private privateStreamRateLimitFirstSeenAt = 0;
  private privateStreamRateLimitErrorCount = 0;
  private lastRateLimitedPingAt = 0;
  private lastRateLimitedPingSkipLogAt = 0;
  private lastIdlePingAt = 0;
  private readonly readonlyFallbackWarningMessage = LIGHTER_GEORESTRICTED_DEGRADED_CONNECTIVITY_MESSAGE;

  private getRateLimitWarningMessage = (): string => this.parent.getAccountRateLimitGuidanceMessage();

  private getReadonlyFallbackWarningKey = (): string => {
    const environment = this.parent.lighterDeploymentProfile.network;
    const accountIndex = this.parent.getPrivateWsAccountIndex();
    return `${environment}:${accountIndex}`;
  };

  private emitReadonlyFallbackWarning = (logMessage = false) => {
    const now = virtualClock.getCurrentTimeMs();
    const warningKey = this.getReadonlyFallbackWarningKey();
    const lastWarningAt = LighterPrivateWebsocket.readonlyFallbackWarningAtByAccount.get(warningKey) || 0;
    if (now - lastWarningAt < LighterPrivateWebsocket.READONLY_FALLBACK_WARNING_COOLDOWN_MS) {
      return;
    }
    LighterPrivateWebsocket.readonlyFallbackWarningAtByAccount.set(warningKey, now);
    if (logMessage) {
      this.parent.log(this.readonlyFallbackWarningMessage, LogSeverity.Warning);
    }
    this.parent.emitter.emit('warning', this.readonlyFallbackWarningMessage, { forceToast: true });
  };

  isReadonlyFallbackActive = (): boolean => this.useReadonlyFallback;
  private shouldUseRestOrderReconciliation = (): boolean =>
    !this.hasReceivedAccountAllOrdersAck || this.useReadonlyFallback;

  // A transport that resolves no host (the direct default) means there is no route to use.
  private isWsProxyRouteEnabled = (): boolean =>
    this.parent.lighterDeploymentProfile.supportsWsProxy &&
    (this.parent.options.tealstreetWsRoute ?? false) &&
    !(this.parent.options.testnet ?? false) &&
    !!resolveTransportRouting(this.parent.name, this.parent.options).wsPrivate.host;

  private shouldUseGeorestrictedConnectivityWarning = (isPolicyRestrictionError = false): boolean =>
    !this.isWsProxyRouteEnabled() &&
    (this.useReadonlyFallback || this.readonlyFallbackFailures > 0 || isPolicyRestrictionError);

  private getPrivateStreamErrorWarningMessage = (params: {
    errMsg: string;
    isRateLimitError: boolean;
    isPolicyRestrictionError: boolean;
  }): string => {
    if (
      params.isPolicyRestrictionError ||
      (params.isRateLimitError && this.shouldUseGeorestrictedConnectivityWarning())
    ) {
      return LIGHTER_GEORESTRICTED_DEGRADED_CONNECTIVITY_MESSAGE;
    }

    if (params.isRateLimitError) {
      return this.getRateLimitWarningMessage();
    }

    return this.isWsProxyRouteEnabled()
      ? `Lighter private WS route returned an error: ${params.errMsg}. Falling back to REST reconciliation for private updates.`
      : `Lighter private websocket returned an error: ${params.errMsg}. Falling back to REST reconciliation for private updates.`;
  };

  private getPingIntervalMs = (): number =>
    this.useReadonlyFallback
      ? LighterPrivateWebsocket.DEGRADED_PING_INTERVAL_MS
      : LighterPrivateWebsocket.STANDARD_PING_INTERVAL_MS;

  private getIdlePingThresholdMs = (): number =>
    this.useReadonlyFallback
      ? LighterPrivateWebsocket.DEGRADED_IDLE_PING_THRESHOLD_MS
      : LighterPrivateWebsocket.STANDARD_IDLE_PING_THRESHOLD_MS;

  private getIdleReconnectThresholdMs = (): number =>
    this.useReadonlyFallback
      ? LighterPrivateWebsocket.DEGRADED_IDLE_RECONNECT_THRESHOLD_MS
      : LighterPrivateWebsocket.STANDARD_IDLE_RECONNECT_THRESHOLD_MS;

  getWatchdogStaleThresholdMs = (): number => this.getIdleReconnectThresholdMs();

  private isWsRateLimited = (): boolean => virtualClock.getCurrentTimeMs() < this.wsRateLimitedUntil;

  private notePrivateStreamRateLimitError = (now: number): void => {
    if (this.privateStreamRateLimitErrorCount === 0) {
      this.privateStreamRateLimitFirstSeenAt = now;
    }
    this.privateStreamRateLimitErrorCount += 1;
  };

  private clearPrivateStreamRateLimitErrorState = (): void => {
    this.privateStreamRateLimitFirstSeenAt = 0;
    this.privateStreamRateLimitErrorCount = 0;
  };

  private shouldForceToastPrivateStreamRateLimit = (now: number): boolean =>
    this.privateStreamRateLimitErrorCount > 1 &&
    this.privateStreamRateLimitFirstSeenAt > 0 &&
    now - this.privateStreamRateLimitFirstSeenAt >= LighterPrivateWebsocket.RATE_LIMIT_FORCE_TOAST_AFTER_MS;

  checkLastReceived = () => {
    const now = Date.now();
    const diff = now - this.lastReceived;
    const pingThresholdMs = this.getIdlePingThresholdMs();
    const reconnectThresholdMs = this.getIdleReconnectThresholdMs();
    const lastReceivedTime = new Date(this.lastReceived).toISOString();

    if (diff > reconnectThresholdMs) {
      this.parent.log(
        `[Lighter][PWS] No private stream messages for ${diff}ms (last at ${lastReceivedTime}); reconnecting.`,
        LogSeverity.Warning,
      );
      clearTimeout(this.lastReceivedTimer);
      this.lastReceived = 0;
      this.pingSent = false;
      this.lastIdlePingAt = 0;
      this.reconnect();
      return;
    }

    if (diff > pingThresholdMs) {
      const pingRetryIntervalMs = this.getPingIntervalMs();
      const shouldRetryPing = !this.pingSent || now - this.lastIdlePingAt >= pingRetryIntervalMs;
      if (shouldRetryPing) {
        this.parent.log(
          `[Lighter][PWS] Private stream idle for ${diff}ms (last at ${lastReceivedTime}); sending ping.`,
          LogSeverity.Info,
        );
        try {
          const previousPingAt = this.pingAt;
          this.ping();
          const didSendPing = this.pingAt !== previousPingAt;
          if (didSendPing) {
            this.pingSent = true;
            this.lastIdlePingAt = now;
          }
        } catch (_e) {}
      }
      return;
    }

    if (diff <= pingThresholdMs) {
      this.pingSent = false;
      this.lastIdlePingAt = 0;
    }
  };

  private beginDiagnosticsLoop = () => {
    if (this.diagnosticsInterval) {
      clearInterval(this.diagnosticsInterval);
    }
    this.hasWarnedNoAccountStream = false;
    this.hasWarnedNoAccountStreamToast = false;
    this.hasWarnedProxyStreamErrorToast = false;
    this.lastStaleAccountStreamWarningAt = 0;
    this.accountStreamMessageCount = 0;
    this.lastAccountStreamMessageAt = 0;

    this.diagnosticsInterval = setInterval(() => {
      if (this.isDisposed || !this.isConnected) {
        return;
      }

      const now = virtualClock.getCurrentTimeMs();
      const timeSinceConnect = now - this.lastConnectTime;

      // If we're connected but never receiving account stream messages, order updates will look REST-only.
      if (
        this.accountStreamMessageCount === 0 &&
        timeSinceConnect > LighterPrivateWebsocket.PRIVATE_STREAM_WARNING_AFTER_MS &&
        !this.hasWarnedNoAccountStream &&
        !this.isWsRateLimited()
      ) {
        this.hasWarnedNoAccountStream = true;
        const noAccountStreamMessage = `[Lighter][PWS] Connected but no private account stream messages after ${Math.round(timeSinceConnect / 1000)}s. wsRoute=${this.isWsProxyRouteEnabled()} readonlyFallback=${this.useReadonlyFallback}.`;
        this.parent.log(noAccountStreamMessage, LogSeverity.Warning);
        if (!this.hasWarnedNoAccountStreamToast) {
          this.hasWarnedNoAccountStreamToast = true;
          this.parent.emitter.emit(
            'warning',
            this.isWsProxyRouteEnabled()
              ? 'Lighter private WS route is connected but not receiving account updates. Proxy infra may be unavailable; private updates may rely on REST reconciliation.'
              : 'Lighter private websocket is connected but not receiving account updates. Private updates may rely on REST reconciliation in this environment.',
            { forceToast: true },
          );
        }
      }

      // Warn if account stream goes stale after previously receiving data.
      if (this.lastAccountStreamMessageAt > 0) {
        const streamAgeMs = now - this.lastAccountStreamMessageAt;
        const shouldWarnStale =
          streamAgeMs > LighterPrivateWebsocket.PRIVATE_STREAM_WARNING_AFTER_MS &&
          (this.lastStaleAccountStreamWarningAt === 0 ||
            now - this.lastStaleAccountStreamWarningAt > LighterPrivateWebsocket.PRIVATE_STREAM_WARNING_COOLDOWN_MS);
        if (shouldWarnStale) {
          this.lastStaleAccountStreamWarningAt = now;
          this.parent.log(
            `[Lighter][PWS] Private account stream stale (${Math.round(streamAgeMs / 1000)}s since last message).`,
            LogSeverity.Warning,
          );
        }
      }

      this.maybeRecoverPrivateStream();
    }, 5_000);
  };

  private stopDiagnosticsLoop = () => {
    if (this.diagnosticsInterval) {
      clearInterval(this.diagnosticsInterval);
      this.diagnosticsInterval = undefined;
    }
  };

  private clearStreamRecoveryReconnectTimeout = () => {
    if (this.streamRecoveryReconnectTimeout) {
      clearTimeout(this.streamRecoveryReconnectTimeout);
      this.streamRecoveryReconnectTimeout = undefined;
    }
  };

  private clearRateLimitResubscribeTimeout = () => {
    if (this.rateLimitResubscribeTimeout) {
      clearTimeout(this.rateLimitResubscribeTimeout);
      this.rateLimitResubscribeTimeout = undefined;
    }
  };

  private scheduleRateLimitResubscribe = () => {
    if (this.isDisposed || !this.accountAllOrdersChannelEnabled) {
      return;
    }

    const now = virtualClock.getCurrentTimeMs();
    const delayMs = Math.max(
      this.wsRateLimitedUntil - now + LighterPrivateWebsocket.RATE_LIMIT_RESUBSCRIBE_BUFFER_MS,
      LighterPrivateWebsocket.RATE_LIMIT_RESUBSCRIBE_BUFFER_MS,
    );
    this.clearRateLimitResubscribeTimeout();
    this.parent.log(
      `[Lighter][PWS] Scheduling private WS subscribe retry in ${Math.round(delayMs)}ms after rate-limit cooldown.`,
      LogSeverity.Info,
    );

    this.rateLimitResubscribeTimeout = setTimeout(() => {
      this.rateLimitResubscribeTimeout = undefined;
      if (this.isDisposed || !this.isConnected || this.reconnecting) {
        return;
      }
      if (this.isWsRateLimited()) {
        this.scheduleRateLimitResubscribe();
        return;
      }
      this.parent.log('[Lighter][PWS] Retrying private WS subscriptions after rate-limit cooldown.', LogSeverity.Info);
      void this.authenticateAndSubscribe();
    }, delayMs);
  };

  private scheduleStreamRecoveryReconnectFallback = () => {
    this.clearStreamRecoveryReconnectTimeout();
    this.streamRecoveryReconnectTimeout = setTimeout(() => {
      this.streamRecoveryReconnectTimeout = undefined;
      if (this.isDisposed || !this.isConnected || this.reconnecting) {
        return;
      }

      const now = virtualClock.getCurrentTimeMs();
      const hasNoStreamMessages = this.accountStreamMessageCount === 0;
      const streamAgeMs =
        this.lastAccountStreamMessageAt > 0 ? now - this.lastAccountStreamMessageAt : Number.POSITIVE_INFINITY;
      const stillStale = hasNoStreamMessages || streamAgeMs > LighterPrivateWebsocket.STALE_STREAM_RECOVERY_AFTER_MS;
      if (!stillStale) {
        return;
      }

      this.parent.log(
        '[Lighter][PWS] Private stream still stale after recovery subscribe attempt; reconnecting.',
        LogSeverity.Warning,
      );
      this.reconnect();
    }, LighterPrivateWebsocket.RECOVERY_RECONNECT_GRACE_MS);
  };

  private maybeRecoverPrivateStream = () => {
    if (this.isDisposed || !this.isConnected || this.reconnecting) {
      return;
    }

    const now = virtualClock.getCurrentTimeMs();
    if (now - this.lastStreamRecoveryAttemptAt < LighterPrivateWebsocket.RECOVERY_ATTEMPT_COOLDOWN_MS) {
      return;
    }

    const timeSinceConnect = now - this.lastConnectTime;
    const hasNoStreamMessages = this.accountStreamMessageCount === 0;
    const streamAgeMs =
      this.lastAccountStreamMessageAt > 0 ? now - this.lastAccountStreamMessageAt : Number.POSITIVE_INFINITY;
    const shouldRecoverNoStream =
      hasNoStreamMessages && timeSinceConnect > LighterPrivateWebsocket.NO_STREAM_RECOVERY_AFTER_MS;
    const shouldRecoverStaleStream =
      this.lastAccountStreamMessageAt > 0 && streamAgeMs > LighterPrivateWebsocket.STALE_STREAM_RECOVERY_AFTER_MS;
    if (!shouldRecoverNoStream && !shouldRecoverStaleStream) {
      return;
    }

    this.lastStreamRecoveryAttemptAt = now;

    if (this.useReadonlyFallback) {
      this.parent.log(
        '[Lighter][PWS] Private stream recovery skipped while readonly fallback is active.',
        LogSeverity.Info,
      );
      return;
    }

    if (this.isWsRateLimited()) {
      this.parent.log(
        '[Lighter][PWS] Private stream recovery deferred while server rate-limit cooldown is active.',
        LogSeverity.Warning,
      );
      this.scheduleRateLimitResubscribe();
      return;
    }

    const reason = shouldRecoverNoStream
      ? `No account stream messages for ${Math.round(timeSinceConnect / 1000)}s since connect`
      : `Account stream stale for ${Math.round(streamAgeMs / 1000)}s`;
    this.parent.log(
      `[Lighter][PWS] ${reason}. Attempting recovery via re-subscribe with guarded reconnect fallback.`,
      LogSeverity.Warning,
    );
    void this.authenticateAndSubscribe();
    this.scheduleStreamRecoveryReconnectFallback();
  };

  throttleFetchOrders = (delayMs = 500, opts: { skipIfRecent?: boolean } = {}) => {
    if (opts.skipIfRecent && this.parent.shouldSkipOrderReconciliationFetch()) {
      return;
    }

    this.scheduleThrottledTask(
      'ordersThrottleTimeout',
      delayMs,
      async () => {
        if (opts.skipIfRecent && this.parent.shouldSkipOrderReconciliationFetch()) {
          return;
        }
        const orders = await this.parent.fetchOrders();
        if (this.isDisposed || this.parent.isDisposed) return;
        this.parent.store.addOrUpdateOrdersFull(orders);
      },
      '[Lighter] Error fetching orders:',
    );
  };

  throttleFetchPositions = (delayMs = 500) => {
    this.scheduleThrottledTask(
      'positionsThrottleTimeout',
      delayMs,
      async () => {
        const positions = await this.parent.fetchPositions();
        this.parent.store.addOrUpdatePositionsFull(positions);
        this.parent.lastWebsocketPositionsUpdate = virtualClock.getCurrentTimeMs();
      },
      '[Lighter] Error fetching positions:',
    );
  };

  throttleFetchBalance = (delayMs = 500) => {
    this.scheduleThrottledTask(
      'balanceThrottleTimeout',
      delayMs,
      async () => {
        await this.parent.fetchAndSetBalance({
          allowRecentAccountSnapshot: false,
          includeAccountLimits: false,
        });
      },
      '[Lighter] Error fetching balance:',
    );
  };

  private resolveExplicitlyClosedPositionUpdates = (rawPosition: Record<string, any>): Array<[Position, Position]> => {
    const rawSize = Number.parseFloat(
      String(rawPosition.size ?? rawPosition.position ?? rawPosition.qty ?? rawPosition.contracts ?? ''),
    );
    // We only synthesize close updates when payload explicitly reports zero size.
    if (!Number.isFinite(rawSize) || Math.abs(rawSize) > 0) {
      return [];
    }

    const marketId =
      rawPosition.marketId ?? rawPosition.market_id ?? rawPosition.marketIndex ?? rawPosition.market_index;
    const market = marketId !== undefined ? this.store.marketsById[String(marketId)] : undefined;
    const symbol = String(rawPosition.symbol ?? market?.symbol ?? '');
    if (!symbol) {
      return [];
    }

    const rawSide = String(rawPosition.side ?? rawPosition.positionSide ?? rawPosition.position_side ?? '')
      .trim()
      .toUpperCase();
    const sign = Number(rawPosition.sign ?? rawPosition.positionSign ?? rawPosition.position_sign ?? Number.NaN);
    const sideHint =
      rawSide === 'LONG' || rawSide === 'BUY'
        ? PositionSide.Long
        : rawSide === 'SHORT' || rawSide === 'SELL'
          ? PositionSide.Short
          : Number.isFinite(sign)
            ? sign < 0
              ? PositionSide.Short
              : sign > 0
                ? PositionSide.Long
                : undefined
            : undefined;

    const targets = this.store.positions.filter(
      (position) => position.symbol === symbol && position.contracts !== 0 && (!sideHint || position.side === sideHint),
    );
    if (!targets.length) {
      return [];
    }

    const parsedRealized = Number.parseFloat(
      String(rawPosition.realizedPnl ?? rawPosition.realized_pnl ?? rawPosition.realized_profit ?? ''),
    );
    const now = virtualClock.getCurrentTimeMs();

    return targets.map((position) => [
      position,
      {
        ...position,
        entryPrice: 0,
        notional: 0,
        unrealizedPnl: 0,
        contracts: 0,
        liquidationPrice: 0,
        realizedPnl: Number.isFinite(parsedRealized) ? parsedRealized : position.realizedPnl,
        info: rawPosition,
        lastUpdate: now,
      },
    ]);
  };

  private flattenByMarketPayload = <T>(
    payload: Record<string, unknown> | undefined,
    includeSingleObjects = false,
  ): T[] => {
    if (!payload) {
      return [];
    }

    return Object.values(payload).flatMap((entry) => {
      if (Array.isArray(entry)) {
        return entry as T[];
      }
      if (includeSingleObjects && entry && typeof entry === 'object') {
        return [entry as T];
      }
      return [];
    });
  };

  private flattenOrdersByMarket = (ordersByMarket: unknown): NonNullable<WsAccountMessage['data']['orders']> => {
    if (!ordersByMarket || typeof ordersByMarket !== 'object') {
      return [];
    }

    return this.flattenByMarketPayload<NonNullable<WsAccountMessage['data']['orders']>[number]>(
      ordersByMarket as Record<string, unknown>,
      true,
    );
  };

  private scheduleThrottledTask = (
    timeoutKey: ThrottleTimeoutKey,
    delayMs: number,
    task: () => Promise<void>,
    errorPrefix: string,
  ): void => {
    const existingTimeout = this[timeoutKey];
    if (existingTimeout) {
      clearTimeout(existingTimeout);
    }

    this[timeoutKey] = setTimeout(async () => {
      this[timeoutKey] = undefined;
      if (this.isDisposed) {
        return;
      }

      try {
        await task();
      } catch (err) {
        console.error(errorPrefix, err);
      }
    }, delayMs);
  };

  private sendSubscribePayload = (payload: Record<string, unknown>, hasAuth: boolean): void => {
    this.parent.log(
      `[Lighter][PWS] Sending subscribe for channel ${String(payload.channel || '')} (auth=${hasAuth ? 'yes' : 'no'}).`,
      LogSeverity.Info,
    );
    this.sendWithLog(JSON.stringify(payload));
  };

  private noteAccountStreamActivity = (statsTopic: 'account_all' | 'account_all_orders', channel: string) => {
    if (this.failedHandshakes > 0) {
      this.failedHandshakes = 0;
    }
    if (this.readonlyFallbackFailures > 0) {
      this.readonlyFallbackFailures = 0;
    }
    // Healthy again — keep the processClose `> 100` cap a per-unhealthy-streak
    // bound, not a lifetime one (long-lived sessions would otherwise die forever).
    if (this.onCloseCount > 0) {
      this.onCloseCount = 0;
    }
    this.accountStreamMessageCount += 1;
    this.lastAccountStreamMessageAt = virtualClock.getCurrentTimeMs();
    this.clearStreamRecoveryReconnectTimeout();
    this.clearPrivateStreamRateLimitErrorState();
    this.recordMessageHandlerStats(statsTopic);

    if (this.accountStreamMessageCount === 1) {
      this.parent.log(
        `[Lighter][PWS] First private account stream message received on channel "${channel || 'unknown'}".`,
        LogSeverity.Info,
      );
    }
  };

  private isRecentStreamEvent = (timestampMs: number): boolean => {
    if (!Number.isFinite(timestampMs) || timestampMs <= 0) {
      return false;
    }

    const isFreshEvent = Date.now() - timestampMs <= 5_000;
    const isEnoughTimePassedAfterConnect = virtualClock.getCurrentTimeMs() - this.lastConnectTime > 5_000;
    return isFreshEvent && isEnoughTimePassedAfterConnect;
  };

  private normalizeOrderStatus = (order: Record<string, any>): string =>
    String(order.status ?? order.order_status ?? order.state ?? '')
      .trim()
      .toUpperCase();

  private isUserCancelledReason = (cancelReason: unknown): boolean => {
    const normalizedReason = String(cancelReason ?? '')
      .trim()
      .toUpperCase();
    return normalizedReason === 'USER_CANCELLED' || normalizedReason === 'USER_CANCELED';
  };

  connectAndSubscribe = async () => {
    if (this.isDisposed) {
      return;
    }

    const useWsProxyRoute = this.isWsProxyRouteEnabled();
    const baseUrl = useWsProxyRoute
      ? resolveTransportRouting(this.parent.name, this.parent.options).wsPrivate.host!
      : this.parent.lighterDeploymentProfile.wsUrl;
    const shouldUseReadonlyFallback = this.useReadonlyFallback && !useWsProxyRoute;
    const wsUrl = shouldUseReadonlyFallback ? `${baseUrl}${baseUrl.includes('?') ? '&' : '?'}readonly=true` : baseUrl;
    const connectionMode = useWsProxyRoute ? 'proxy-route' : shouldUseReadonlyFallback ? 'readonly-fallback' : 'direct';
    this.parent.log(
      `[Lighter][PWS] Connecting (${connectionMode}, attempt #${this.failedHandshakes + 1}): ${wsUrl}`,
      LogSeverity.Info,
    );

    this.openedSinceLastConnect = false;
    this.ws = this.createWebsocket(wsUrl);

    this.ws.addEventListener('open', this.onOpen);
    this.ws.addEventListener('message', this.onMessage);
    this.ws.addEventListener('close', this.onClose);
    this.ws.addEventListener('error', ((evt: ErrorEvent) => {
      const errorDetail = evt?.message || (evt as any)?.error?.message || '';
      this.parent.log(
        `[Lighter][PWS] Connection error${errorDetail ? `: ${errorDetail}` : ''} (mode: ${connectionMode})`,
        LogSeverity.Warning,
      );
    }) as any);
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.openedSinceLastConnect = true;
      this.lastStreamRecoveryAttemptAt = 0;
      this.hasPendingAccountAllOrdersSubscribe = false;
      this.hasReceivedAccountAllOrdersAck = false;
      this.clearStreamRecoveryReconnectTimeout();
      this.clearRateLimitResubscribeTimeout();
      this.lastConnectTime = virtualClock.getCurrentTimeMs();
      this.lastRateLimitedPingAt = 0;
      this.lastRateLimitedPingSkipLogAt = 0;
      this.lastIdlePingAt = 0;
      if (this.useReadonlyFallback && !this.hasWarnedReadonlyFallback) {
        this.hasWarnedReadonlyFallback = true;
        this.emitReadonlyFallbackWarning();
      }
      this.beginDiagnosticsLoop();
      this.ping();
      void this.authenticateAndSubscribe();
      if (ENABLE_RECONNECTING_WEBSOCKET) {
        this.parent.fetchAndSetPrivateSnapshot();
      }
    }
  };

  protected onClose = (event?: any): void => {
    // This override does not delegate to the base onClose, so the shared close
    // diagnostics have to be recorded explicitly.
    this.recordCloseDiagnostics(Number.isFinite(Number(event?.code)) ? Number(event?.code) : undefined, { syntheticIfNoSocket: true });
    this.stopDiagnosticsLoop();
    this.clearStreamRecoveryReconnectTimeout();
    this.clearRateLimitResubscribeTimeout();

    const closeCode = Number(event?.code);
    const closeReason = typeof event?.reason === 'string' ? event.reason : '';
    const failedBeforeOpen = !this.openedSinceLastConnect;
    const now = virtualClock.getCurrentTimeMs();
    const timeSinceOpen = this.lastConnectTime > 0 ? now - this.lastConnectTime : Number.POSITIVE_INFINITY;
    const earlyPostOpenNoStream =
      this.openedSinceLastConnect &&
      timeSinceOpen <= LighterPrivateWebsocket.EARLY_CLOSE_WINDOW_MS &&
      this.accountStreamMessageCount === 0;
    const closeClassification: LighterPrivateCloseClassification = classifyLighterPrivateClose({
      closeCode,
      closeReason,
      failedBeforeOpen,
      earlyPostOpenNoStream,
    });

    if (closeClassification.isHandshakeFailure) {
      this.failedHandshakes += 1;
      if (closeClassification.isReadonlyFallbackSignal) {
        this.readonlyFallbackFailures += 1;
      }
      if (
        !closeClassification.isAuthIssue &&
        !this.isWsProxyRouteEnabled() &&
        !this.useReadonlyFallback &&
        closeClassification.isReadonlyFallbackSignal &&
        this.readonlyFallbackFailures >= 3
      ) {
        this.useReadonlyFallback = true;
        this.hasWarnedReadonlyFallback = true;
        this.emitReadonlyFallbackWarning(true);
      }
    }

    if (closeClassification.isRateLimited) {
      this.wsRateLimitedUntil = now + LighterPrivateWebsocket.RATE_LIMIT_COOLDOWN_MS;
    }

    this.openedSinceLastConnect = false;

    this.lastReceived = 0;
    this.lastIdlePingAt = 0;
    if (this.lastReceivedTimer) {
      clearTimeout(this.lastReceivedTimer);
      this.lastReceivedTimer = undefined;
    }
    this.parent.syncSocketHealthToStore?.();

    if (closeCode !== undefined && closeCode !== 1000 && !this.parent.isDisposed) {
      this.parent.log(
        `[Lighter][PWS] WebSocket closed (code ${closeCode}${closeReason ? `, reason: ${closeReason}` : ''}, handshakes failed: ${this.failedHandshakes})`,
        LogSeverity.Warning,
      );
    }

    if (this.reconnecting || this.parent.isDisposed) {
      return;
    }
    this.reconnecting = true;

    if (this.pingTimeoutId) {
      clearTimeout(this.pingTimeoutId);
      this.pingTimeoutId = undefined;
    }
    if (this.pongMissTimeoutId) {
      clearTimeout(this.pongMissTimeoutId);
      this.pongMissTimeoutId = undefined;
    }

    this.ws?.removeEventListener?.('open', this.onOpen);
    this.ws?.removeEventListener?.('message', this.onMessage);
    this.ws?.removeEventListener?.('close', this.onClose);
    this.ws = undefined;

    this.onCloseCount++;
    if (this.onCloseCount > 100) {
      this.parent.log('[Lighter][PWS] onClose count > 100, stopping reconnection', LogSeverity.Error);
      return;
    }

    const reconnectDelay = calculateLighterPrivateReconnectDelay({
      baseDelayMs: WS_MIN_RECONNECTION_DELAY,
      failedHandshakes: this.failedHandshakes,
      shouldBackoff: closeClassification.shouldBackoff,
      isRateLimited: closeClassification.isRateLimited,
      isWsRateLimited: this.isWsRateLimited(),
      rateLimitReconnectDelayMs: LighterPrivateWebsocket.RATE_LIMIT_RECONNECT_DELAY_MS,
    });

    this.parent.log(`[Lighter][PWS] Reconnecting in ${Math.round(reconnectDelay)}ms...`, LogSeverity.Info);

    setTimeout(() => {
      if (this.parent.isDisposed) {
        return;
      }
      this.reconnecting = false;
      this.lastCloseTime = Date.now();
      void this.connectAndSubscribe();
    }, reconnectDelay);
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      this.trackLastReceived();
      this.emitWsDiagnosticReceive(data);

      const json = jsonParse<any>(data);

      const channel = typeof json?.channel === 'string' ? json.channel : '';
      const isAccountAllChannel =
        channel === 'account_all' || channel.startsWith('account_all:') || channel.startsWith('account_all/');
      const isAccountAllOrdersChannel =
        channel === 'account_all_orders' ||
        channel.startsWith('account_all_orders:') ||
        channel.startsWith('account_all_orders/');

      if (isAccountAllChannel) {
        const dataEnvelope = json?.data as Record<string, unknown> | undefined;
        const envelopeOrders = dataEnvelope?.orders;
        const envelopePositions = dataEnvelope?.positions;
        const envelopeTrades = dataEnvelope?.trades;
        const envelopeAccount = dataEnvelope?.account;
        const envelopeBalances = dataEnvelope?.balances;
        const hasArrayPayloads =
          dataEnvelope &&
          (Array.isArray(envelopeOrders) || Array.isArray(envelopePositions) || Array.isArray(dataEnvelope.fills));
        const byMarketOrders = (json?.orders ?? envelopeOrders) as Record<string, unknown> | undefined;
        const byMarketPositions = (json?.positions ?? envelopePositions) as Record<string, unknown> | undefined;
        const byMarketTrades = (json?.trades ?? envelopeTrades) as Record<string, unknown> | undefined;
        const hasBalancePayload = Boolean(json?.account ?? json?.balances ?? envelopeAccount ?? envelopeBalances);
        const hasByMarketPayload = Boolean(
          (byMarketOrders && Object.keys(byMarketOrders).length > 0) ||
          (byMarketPositions && Object.keys(byMarketPositions).length > 0) ||
          (byMarketTrades && Object.keys(byMarketTrades).length > 0),
        );
        const hasMeaningfulPayload = Boolean(hasArrayPayloads || hasByMarketPayload || hasBalancePayload);

        if (hasMeaningfulPayload) {
          this.noteAccountStreamActivity('account_all', channel);
          this.parent.invalidateAccountSnapshotCache();
          this.throttleFetchBalance(500);

          // Lighter WS streams position aggregates (open_order_count, position size)
          // but not individual order details. Trigger a REST order fetch so chart
          // order updates don't wait for the next tick() cycle.
          if (this.shouldUseRestOrderReconciliation()) {
            this.throttleFetchOrders(500, { skipIfRecent: true });
          }
        }

        if (hasArrayPayloads && dataEnvelope) {
          this.handleAccountTopic({ ...(json as WsAccountMessage), data: dataEnvelope } as WsAccountMessage);
        } else {
          const accountAllPayloadSource = hasByMarketPayload && dataEnvelope ? dataEnvelope : json;
          this.handleAccountAllTopic(accountAllPayloadSource);
        }
      }

      if (isAccountAllOrdersChannel) {
        const dataEnvelope = json?.data as Record<string, unknown> | undefined;
        const ordersByMarket = (json?.orders ?? dataEnvelope?.orders) as Record<string, unknown> | undefined;
        const orders = this.flattenOrdersByMarket(ordersByMarket);
        if (orders.length > 0) {
          this.noteAccountStreamActivity('account_all_orders', channel);
          this.handleOrderTopic(orders);
        }
      }

      const msgType = typeof json?.type === 'string' ? json.type : '';
      if (
        (msgType === 'subscribed' || msgType.startsWith('subscribed/')) &&
        typeof json?.channel === 'string' &&
        (json.channel.startsWith('account_all/') ||
          json.channel.startsWith('account_all:') ||
          json.channel === 'account_all' ||
          json.channel.startsWith('account_all_orders/') ||
          json.channel.startsWith('account_all_orders:') ||
          json.channel === 'account_all_orders')
      ) {
        if (
          json.channel.startsWith('account_all_orders/') ||
          json.channel.startsWith('account_all_orders:') ||
          json.channel === 'account_all_orders'
        ) {
          this.hasPendingAccountAllOrdersSubscribe = false;
          this.hasReceivedAccountAllOrdersAck = true;
        }
        this.parent.log(
          `[Lighter][PWS] Subscription acknowledged for ${json.channel} (type=${msgType}).`,
          LogSeverity.Info,
        );
      }

      if (msgType.startsWith('update/') && (isAccountAllChannel || isAccountAllOrdersChannel)) {
        this.parent.log(`[Lighter][PWS] Streaming update received (type=${msgType}).`, LogSeverity.Info);
      }

      if (json?.type === 'error' || json?.error) {
        const errorObj = json?.error;
        const rawErrorCode =
          typeof errorObj === 'object' && errorObj !== null ? (errorObj as Record<string, any>).code : json?.code;
        const parsedErrorCode = Number.parseInt(String(rawErrorCode ?? ''), 10);
        const errMsg =
          typeof errorObj === 'string'
            ? errorObj
            : typeof errorObj === 'object' && errorObj !== null
              ? `[${errorObj.code ?? '?'}] ${errorObj.message ?? JSON.stringify(errorObj)}`
              : String(json?.message || 'unknown private websocket error');
        const normalizedErrMsg = errMsg.toLowerCase();
        const isAlreadySubscribedError = parsedErrorCode === 30003 || normalizedErrMsg.includes('already subscribed');
        if (isAlreadySubscribedError) {
          const isOrdersSubscriptionDuplicate =
            this.hasPendingAccountAllOrdersSubscribe ||
            isAccountAllOrdersChannel ||
            normalizedErrMsg.includes('account_all_orders');
          if (isOrdersSubscriptionDuplicate) {
            this.hasPendingAccountAllOrdersSubscribe = false;
            this.hasReceivedAccountAllOrdersAck = true;
          }
          this.parent.log(
            `[Lighter][PWS] Ignoring benign duplicate subscription error from private stream: ${errMsg}`,
            LogSeverity.Info,
          );
          return;
        }
        const isInvalidChannelError = errMsg.toLowerCase().includes('invalid channel');
        const isRateLimitError =
          normalizedErrMsg.includes('too many websocket messages') || normalizedErrMsg.includes('rate limit');
        const isPolicyRestrictionError = LIGHTER_POLICY_REASON_RE.test(errMsg);
        const now = virtualClock.getCurrentTimeMs();
        if (isRateLimitError) {
          this.notePrivateStreamRateLimitError(now);
          this.hasPendingAccountAllOrdersSubscribe = false;
          this.wsRateLimitedUntil = now + LighterPrivateWebsocket.RATE_LIMIT_COOLDOWN_MS;
          this.parent.log(
            `[Lighter][PWS] Server rate-limited private WS messages. Applying ${
              LighterPrivateWebsocket.RATE_LIMIT_COOLDOWN_MS / 1000
            }s cooldown.`,
            LogSeverity.Warning,
          );
          this.scheduleRateLimitResubscribe();
        }
        if (isInvalidChannelError && this.hasPendingAccountAllOrdersSubscribe && !this.hasReceivedAccountAllOrdersAck) {
          this.accountAllOrdersChannelEnabled = false;
          this.hasPendingAccountAllOrdersSubscribe = false;
          if (!this.hasWarnedOrdersChannelUnsupported) {
            this.hasWarnedOrdersChannelUnsupported = true;
            this.parent.log(
              '[Lighter][PWS] account_all_orders channel appears unsupported in this environment. Continuing with REST order reconciliation.',
              LogSeverity.Warning,
            );
          }
          return;
        }
        this.parent.log(`[Lighter][PWS] Received error from private stream: ${errMsg}`, LogSeverity.Warning);
        if (!this.hasWarnedProxyStreamErrorToast) {
          const toastMessage = this.getPrivateStreamErrorWarningMessage({
            errMsg,
            isRateLimitError,
            isPolicyRestrictionError,
          });
          const rateLimitWarningMessage = this.getRateLimitWarningMessage();
          const shouldForceToastStreamError =
            !isRateLimitError ||
            toastMessage !== rateLimitWarningMessage ||
            this.shouldForceToastPrivateStreamRateLimit(now);
          if (shouldForceToastStreamError) {
            this.hasWarnedProxyStreamErrorToast = true;
            this.parent.emitter.emit('warning', toastMessage, { forceToast: true });
          }
        }
      }

      if (json?.type === 'ping') {
        this.send(JSON.stringify({ type: 'pong', timestamp: virtualClock.getCurrentTimeMs() }));
      }

      if (json?.type === 'pong') {
        this.handlePongEvent(this.getPingIntervalMs());
      }
    }
  };

  ping = () => {
    if (!this.isDisposed) {
      const now = virtualClock.getCurrentTimeMs();
      if (this.isWsRateLimited()) {
        const timeSinceLastRateLimitedPing = now - this.lastRateLimitedPingAt;
        if (timeSinceLastRateLimitedPing < LighterPrivateWebsocket.RATE_LIMIT_PING_INTERVAL_MS) {
          if (now - this.lastRateLimitedPingSkipLogAt >= LighterPrivateWebsocket.RATE_LIMIT_PING_SKIP_LOG_COOLDOWN_MS) {
            this.lastRateLimitedPingSkipLogAt = now;
            this.parent.log(
              '[Lighter][PWS] Deferring ping while server rate-limit cooldown is active.',
              LogSeverity.Info,
            );
          }
          return;
        }
        this.lastRateLimitedPingAt = now;
      }
      this.pingAt = performance.now();
      this.sendWithLog(JSON.stringify({ type: 'ping', timestamp: now }));
    }
  };

  handleAccountTopic = (data: WsAccountMessage) => {
    if (data.data.orders) {
      this.handleOrderTopic(data.data.orders);
    }
    if (data.data.positions) {
      this.handlePositionTopic(data.data.positions);
    }
    if (data.data.fills) {
      this.handleFillsTopic(data.data.fills);
    }
  };

  handleAccountAllTopic = (message: Record<string, any>) => {
    const ordersByMarket = message.orders as Record<string, unknown> | undefined;
    if (ordersByMarket) {
      const orders =
        this.flattenByMarketPayload<NonNullable<WsAccountMessage['data']['orders']>[number]>(ordersByMarket);
      this.handleOrderTopic(orders as WsAccountMessage['data']['orders']);
    }

    const positionsByMarket = message.positions as Record<string, unknown> | undefined;
    if (positionsByMarket) {
      const positions = this.flattenByMarketPayload<NonNullable<WsAccountMessage['data']['positions']>[number]>(
        positionsByMarket,
        true,
      );
      this.handlePositionTopic(positions as WsAccountMessage['data']['positions']);
    }

    const tradesByMarket = message.trades as Record<string, unknown> | undefined;
    if (tradesByMarket) {
      const fills = this.flattenByMarketPayload<Record<string, any>>(tradesByMarket);
      this.handleFillsTopic(
        fills.map((trade: any) => ({
          fillId: String(trade.trade_id ?? trade.fill_id ?? ''),
          orderId: String(trade.order_id ?? trade.ask_id ?? trade.bid_id ?? ''),
          tradeId: String(trade.trade_id ?? ''),
          symbol: String(trade.symbol ?? ''),
          side: trade.side === 'buy' || trade.is_maker_ask ? 'SELL' : 'BUY',
          price: String(trade.price ?? '0'),
          size: String(trade.size ?? trade.amount ?? '0'),
          fee: String(trade.fee ?? '0'),
          isMaker: Boolean(trade.is_maker_ask),
          timestamp: Number(trade.timestamp ?? Date.now()),
        })) as WsAccountMessage['data']['fills'],
      );
    }
  };

  handleOrderTopic = (data: WsAccountMessage['data']['orders']) => {
    if (!data) return;

    // Check if any orders are cancelled before deciding to throttle fetch
    const hasCancelledOrders = data.some((order) => {
      const normalized = this.normalizeOrderStatus(order as unknown as Record<string, any>);
      return normalized === 'CANCELLED' || normalized === 'CANCELED';
    });
    const shouldBackfillOrdersViaRest = this.shouldUseRestOrderReconciliation();

    // Only fetch orders if we're not dealing with cancellations
    if (!hasCancelledOrders && shouldBackfillOrdersViaRest) {
      this.throttleFetchOrders(500, { skipIfRecent: true });
    }

    data.forEach((order) => {
      const rawOrder = order as unknown as Record<string, any>;
      const normalizedStatus = this.normalizeOrderStatus(rawOrder);
      const rawOrderId = String(
        rawOrder.orderId ??
          rawOrder.order_id ??
          rawOrder.orderIndex ??
          rawOrder.order_index ??
          rawOrder.OrderId ??
          rawOrder.OrderIndex ??
          '',
      );

      // Skip recently cancelled orders
      if (rawOrderId && this.parent.recentlyCancelledOrders.has(rawOrderId)) {
        return;
      }

      const orders = this.parent.mapOrder(order);
      if (!orders.length) {
        return;
      }
      const firstOrder = orders[0];
      const isOpenLikeStatus =
        normalizedStatus === 'OPEN' ||
        normalizedStatus === 'PENDING' ||
        normalizedStatus === 'PARTIALLY_FILLED' ||
        firstOrder.status === OrderStatus.Open;
      const isCancelledStatus =
        normalizedStatus === 'CANCELLED' ||
        normalizedStatus === 'CANCELED' ||
        firstOrder.status === OrderStatus.Canceled;
      const isFilledStatus = normalizedStatus === 'FILLED';

      const price = Number.isFinite(firstOrder.price)
        ? firstOrder.price
        : parseFloat(v(order, 'price') || v(order, 'limit_price') || '0');
      const amount = Number.isFinite(firstOrder.amount)
        ? firstOrder.amount
        : parseFloat(
            v(order, 'amount') ||
              v(order, 'size') ||
              v(order, 'initialBaseAmount') ||
              v(order, 'initial_base_amount') ||
              '0',
          );
      const updatedAtMs = Number(v(order, 'updatedAt') || v(order, 'updated_at') || v(order, 'timestamp') || 0);

      const shouldNotify = this.isRecentStreamEvent(updatedAtMs);

      if (isFilledStatus) {
        // False positive when order is replaced
        if (price <= 0 && amount <= 0) return;
      }

      if (isFilledStatus && shouldNotify) {
        this.emitter.emit('fill', {
          id: firstOrder.id,
          side: firstOrder.side,
          symbol: firstOrder.symbol,
          price,
          amount,
        });
      }

      if (isFilledStatus || isCancelledStatus) {
        const cancelReason = String(rawOrder.cancelReason ?? rawOrder.cancel_reason ?? '');
        if (!isFilledStatus && shouldNotify && cancelReason && !this.isUserCancelledReason(cancelReason)) {
          this.emitAndLogError(cancelReason);
        }

        if (isCancelledStatus) {
          // Remove cancelled orders entirely
          const idsToRemove = orders.map((safeCexOrder) => ({ id: safeCexOrder.id }));
          this.store.removeOrders(idsToRemove);
        } else {
          // For FILLED orders, update their status
          this.store.addOrUpdateOrders(
            orders.map((safeCexOrder) => ({
              ...safeCexOrder,
              status: OrderStatus.Closed,
            })),
          );
        }
      } else if (isOpenLikeStatus) {
        // Only add open/pending orders
        this.store.addOrUpdateOrders(orders);
      }
    });
  };

  handlePositionTopic = (data: WsAccountMessage['data']['positions']) => {
    if (!data) return;

    const updates: Array<[Position, Position]> = [];
    for (const rawPosition of data) {
      try {
        const mapped = this.parent.mapPosition(rawPosition as Record<string, any>);
        if (mapped) {
          updates.push([mapped, mapped]);
        } else {
          updates.push(...this.resolveExplicitlyClosedPositionUpdates(rawPosition as Record<string, any>));
        }
      } catch (error: any) {
        this.parent.log(
          `[Lighter][PWS] Failed to map position update: ${error?.message || String(error)}`,
          LogSeverity.Warning,
        );
      }
    }

    if (!updates.length) {
      return;
    }

    this.store.updatePositions(updates);
    this.parent.lastWebsocketPositionsUpdate = virtualClock.getCurrentTimeMs();
  };

  handleFillsTopic = (data: WsAccountMessage['data']['fills']) => {
    if (!data) return;

    // Emit fill events for each fill
    data.forEach((fill) => {
      if (this.isRecentStreamEvent(fill.timestamp)) {
        this.emitter.emit('fill', {
          id: fill.orderId,
          side: fill.side === 'BUY' ? 'buy' : 'sell',
          symbol: fill.symbol,
          price: parseFloat(fill.price),
          amount: parseFloat(fill.size),
        });
      }
    });

    // In restricted readonly fallback mode, fills can arrive before/without a complete
    // position delta payload. Reconcile positions quickly so closed positions disappear fast.
    if (this.useReadonlyFallback) {
      this.throttleFetchPositions(250);
    }
  };

  private authenticateAndSubscribe = async () => {
    const accountIndex = this.parent.getPrivateWsAccountIndex();

    const accountAllPayload: Record<string, unknown> = {
      type: 'subscribe',
      channel: `account_all/${accountIndex}`,
    };

    this.sendSubscribePayload(accountAllPayload, false);

    if (!this.accountAllOrdersChannelEnabled) {
      return;
    }
    if (this.hasPendingAccountAllOrdersSubscribe || this.hasReceivedAccountAllOrdersAck) {
      return;
    }

    if (this.isWsRateLimited()) {
      this.parent.log(
        '[Lighter][PWS] Skipping account_all_orders subscription during server rate-limit cooldown.',
        LogSeverity.Warning,
      );
      this.scheduleRateLimitResubscribe();
      return;
    }

    let token: string | undefined;
    try {
      token = await this.parent.getPrivateWsAuthToken(60);
    } catch {
      this.parent.log('[Lighter][PWS] Auth token unavailable, subscribing without auth.', LogSeverity.Warning);
    }

    if (!token) {
      this.parent.log(
        '[Lighter][PWS] Skipping account_all_orders subscription because auth token is unavailable.',
        LogSeverity.Warning,
      );
      return;
    }

    const accountAllOrdersPayload: Record<string, unknown> = {
      type: 'subscribe',
      channel: `account_all_orders/${accountIndex}`,
      auth: token,
    };
    this.hasPendingAccountAllOrdersSubscribe = true;
    this.sendSubscribePayload(accountAllOrdersPayload, true);
  };

  dispose = () => {
    this.stopDiagnosticsLoop();
    this.clearStreamRecoveryReconnectTimeout();
    this.clearRateLimitResubscribeTimeout();
    if (this.positionsThrottleTimeout) {
      clearTimeout(this.positionsThrottleTimeout);
      this.positionsThrottleTimeout = undefined;
    }
    if (this.balanceThrottleTimeout) {
      clearTimeout(this.balanceThrottleTimeout);
      this.balanceThrottleTimeout = undefined;
    }
    super.dispose();
  };
}
