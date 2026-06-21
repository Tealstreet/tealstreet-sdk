import flatten from 'lodash/flatten';
import { resolveExchangeRouting } from '../../routing/resolveExchangeRouting';
import { nanoid } from 'nanoid';

import { jsonParse } from '@tealstreet/json-parse/src/jsonParse';
import { WS_MIN_RECONNECTION_DELAY } from '../../config/config';
import { Candle, LogSeverity, OHLCVOptions, OrderBook, PositionSide, Ticker, Writable } from '../../types';
import { getCandleStartTime } from '../../utils/getCandleStartTime';
import { calcOrderBookTotal } from '../../utils/orderbook';
import { add, divide } from '../../utils/safe-math';
import { updateOrderBook } from '../../utils/updateOrderBook';
import { virtualClock } from '../../utils/virtualClock';
import { BaseWebSocket } from '../base.ws';
import { LighterExchange } from './lighter.exchange';
import { BASE_WS_URL, ENDPOINTS, INTERVAL } from './lighter.types';
import { calculateLighterPublicReconnectDelay, classifyLighterPublicClose } from './lighter.ws-policy';
import {
  buildLighterCandleTopic,
  buildLighterTradesTopic,
  shouldAttemptLighterOhlcvRecovery,
  shouldPromoteLighterOhlcvRecoveryToProxy,
} from './lighter.ws-public-ohlcv';
import {
  extractLighterTickerMarketIdFromChannel,
  parseLighterWsBookSidePrice,
  parseLighterWsNumber,
  shouldDispatchLighterPublicMessage,
} from './lighter.ws-public-parsers';

type Data = Record<string, any>;
type MessageHandlers = {
  [topic: string]: Array<(json: Data) => void>;
};

type SubscribedTopics = {
  [id: string]: string[] | string;
};

const normalizeLighterFundingRate = (fundingRatePercentPerHour: number): number => {
  const eightHourPercent =
    // Lighter's UI treats the displayed 0.0012% hourly floor as the canonical 0.0100% 8h rate.
    fundingRatePercentPerHour === 0.0012 ? 0.01 : fundingRatePercentPerHour * 8;

  return eightHourPercent / 100;
};

export class LighterPublicWebsocket extends BaseWebSocket<LighterExchange> {
  private static readonly PROXY_ROUTE_FAILURES_BEFORE_DIRECT_FALLBACK = 3;
  private static readonly PROXY_DIRECT_FALLBACK_FAILURE_COOLDOWN_MS = 2 * 60_000;
  private static readonly DIRECT_ROUTE_FAILURES_BEFORE_READONLY = 2;
  private static readonly EARLY_CLOSE_WINDOW_MS = 12_000;
  private static readonly RATE_LIMIT_RECONNECT_DELAY_MS = 30_000;
  private static readonly QUIET_STREAM_STALE_THRESHOLD_MS = 120_000;
  private static readonly OHLCV_DIRECT_ROUTE_NO_DATA_TIMEOUT_MS =
    LighterPublicWebsocket.QUIET_STREAM_STALE_THRESHOLD_MS;
  private static readonly OHLCV_RECOVERY_RECONNECT_COOLDOWN_MS = 120_000;
  private static readonly ORDER_BOOK_STALE_TIMEOUT_MS = 90_000;
  private static readonly ORDER_BOOK_RECOVERY_COOLDOWN_MS = 45_000;
  private static readonly ORDER_BOOK_INITIAL_DATA_TIMEOUT_MS = 1_500;
  private static readonly ORDER_BOOK_REST_FALLBACK_INTERVAL_MS = 2_000;
  private static readonly ORDER_BOOK_REST_FALLBACK_403_COOLDOWN_MS = 60_000;
  private static readonly TICKER_REST_FALLBACK_CONNECTING_WATCHDOG_MS = 1_500;
  private static readonly TICKER_REST_FALLBACK_WATCHDOG_MS = 8_000;
  private static readonly TICKER_REST_FALLBACK_INTERVAL_MS = 3_000;
  private static readonly TICKER_REST_FALLBACK_403_COOLDOWN_MS = 60_000;
  private static readonly MAX_IMMEDIATE_SUBSCRIBE_MESSAGES = 24;
  private static readonly SUBSCRIBE_STAGGER_DELAY_MS = 15;

  private readonly bboTickerTopicPrefix = 'bbo_ticker_';
  // See LighterPrivateWebsocket — any inbound traffic (ticker / book / trade
  // updates) is sufficient proof of liveness on Lighter's public stream.
  protected override treatAnyMessageAsPong = true;
  protected shouldEnforcePongMissReconnect = (): boolean => false;
  getWatchdogStaleThresholdMs = (): number => LighterPublicWebsocket.QUIET_STREAM_STALE_THRESHOLD_MS;

  private failedConnections = 0;
  private openedSinceLastConnect = false;
  private hasEverConnected = false;
  private useProxyRoute = false;
  private useDirectFallbackRoute = false;
  private useReadonlyDirectRoute = false;
  private proxyRouteFailureCount = 0;
  private directRouteFailureCount = 0;
  private directFallbackProbePausedUntil = 0;
  private connectedViaProxyRoute = false;
  private connectStartedAt = 0;
  private hasReceivedMessageSinceConnect = false;
  private ohlcvNoDataTimeouts: Record<string, NodeJS.Timeout | undefined> = {};
  private orderBookStaleTimeouts: Record<string, NodeJS.Timeout | undefined> = {};
  private orderBookRecoveryAttempts: Record<string, number> = {};
  private lastOrderBookRecoveryAtByTopic: Record<string, number> = {};
  private lastOhlcvRecoveryReconnectAt = 0;
  private pendingSubscribeTimeouts: NodeJS.Timeout[] = [];
  private orderBookInitialDataTimeouts: Record<string, NodeJS.Timeout | undefined> = {};
  private orderBookRestFallbackIntervals: Record<string, NodeJS.Timeout | undefined> = {};
  private orderBookRestFallbackInFlight: Record<string, boolean> = {};
  private orderBookRestFallbackCooldownUntilByTopic: Record<string, number> = {};
  private tickerRestFallbackWatchdog?: NodeJS.Timeout;
  private tickerRestFallbackInterval?: NodeJS.Timeout;
  private tickerRestFallbackInFlight?: Promise<void>;
  private tickerRestFallbackCooldownUntil = 0;
  private hasPrimedTickerRestFallbackForConnection = false;
  private hasWarnedTickerRestFallback = false;

  topics: SubscribedTopics = {
    marketStatsAll: 'market_stats/all',
  };
  messageHandlers: MessageHandlers = {
    'market_stats:all': [(d: Data) => this.handleMarketStatsEvent(d)],
    ticker: [(d: Data) => this.handleTickerEvent(d)],
    tickers: [(d: Data) => this.handleTickersEvent(d)],
    pong: [() => this.handlePongEvent()],
  };

  constructor(parent: LighterExchange) {
    super(parent);
  }

  private isWsProxyRouteEnabled = (): boolean =>
    (this.parent.options.tealstreetWsRoute ?? false) && !(this.parent.options.testnet ?? false);

  private isTickerRestFallbackCoolingDown = (): boolean =>
    virtualClock.getCurrentTimeMs() < this.tickerRestFallbackCooldownUntil;

  private hasHydratedTickerPrice = (): boolean =>
    this.parent.store.tickers.some((ticker) =>
      [ticker.mark, ticker.last, ticker.index, ticker.bid, ticker.ask].some(
        (price) => typeof price === 'number' && Number.isFinite(price) && price > 0,
      ),
    );

  private syncTickerRestFallbackToPriceHydration = (missingPriceReason?: string): void => {
    if (this.hasHydratedTickerPrice()) {
      this.clearTickerRestFallbackWatchdog();
      this.stopTickerRestFallback();
      return;
    }

    if (missingPriceReason) {
      this.startTickerRestFallback(missingPriceReason);
    }
  };

  private primeTickerRestFallback = () => {
    if (this.connectedViaProxyRoute && this.hasPrimedTickerRestFallbackForConnection) {
      return;
    }

    this.hasPrimedTickerRestFallbackForConnection = true;
    void this.fetchAndApplyTickerFallback();
  };

  connectAndSubscribe = (_isReconnect = false) => {
    if (!this.isDisposed) {
      // Keep public and private routing aligned: proxy-mode accounts should not
      // wait for direct public WS failures before ticker/orderbook priming.
      const shouldUseProxyRoute = this.isWsProxyRouteEnabled() && !this.useDirectFallbackRoute;
      const shouldUseReadonlyDirectRoute = this.useReadonlyDirectRoute && !shouldUseProxyRoute;
      const endpoint = shouldUseProxyRoute
        ? resolveExchangeRouting(this.parent.name, this.parent.options).wsPublic.host!
        : BASE_WS_URL.public[this.parent.options.testnet ? 'testnet' : 'livenet'];
      const separator = endpoint.includes('?') ? '&' : '?';
      const query = `requestId=${nanoid()}${shouldUseReadonlyDirectRoute ? '&readonly=true' : ''}`;

      this.connectedViaProxyRoute = shouldUseProxyRoute;
      this.openedSinceLastConnect = false;
      this.connectStartedAt = virtualClock.getCurrentTimeMs();
      this.hasReceivedMessageSinceConnect = false;
      this.hasPrimedTickerRestFallbackForConnection = false;
      this.ws = this.createWebsocket(`${endpoint}${separator}${query}`);

      this.ws.addEventListener('open', this.onOpen);
      this.ws.addEventListener('message', this.onMessage);
      this.ws.addEventListener('close', this.onClose);

      if (shouldUseProxyRoute) {
        this.primeTickerRestFallback();
        this.armTickerRestFallbackWatchdog(
          LighterPublicWebsocket.TICKER_REST_FALLBACK_CONNECTING_WATCHDOG_MS,
          'public proxy websocket has not produced messages yet; polling tickers via REST',
        );
      }
    }
  };

  onOpen = () => {
    if (!this.isDisposed) {
      this.openedSinceLastConnect = true;
      this.syncBboTickerTopics();
      this.subscribe();
      // Prime mark/last/bid/ask quickly while ticker subscriptions spin up.
      this.primeTickerRestFallback();
      this.armExistingOrderBookTimeouts();
      this.armTickerRestFallbackWatchdog();
      this.ping();

      // Subscribe to new markets dynamically when market list is refreshed
      this.parent.emitter.off('markets:changed', this.onMarketsChanged);
      this.parent.emitter.on('markets:changed', this.onMarketsChanged);

      if (this.hasEverConnected) {
        this.emitWsReconnected();
      }
      this.hasEverConnected = true;
    }
  };

  private onMarketsChanged = ({ added }: { added: Array<{ id: string | number }> }) => {
    if (added.length === 0 || !this.ws || this.isDisposed) return;
    if (!this.isConnected) return;

    for (const market of added) {
      const marketId = String(market.id || '');
      if (!marketId) continue;

      const topicKey = `${this.bboTickerTopicPrefix}${marketId}`;
      const topicValue = `ticker/${marketId}`;
      this.topics[topicKey] = topicValue;

      this.send(JSON.stringify({ type: 'subscribe', channel: topicValue }));
    }
  };

  protected onClose = (event?: any): void => {
    const closeCodeValue = Number(event?.code);
    const closeCode = Number.isFinite(closeCodeValue) ? closeCodeValue : undefined;
    const closeReason = typeof event?.reason === 'string' ? event.reason : '';
    const now = virtualClock.getCurrentTimeMs();
    const timeSinceConnect = this.connectStartedAt > 0 ? now - this.connectStartedAt : Number.POSITIVE_INFINITY;
    const failedBeforeOpen = !this.openedSinceLastConnect;
    const closedBeforeFirstMessage = this.openedSinceLastConnect && !this.hasReceivedMessageSinceConnect;
    const earlyCloseWithoutMessages =
      closedBeforeFirstMessage && timeSinceConnect <= LighterPublicWebsocket.EARLY_CLOSE_WINDOW_MS;
    const closeClassification = classifyLighterPublicClose(closeCode, closeReason);
    const shouldCountAsFailure =
      failedBeforeOpen ||
      earlyCloseWithoutMessages ||
      closeCode === 1006 ||
      closeCode === 1011 ||
      closeClassification.isRateLimited ||
      closeClassification.isPolicyRestricted;

    if (shouldCountAsFailure) {
      this.failedConnections += 1;
    } else if (closeCode === 1000) {
      this.failedConnections = 0;
    } else {
      this.failedConnections = Math.max(0, this.failedConnections - 1);
    }

    if (
      !this.isWsProxyRouteEnabled() &&
      !this.connectedViaProxyRoute &&
      !this.useReadonlyDirectRoute &&
      shouldCountAsFailure &&
      this.failedConnections >= LighterPublicWebsocket.DIRECT_ROUTE_FAILURES_BEFORE_READONLY
    ) {
      this.useReadonlyDirectRoute = true;
      this.parent.log(
        `[Lighter] Public WS switching to readonly direct route after ${this.failedConnections} direct-route failures.`,
        LogSeverity.Warning,
      );
    }

    if (this.isWsProxyRouteEnabled()) {
      if (this.connectedViaProxyRoute) {
        this.directRouteFailureCount = 0;
        if (shouldCountAsFailure) {
          this.proxyRouteFailureCount += 1;
          const canProbeDirectFallback = now >= this.directFallbackProbePausedUntil;
          if (
            !this.useDirectFallbackRoute &&
            canProbeDirectFallback &&
            this.proxyRouteFailureCount >= LighterPublicWebsocket.PROXY_ROUTE_FAILURES_BEFORE_DIRECT_FALLBACK
          ) {
            this.useDirectFallbackRoute = true;
            this.parent.log(
              `[Lighter] Public WS proxy route failed ${this.proxyRouteFailureCount} times; probing direct route as a fallback.`,
              LogSeverity.Warning,
            );
          }
        } else if (this.hasReceivedMessageSinceConnect || closeCode === 1000) {
          this.proxyRouteFailureCount = 0;
          this.useDirectFallbackRoute = false;
        }
      } else {
        if (shouldCountAsFailure) {
          this.directRouteFailureCount += 1;
          this.useDirectFallbackRoute = false;
          this.directFallbackProbePausedUntil = now + LighterPublicWebsocket.PROXY_DIRECT_FALLBACK_FAILURE_COOLDOWN_MS;
          this.parent.log(
            `[Lighter] Public WS direct fallback failed ${this.directRouteFailureCount} times; returning to proxy route and pausing direct probes for ${Math.round(
              LighterPublicWebsocket.PROXY_DIRECT_FALLBACK_FAILURE_COOLDOWN_MS / 60_000,
            )} minutes.`,
            LogSeverity.Warning,
          );
        } else if (this.hasReceivedMessageSinceConnect || closeCode === 1000) {
          this.directRouteFailureCount = 0;
          this.proxyRouteFailureCount = 0;
          this.useDirectFallbackRoute = false;
        }
      }
    }

    this.openedSinceLastConnect = false;
    this.hasReceivedMessageSinceConnect = false;

    this.clearPendingSubscribeTimeouts();
    this.clearAllOrderBookStaleTimeouts();
    this.clearAllOrderBookInitialDataTimeouts();
    this.startTickerRestFallback('public websocket closed; polling tickers via REST');

    this.lastReceived = 0;
    if (this.lastReceivedTimer) {
      clearTimeout(this.lastReceivedTimer);
      this.lastReceivedTimer = undefined;
    }
    this.parent.syncSocketHealthToStore?.();

    if (closeCode !== undefined && closeCode !== 1000 && !this.parent.isDisposed) {
      this.parent.log(
        `[Lighter] Public WS closed (code ${closeCode}${closeReason ? `, reason: ${closeReason}` : ''})`,
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
      this.parent.log('[Lighter] Public WS onClose count > 100, stopping reconnection', LogSeverity.Error);
      return;
    }

    const reconnectDelay = calculateLighterPublicReconnectDelay({
      baseDelayMs: WS_MIN_RECONNECTION_DELAY,
      failureCount: this.failedConnections,
      isRateLimited: closeClassification.isRateLimited,
      rateLimitReconnectDelayMs: LighterPublicWebsocket.RATE_LIMIT_RECONNECT_DELAY_MS,
    });

    this.parent.log(`[Lighter] Public WS reconnecting in ${Math.round(reconnectDelay)}ms...`, LogSeverity.Info);

    setTimeout(() => {
      if (this.parent.isDisposed) {
        return;
      }
      this.reconnecting = false;
      this.lastCloseTime = Date.now();
      this.connectAndSubscribe(true);
    }, reconnectDelay);
  };

  private sendPayload = (payload: Record<string, unknown>): void => {
    this.send(JSON.stringify(payload));
  };

  private clearOhlcvNoDataTimeout = (topic: string) => {
    const timeoutId = this.ohlcvNoDataTimeouts[topic];
    if (!timeoutId) {
      return;
    }
    clearTimeout(timeoutId);
    delete this.ohlcvNoDataTimeouts[topic];
  };

  private clearOrderBookStaleTimeout = (topic: string) => {
    const timeoutId = this.orderBookStaleTimeouts[topic];
    if (!timeoutId) {
      return;
    }
    clearTimeout(timeoutId);
    delete this.orderBookStaleTimeouts[topic];
  };

  private clearOrderBookInitialDataTimeout = (topic: string) => {
    const timeoutId = this.orderBookInitialDataTimeouts[topic];
    if (!timeoutId) {
      return;
    }
    clearTimeout(timeoutId);
    delete this.orderBookInitialDataTimeouts[topic];
  };

  private clearAllOrderBookInitialDataTimeouts = () => {
    for (const timeoutId of Object.values(this.orderBookInitialDataTimeouts)) {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
    }
    this.orderBookInitialDataTimeouts = {};
  };

  private clearAllOrderBookStaleTimeouts = () => {
    for (const timeoutId of Object.values(this.orderBookStaleTimeouts)) {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
    }
    this.orderBookStaleTimeouts = {};
  };

  private stopOrderBookRestFallback = (topic: string) => {
    const intervalId = this.orderBookRestFallbackIntervals[topic];
    if (intervalId) {
      clearInterval(intervalId);
      delete this.orderBookRestFallbackIntervals[topic];
    }
    delete this.orderBookRestFallbackInFlight[topic];
  };

  private stopAllOrderBookRestFallback = () => {
    for (const intervalId of Object.values(this.orderBookRestFallbackIntervals)) {
      if (intervalId) {
        clearInterval(intervalId);
      }
    }
    this.orderBookRestFallbackIntervals = {};
    this.orderBookRestFallbackInFlight = {};
    this.orderBookRestFallbackCooldownUntilByTopic = {};
  };

  private isOrderBookRestFallbackCoolingDown = (topic: string): boolean =>
    virtualClock.getCurrentTimeMs() < (this.orderBookRestFallbackCooldownUntilByTopic[topic] || 0);

  private parseRestOrderBookLevels = (
    payload: any,
  ): { bids: ReadonlyArray<any>; asks: ReadonlyArray<any> } | undefined => {
    const candidate =
      payload?.order_book ??
      payload?.data?.order_book ??
      payload?.result?.order_book ??
      payload?.orderbook ??
      payload?.data?.orderbook ??
      payload?.result?.orderbook ??
      payload?.data ??
      payload?.result ??
      payload;
    if (!candidate || typeof candidate !== 'object') {
      return undefined;
    }
    const bids = Array.isArray((candidate as Record<string, unknown>).bids)
      ? ((candidate as Record<string, unknown>).bids as ReadonlyArray<any>)
      : [];
    const asks = Array.isArray((candidate as Record<string, unknown>).asks)
      ? ((candidate as Record<string, unknown>).asks as ReadonlyArray<any>)
      : [];
    if (!bids.length && !asks.length) {
      return undefined;
    }
    return { bids, asks };
  };

  private fetchAndEmitOrderBookFromRest = async (
    incomingChannel: string,
    marketId: string,
    orderBook: OrderBook,
    callback: (orderBook: OrderBook) => void,
  ) => {
    if (this.isOrderBookRestFallbackCoolingDown(incomingChannel)) {
      return;
    }
    if (this.orderBookRestFallbackInFlight[incomingChannel]) {
      return;
    }
    this.orderBookRestFallbackInFlight[incomingChannel] = true;
    try {
      const payload = await this.parent.lighterApiClient.get<any>(ENDPOINTS.ORDERBOOK, {
        params: { market_id: Number.parseInt(marketId, 10) },
        __suppressExchangeError: true,
      } as any);
      const levels = this.parseRestOrderBookLevels(payload);
      if (!levels) {
        return;
      }
      this.applyOrderBookLevels(orderBook, 'bids', levels.bids, 'snapshot');
      this.applyOrderBookLevels(orderBook, 'asks', levels.asks, 'snapshot');
      calcOrderBookTotal(orderBook);
      callback(orderBook);
    } catch (error: any) {
      const statusCode = Number(error?.response?.status ?? error?.status);
      if (statusCode === 403) {
        this.orderBookRestFallbackCooldownUntilByTopic[incomingChannel] =
          virtualClock.getCurrentTimeMs() + LighterPublicWebsocket.ORDER_BOOK_REST_FALLBACK_403_COOLDOWN_MS;
        this.parent.log(
          `[Lighter] REST orderbook fallback returned 403 for market ${marketId}; pausing fallback requests.`,
          LogSeverity.Warning,
        );
        this.stopOrderBookRestFallback(incomingChannel);
        return;
      }

      this.parent.log(
        `[Lighter] REST orderbook fallback failed for market ${marketId}: ${error?.message || String(error)}`,
        LogSeverity.Warning,
      );
    } finally {
      this.orderBookRestFallbackInFlight[incomingChannel] = false;
    }
  };

  private startOrderBookRestFallback = (
    incomingChannel: string,
    marketId: string,
    orderBook: OrderBook,
    callback: (orderBook: OrderBook) => void,
    reason: string,
  ) => {
    if (this.isOrderBookRestFallbackCoolingDown(incomingChannel)) {
      return;
    }
    if (this.orderBookRestFallbackIntervals[incomingChannel]) {
      return;
    }
    this.parent.log(
      `[Lighter] Public WS orderbook stream stalled for ${incomingChannel}; enabling REST fallback (${reason}).`,
      LogSeverity.Warning,
    );
    void this.fetchAndEmitOrderBookFromRest(incomingChannel, marketId, orderBook, callback);
    this.orderBookRestFallbackIntervals[incomingChannel] = setInterval(() => {
      if (this.isDisposed || !this.messageHandlers[incomingChannel]?.length) {
        this.stopOrderBookRestFallback(incomingChannel);
        return;
      }
      void this.fetchAndEmitOrderBookFromRest(incomingChannel, marketId, orderBook, callback);
    }, LighterPublicWebsocket.ORDER_BOOK_REST_FALLBACK_INTERVAL_MS);
  };

  private clearTickerRestFallbackWatchdog = () => {
    if (this.tickerRestFallbackWatchdog) {
      clearTimeout(this.tickerRestFallbackWatchdog);
      this.tickerRestFallbackWatchdog = undefined;
    }
  };

  private armTickerRestFallbackWatchdog = (
    delayMs = LighterPublicWebsocket.TICKER_REST_FALLBACK_WATCHDOG_MS,
    reason = 'public websocket connected but no messages; polling tickers via REST',
  ) => {
    if (this.isTickerRestFallbackCoolingDown()) {
      return;
    }

    this.clearTickerRestFallbackWatchdog();
    this.tickerRestFallbackWatchdog = setTimeout(() => {
      this.tickerRestFallbackWatchdog = undefined;
      if (this.isDisposed) {
        return;
      }
      if (!this.hasReceivedMessageSinceConnect || !this.hasHydratedTickerPrice()) {
        this.startTickerRestFallback(
          this.hasReceivedMessageSinceConnect
            ? 'public websocket is active but ticker prices are not hydrated yet; polling tickers via REST'
            : reason,
        );
      }
    }, delayMs);
  };

  private parseRestTickerItems = (payload: any): any[] => {
    const candidates = [
      payload?.tickers,
      payload?.data?.tickers,
      payload?.result?.tickers,
      payload?.market_stats,
      payload?.data?.market_stats,
      payload?.result?.market_stats,
      payload?.data,
      payload?.result,
      payload,
    ];
    for (const candidate of candidates) {
      if (Array.isArray(candidate)) {
        if (candidate.length > 0) {
          return candidate;
        }
        continue;
      }
      if (candidate && typeof candidate === 'object') {
        const values = Object.values(candidate as Record<string, unknown>);
        if (values.length > 0) {
          return values;
        }
      }
    }
    return [];
  };

  private fetchAndApplyTickerFallback = async () => {
    if (!this.parent.lighterApiClient?.get) {
      return;
    }
    if (this.isTickerRestFallbackCoolingDown()) {
      return;
    }
    if (this.tickerRestFallbackInFlight) {
      return this.tickerRestFallbackInFlight;
    }

    this.tickerRestFallbackInFlight = (async () => {
      try {
        const payload = await this.parent.lighterApiClient.get<any>(ENDPOINTS.TICKERS, {
          __suppressExchangeError: true,
        } as any);
        if (payload?.market_stats && typeof payload.market_stats === 'object') {
          this.handleMarketStatsEvent(payload);
        }
        const tickerItems = this.parseRestTickerItems(payload);
        for (const item of tickerItems) {
          if (!item || typeof item !== 'object') {
            continue;
          }
          const marketId = (item as Record<string, any>).market_id ?? (item as Record<string, any>).marketId;
          this.handleTickerEvent({
            data: item,
            ...(marketId !== undefined ? { channel: `ticker/${marketId}` } : {}),
          });
        }
        this.syncTickerRestFallbackToPriceHydration();
      } catch (error: any) {
        const statusCode = Number(error?.response?.status ?? error?.status);
        if (statusCode === 403) {
          this.tickerRestFallbackCooldownUntil =
            virtualClock.getCurrentTimeMs() + LighterPublicWebsocket.TICKER_REST_FALLBACK_403_COOLDOWN_MS;
          this.parent.log(
            '[Lighter] REST ticker fallback returned 403; pausing ticker REST fallback to avoid noisy retry loop.',
            LogSeverity.Warning,
          );
          this.clearTickerRestFallbackWatchdog();
          this.stopTickerRestFallback(false);
          if (!this.isWsProxyRouteEnabled() && !this.connectedViaProxyRoute && !this.useReadonlyDirectRoute) {
            this.useReadonlyDirectRoute = true;
            this.parent.log(
              '[Lighter] Promoting public WS to readonly direct route after REST 403 during fallback.',
              LogSeverity.Warning,
            );
            this.reconnect();
          }
          return;
        }
        this.parent.log(`[Lighter] REST ticker fallback failed: ${error?.message || String(error)}`, LogSeverity.Warning);
      }
    })().finally(() => {
      this.tickerRestFallbackInFlight = undefined;
    });

    return this.tickerRestFallbackInFlight;
  };

  private stopTickerRestFallback = (logRecovered = true) => {
    if (this.tickerRestFallbackInterval) {
      clearInterval(this.tickerRestFallbackInterval);
      this.tickerRestFallbackInterval = undefined;
      if (logRecovered) {
        this.parent.log('[Lighter] Ticker prices hydrated; disabling REST ticker fallback.', LogSeverity.Info);
      }
    }
  };

  private startTickerRestFallback = (reason: string) => {
    if (this.isTickerRestFallbackCoolingDown()) {
      return;
    }
    if (this.tickerRestFallbackInterval) {
      return;
    }
    if (!this.hasWarnedTickerRestFallback) {
      this.hasWarnedTickerRestFallback = true;
      this.parent.log(`[Lighter] ${reason}`, LogSeverity.Warning);
    }
    void this.fetchAndApplyTickerFallback();
    this.tickerRestFallbackInterval = setInterval(() => {
      if (this.isDisposed) {
        this.stopTickerRestFallback();
        return;
      }
      if (this.isConnected && this.hasReceivedMessageSinceConnect && this.hasHydratedTickerPrice()) {
        this.stopTickerRestFallback();
        return;
      }
      void this.fetchAndApplyTickerFallback();
    }, LighterPublicWebsocket.TICKER_REST_FALLBACK_INTERVAL_MS);
  };

  private clearPendingSubscribeTimeouts = () => {
    if (!this.pendingSubscribeTimeouts.length) {
      return;
    }
    this.pendingSubscribeTimeouts.forEach((timeoutId) => clearTimeout(timeoutId));
    this.pendingSubscribeTimeouts = [];
  };

  private armOrderBookStaleTimeout = (topic: string, subscribeChannel: string) => {
    this.clearOrderBookStaleTimeout(topic);

    this.orderBookStaleTimeouts[topic] = setTimeout(() => {
      delete this.orderBookStaleTimeouts[topic];
      if (this.isDisposed || this.reconnecting) {
        return;
      }

      this.attemptOrderBookRecovery(
        topic,
        subscribeChannel,
        `no order book messages after ${LighterPublicWebsocket.ORDER_BOOK_STALE_TIMEOUT_MS}ms`,
      );
    }, LighterPublicWebsocket.ORDER_BOOK_STALE_TIMEOUT_MS);
  };

  private isOrderBookTopic = (topic: string): boolean =>
    topic.startsWith('order_book:') || topic.startsWith('order_book/');

  private armExistingOrderBookTimeouts = () => {
    for (const [incomingTopic, subscribedTopicValue] of Object.entries(this.topics)) {
      if (!this.isOrderBookTopic(incomingTopic)) {
        continue;
      }
      const subscribeChannel = Array.isArray(subscribedTopicValue) ? subscribedTopicValue[0] : subscribedTopicValue;
      if (!subscribeChannel) {
        continue;
      }
      this.armOrderBookStaleTimeout(incomingTopic, subscribeChannel);
    }
  };

  private armOhlcvNoDataTimeout = (topic: string) => {
    this.clearOhlcvNoDataTimeout(topic);

    this.ohlcvNoDataTimeouts[topic] = setTimeout(() => {
      delete this.ohlcvNoDataTimeouts[topic];
      if (this.isDisposed || this.reconnecting) {
        return;
      }
      this.attemptOhlcvRecovery(
        topic,
        `no OHLCV messages after ${LighterPublicWebsocket.OHLCV_DIRECT_ROUTE_NO_DATA_TIMEOUT_MS}ms`,
      );
    }, LighterPublicWebsocket.OHLCV_DIRECT_ROUTE_NO_DATA_TIMEOUT_MS);
  };

  private attemptOhlcvRecovery = (topic: string, reason: string): void => {
    const now = virtualClock.getCurrentTimeMs();
    const shouldAttemptRecovery = shouldAttemptLighterOhlcvRecovery({
      hasTopicHandlers: Boolean(this.messageHandlers[topic]?.length),
      nowMs: now,
      lastRecoveryReconnectAtMs: this.lastOhlcvRecoveryReconnectAt,
      recoveryCooldownMs: LighterPublicWebsocket.OHLCV_RECOVERY_RECONNECT_COOLDOWN_MS,
    });
    if (!shouldAttemptRecovery) {
      return;
    }

    const shouldPromoteToProxy = shouldPromoteLighterOhlcvRecoveryToProxy({
      isWsProxyRouteEnabled: this.isWsProxyRouteEnabled(),
      useProxyRoute: !this.useDirectFallbackRoute,
      connectedViaProxyRoute: this.connectedViaProxyRoute,
    });
    if (shouldPromoteToProxy) {
      this.useProxyRoute = true;
      this.useDirectFallbackRoute = false;
      this.parent.log(
        `[Lighter] Public WS OHLCV recovery promoting route to proxy for ${topic} (${reason}).`,
        LogSeverity.Warning,
      );
    } else {
      this.parent.log(
        `[Lighter] Public WS OHLCV recovery probing via reconnect for ${topic} (${reason}).`,
        LogSeverity.Warning,
      );
    }

    this.lastOhlcvRecoveryReconnectAt = now;
    this.reconnect();
  };

  maybeRecoverOhlcvStream = (opts: Pick<OHLCVOptions, 'symbol' | 'interval'>, reason = 'stale OHLCV stream') => {
    const topic = buildLighterCandleTopic(INTERVAL[opts.interval], opts.symbol);
    this.attemptOhlcvRecovery(topic, reason);
  };

  private attemptOrderBookRecovery = (topic: string, subscribeChannel: string, reason: string) => {
    if (!this.messageHandlers[topic]?.length) {
      return;
    }

    const now = virtualClock.getCurrentTimeMs();
    const lastRecoveryAt = this.lastOrderBookRecoveryAtByTopic[topic] || 0;
    if (now - lastRecoveryAt < LighterPublicWebsocket.ORDER_BOOK_RECOVERY_COOLDOWN_MS) {
      this.armOrderBookStaleTimeout(topic, subscribeChannel);
      return;
    }

    this.lastOrderBookRecoveryAtByTopic[topic] = now;
    const recoveryAttempt = (this.orderBookRecoveryAttempts[topic] || 0) + 1;
    this.orderBookRecoveryAttempts[topic] = recoveryAttempt;
    const shouldReconnect = !this.isConnected || recoveryAttempt >= 2;

    if (shouldReconnect) {
      this.parent.log(
        `[Lighter] Public WS order book recovery reconnecting for ${topic} (${reason}, attempt ${recoveryAttempt}).`,
        LogSeverity.Warning,
      );
      this.reconnect();
      return;
    }

    this.parent.log(
      `[Lighter] Public WS order book recovery re-subscribing ${topic} (${reason}, attempt ${recoveryAttempt}).`,
      LogSeverity.Warning,
    );
    try {
      this.sendPayload({ type: 'unsubscribe', channel: subscribeChannel });
      this.sendPayload({ type: 'subscribe', channel: subscribeChannel });
    } catch (error: any) {
      this.parent.log(
        `[Lighter] Public WS order book re-subscribe failed for ${topic}: ${error?.message || String(error)}`,
        LogSeverity.Warning,
      );
      this.reconnect();
      return;
    }

    this.armOrderBookStaleTimeout(topic, subscribeChannel);
  };

  ping = () => {
    if (!this.isDisposed) {
      this.pingAt = performance.now();
      this.sendPayload({ type: 'ping', timestamp: virtualClock.getCurrentTimeMs() });
    }
  };

  subscribe = () => {
    this.clearPendingSubscribeTimeouts();
    const topics = flatten(Object.values(this.topics))
      .map((topic) => String(topic || '').trim())
      .filter((topic) => topic.length > 0);
    const uniqueTopics = Array.from(new Set(topics));

    uniqueTopics.forEach((topic, index) => {
      const sendSubscribe = () => {
        if (this.isDisposed || !this.isConnected || this.reconnecting) {
          return;
        }
        try {
          this.sendPayload({ type: 'subscribe', channel: topic });
        } catch (error: any) {
          this.parent.log(
            `[Lighter] Public WS delayed subscribe failed for ${topic}: ${error?.message || String(error)}`,
            LogSeverity.Warning,
          );
        }
      };

      if (index < LighterPublicWebsocket.MAX_IMMEDIATE_SUBSCRIBE_MESSAGES) {
        sendSubscribe();
        return;
      }

      const delayMs =
        (index - LighterPublicWebsocket.MAX_IMMEDIATE_SUBSCRIBE_MESSAGES + 1) *
        LighterPublicWebsocket.SUBSCRIBE_STAGGER_DELAY_MS;
      const timeoutId = setTimeout(() => {
        this.pendingSubscribeTimeouts = this.pendingSubscribeTimeouts.filter((id) => id !== timeoutId);
        sendSubscribe();
      }, delayMs);
      this.pendingSubscribeTimeouts.push(timeoutId);
    });
  };

  private syncBboTickerTopics = () => {
    for (const key of Object.keys(this.topics)) {
      if (key.startsWith(this.bboTickerTopicPrefix)) {
        delete this.topics[key];
      }
    }

    for (const market of this.parent.store.markets) {
      const marketId = String(market.id || '');
      if (!marketId) {
        continue;
      }
      this.topics[`${this.bboTickerTopicPrefix}${marketId}`] = `ticker/${marketId}`;
    }
  };

  onMessage = ({ data }: MessageEvent) => {
    if (!this.isDisposed) {
      if (!this.hasReceivedMessageSinceConnect) {
        this.hasReceivedMessageSinceConnect = true;
        this.failedConnections = 0;
        if (!this.connectedViaProxyRoute && this.directRouteFailureCount > 0) {
          this.directRouteFailureCount = 0;
        }
      }
      this.trackLastReceived();
      const handlers = Object.entries(this.messageHandlers);

      if (data.includes('type":"ping')) {
        this.sendPayload({ type: 'pong', timestamp: virtualClock.getCurrentTimeMs() });
      }

      for (const [topic, handler] of handlers) {
        if (shouldDispatchLighterPublicMessage(topic, data)) {
          this.recordMessageHandlerStats(topic);
          const json = jsonParse(data);
          for (const cb of handler) {
            if (json) cb(json);
          }
        }
      }
      this.syncTickerRestFallbackToPriceHydration(
        'public websocket is active but ticker prices are not hydrated yet; polling tickers via REST',
      );
    }
  };

  private toOrderBookLevel = (level: any) => ({
    price: parseFloat(level.price || level[0]),
    amount: parseFloat(level.size || level[1]),
    total: 0,
  });

  private applyOrderBookLevels = (
    orderBook: OrderBook,
    side: 'bids' | 'asks',
    levels: ReadonlyArray<any>,
    mode: 'snapshot' | 'delta',
  ) => {
    if (mode === 'snapshot') {
      orderBook[side] = [];
      levels.forEach((level) => {
        orderBook[side].push(this.toOrderBookLevel(level));
      });
      return;
    }

    levels.forEach((level) => {
      updateOrderBook(orderBook, side, this.toOrderBookLevel(level));
    });
  };

  handleTickerEvent = (json: Record<string, any>) => {
    const tickerData = json.data || json.ticker || json;

    if (!tickerData) return;

    const channel = String(json.channel || '');
    const channelMarketId = extractLighterTickerMarketIdFromChannel(channel);
    const symbol = String(
      tickerData.symbol || tickerData.marketSymbol || tickerData.market_symbol || tickerData.s || json.symbol || '',
    );

    const ticker =
      this.parent.store.tickers.find((t) => (symbol ? t.symbol === symbol || t.id === symbol : false)) ||
      this.parent.store.tickers.find((t) => (channelMarketId ? t.id === channelMarketId : false));

    if (ticker) {
      const update: Partial<Writable<Ticker>> = {};
      const last = parseLighterWsNumber(
        tickerData.lastPrice,
        tickerData.last_price,
        tickerData.lastTradePrice,
        tickerData.last_trade_price,
        tickerData.last,
        tickerData.price,
      );
      const mark = parseLighterWsNumber(tickerData.markPrice, tickerData.mark_price, tickerData.mark);
      const index = parseLighterWsNumber(tickerData.indexPrice, tickerData.index_price, tickerData.index);
      const bid = parseLighterWsNumber(
        tickerData.bidPrice,
        tickerData.bid_price,
        tickerData.bestBidPrice,
        tickerData.best_bid_price,
        tickerData.bestBid,
        tickerData.best_bid,
        tickerData.b,
        tickerData.bid?.[0],
        tickerData.b?.[0],
        tickerData.b?.price,
        tickerData.bid?.price,
        parseLighterWsBookSidePrice(tickerData.bid),
        parseLighterWsBookSidePrice(tickerData.b),
      );
      const ask = parseLighterWsNumber(
        tickerData.askPrice,
        tickerData.ask_price,
        tickerData.bestAskPrice,
        tickerData.best_ask_price,
        tickerData.bestAsk,
        tickerData.best_ask,
        tickerData.a,
        tickerData.ask?.[0],
        tickerData.a?.[0],
        tickerData.a?.price,
        tickerData.ask?.price,
        parseLighterWsBookSidePrice(tickerData.ask),
        parseLighterWsBookSidePrice(tickerData.a),
      );
      const volume = parseLighterWsNumber(tickerData.volume24h, tickerData.volume_24h, tickerData.volume);
      const quoteVolume = parseLighterWsNumber(
        tickerData.quoteVolume24h,
        tickerData.quote_volume_24h,
        tickerData.quoteVolume,
        tickerData.quote_volume,
      );
      const openInterest = parseLighterWsNumber(
        tickerData.openInterest,
        tickerData.open_interest,
        tickerData.oi,
        tickerData.openInterestUsd,
        tickerData.open_interest_usd,
      );
      const fundingRate = parseLighterWsNumber(
        tickerData.fundingRate,
        tickerData.funding_rate,
        tickerData.currentFundingRate,
      );
      const priceChange24h = parseLighterWsNumber(tickerData.priceChange24h, tickerData.price_change_24h);

      if (last !== undefined) update.last = last;
      if (mark !== undefined) update.mark = mark;
      if (index !== undefined) update.index = index;
      if (mark === undefined && index !== undefined) {
        update.mark = index;
      }
      if (priceChange24h !== undefined) {
        update.percentage = priceChange24h;
      }
      if (openInterest !== undefined) update.openInterest = openInterest;
      if (fundingRate !== undefined) update.fundingRate = normalizeLighterFundingRate(fundingRate);
      if (volume !== undefined) update.volume = volume;
      if (quoteVolume !== undefined) update.quoteVolume = quoteVolume;
      if (bid !== undefined) update.bid = bid;
      if (ask !== undefined) update.ask = ask;

      this.parent.store.updateTicker(ticker, update);

      // Update position unrealized PnL
      const resolvedSymbol = symbol || ticker.symbol;
      const referencePrice = update.mark || update.last;
      if (referencePrice) {
        const matchingPositions = this.store.positions.filter((p) => p.symbol === resolvedSymbol && p.contracts !== 0);
        for (const position of matchingPositions) {
          const market = this.store.markets.find((m) => m.symbol === resolvedSymbol);
          if (!market || market.isInverse) return;
          const entryValue = position.contracts * position.entryPrice;
          const currentValue = position.contracts * referencePrice;
          const directionMult = position.side === PositionSide.Long ? 1 : -1;
          const unrealizedPnl = directionMult * (currentValue - entryValue);
          this.store.updatePosition(position, { unrealizedPnl });
        }
      }
    }
  };

  handleTickersEvent = (json: Record<string, any>) => {
    const tickers = json.data;

    if (!Array.isArray(tickers)) return;

    for (const tickerData of tickers) {
      this.handleTickerEvent({ data: tickerData });
    }
  };

  handleMarketStatsEvent = (json: Record<string, any>) => {
    const marketStatsById = (json.market_stats ?? json.data?.market_stats) as Record<string, any> | undefined;
    if (!marketStatsById) return;

    for (const stats of Object.values(marketStatsById)) {
      const symbol = String(stats?.symbol || '');
      const marketId = stats?.market_id ?? stats?.marketId;

      const ticker = this.parent.store.tickers.find(
        (t) => (symbol ? t.symbol === symbol : false) || (marketId !== undefined ? t.id === String(marketId) : false),
      );
      if (!ticker) continue;

      const mark = parseLighterWsNumber(stats.mark_price, stats.markPrice, stats.mark);
      const index = parseLighterWsNumber(stats.index_price, stats.indexPrice, stats.index);
      const last = parseLighterWsNumber(stats.last_trade_price, stats.lastTradePrice, stats.last, stats.price);
      const dailyBaseVolume = parseLighterWsNumber(stats.daily_base_token_volume, stats.volume);
      const dailyQuoteVolume = parseLighterWsNumber(stats.daily_quote_token_volume, stats.quote_volume);
      const openInterest = parseLighterWsNumber(stats.open_interest, stats.openInterest);
      const fundingRate = parseLighterWsNumber(stats.current_funding_rate, stats.funding_rate);
      const percentage = parseLighterWsNumber(stats.daily_price_change);
      const bid = parseLighterWsNumber(stats.best_bid_price, stats.bid_price, stats.bid);
      const ask = parseLighterWsNumber(stats.best_ask_price, stats.ask_price, stats.ask);

      this.parent.store.updateTicker(ticker, {
        ...(mark !== undefined ? { mark } : {}),
        ...(index !== undefined ? { index } : {}),
        ...(last !== undefined ? { last } : {}),
        ...(dailyBaseVolume !== undefined ? { volume: dailyBaseVolume } : {}),
        ...(dailyQuoteVolume !== undefined ? { quoteVolume: dailyQuoteVolume } : {}),
        ...(openInterest !== undefined ? { openInterest } : {}),
        ...(fundingRate !== undefined ? { fundingRate: normalizeLighterFundingRate(fundingRate) } : {}),
        ...(percentage !== undefined ? { percentage } : {}),
        ...(bid !== undefined ? { bid } : {}),
        ...(ask !== undefined ? { ask } : {}),
      });
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    const topic = buildLighterCandleTopic(INTERVAL[opts.interval], opts.symbol);
    const secondaryTopic = buildLighterTradesTopic(opts.symbol);

    let lastCandle: Candle | undefined = undefined;

    const parser = ({ data }: Data) => {
      if (!data || !data.candle) return;

      this.clearOhlcvNoDataTimeout(topic);

      const candle = data.candle;
      const timestamp = candle.timestamp / 1000;

      if (lastCandle && lastCandle.timestamp > timestamp) {
        return;
      }

      lastCandle = {
        timestamp,
        open: parseFloat(candle.open),
        high: parseFloat(candle.high),
        low: parseFloat(candle.low),
        close: parseFloat(candle.close),
        volume: parseFloat(candle.volume),
      };
      callback(lastCandle);
    };

    const secondaryParser = ({ data }: Data) => {
      if (!data || !data.trade) return;
      if (!lastCandle) return;

      const trade = data.trade;
      const timestamp = trade.timestamp / 1000;
      const candleStart = getCandleStartTime(opts.interval, timestamp);
      const price = parseFloat(trade.price);

      if (lastCandle.timestamp > candleStart) {
        return;
      }

      if (lastCandle.timestamp < candleStart) {
        lastCandle = {
          timestamp: candleStart,
          open: lastCandle.close,
          high: lastCandle.close,
          low: lastCandle.close,
          close: lastCandle.close,
          volume: 0,
        };
      }

      lastCandle = {
        ...lastCandle,
        high: Math.max(lastCandle.high, price),
        low: Math.min(lastCandle.low, price),
        close: price,
        volume: add(lastCandle.volume, divide(parseFloat(trade.size), price)),
      };
      callback(lastCandle);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), parser];
          if (!this.parent.options.publicDataOnly) {
            this.messageHandlers[secondaryTopic] = [...(this.messageHandlers[secondaryTopic] || []), secondaryParser];
          }

          const payload = { type: 'subscribe', channel: topic };
          this.sendPayload(payload);
          this.armOhlcvNoDataTimeout(topic);

          if (!this.parent.options.publicDataOnly) {
            const secondaryPayload = { type: 'subscribe', channel: secondaryTopic };
            this.sendPayload(secondaryPayload);
          }

          // Store subscribed topic to re-subscribe on reconnect
          this.topics[topic] = topic;
          if (!this.parent.options.publicDataOnly) {
            this.topics[secondaryTopic] = secondaryTopic;
          }
        }
      } else {
        setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== parser)];
      this.clearOhlcvNoDataTimeout(topic);

      this.messageHandlers[secondaryTopic] = [
        ...(this.messageHandlers[secondaryTopic] || []).filter((f) => f !== secondaryParser),
      ];

      if (!this.messageHandlers[topic].length) {
        delete this.topics[topic];
        delete this.topics[secondaryTopic];
      }

      if (this.isConnected && !this.messageHandlers[topic].length) {
        const payload = { type: 'unsubscribe', channel: topic };
        this.sendPayload(payload);
      }

      if (this.isConnected && !this.messageHandlers[secondaryTopic].length) {
        const payload = { type: 'unsubscribe', channel: secondaryTopic };
        this.cleanupMessageHandlerStats(topic);
        this.cleanupMessageHandlerStats(secondaryTopic);
        this.sendPayload(payload);
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    let timeoutId: NodeJS.Timeout | null = null;
    const market = this.parent.store.marketsBySymbol[symbol];
    if (!market) {
      this.parent.log(`[Lighter] listenOrderBook market not found for symbol: ${symbol}`);
      return () => {};
    }
    const marketId = String(market.id);
    const incomingChannel = `order_book:${marketId}`;
    const subscribeChannel = `order_book/${marketId}`;
    const orderBook: OrderBook = { bids: [], asks: [] };
    let receivedAnyOrderBookUpdate = false;

    const parser = (data: Data) => {
      const obData = data.order_book || data.data;
      if (!obData) return;

      receivedAnyOrderBookUpdate = true;
      this.clearOrderBookInitialDataTimeout(incomingChannel);
      this.stopOrderBookRestFallback(incomingChannel);
      delete this.orderBookRestFallbackCooldownUntilByTopic[incomingChannel];
      this.clearOrderBookStaleTimeout(incomingChannel);
      this.orderBookRecoveryAttempts[incomingChannel] = 0;

      const messageType = String(data.type || '');
      const bids = Array.isArray(obData.bids) ? obData.bids : [];
      const asks = Array.isArray(obData.asks) ? obData.asks : [];
      const shouldResetBook =
        messageType.includes('subscribed') ||
        messageType.includes('snapshot') ||
        (orderBook.bids.length === 0 && orderBook.asks.length === 0);

      if (shouldResetBook) {
        this.applyOrderBookLevels(orderBook, 'bids', bids, 'snapshot');
        this.applyOrderBookLevels(orderBook, 'asks', asks, 'snapshot');
      } else {
        this.applyOrderBookLevels(orderBook, 'bids', bids, 'delta');
        this.applyOrderBookLevels(orderBook, 'asks', asks, 'delta');
      }

      calcOrderBookTotal(orderBook);

      callback(orderBook);
      this.armOrderBookStaleTimeout(incomingChannel, subscribeChannel);
    };

    const waitForConnectedAndSubscribe = () => {
      if (this.isConnected) {
        if (!this.isDisposed) {
          this.messageHandlers[incomingChannel] = [...(this.messageHandlers[incomingChannel] || []), parser];

          const payload = { type: 'subscribe', channel: subscribeChannel };
          this.sendPayload(payload);
          // Prime order book immediately instead of waiting for initial-data timeout.
          void this.fetchAndEmitOrderBookFromRest(incomingChannel, marketId, orderBook, callback);
          this.armOrderBookStaleTimeout(incomingChannel, subscribeChannel);
          this.clearOrderBookInitialDataTimeout(incomingChannel);
          this.orderBookInitialDataTimeouts[incomingChannel] = setTimeout(() => {
            delete this.orderBookInitialDataTimeouts[incomingChannel];
            if (this.isDisposed || receivedAnyOrderBookUpdate || !this.messageHandlers[incomingChannel]?.length) {
              return;
            }
            this.startOrderBookRestFallback(
              incomingChannel,
              marketId,
              orderBook,
              callback,
              'no initial websocket snapshot',
            );
          }, LighterPublicWebsocket.ORDER_BOOK_INITIAL_DATA_TIMEOUT_MS);

          // Store subscribed topic to re-subscribe on reconnect
          this.topics[incomingChannel] = subscribeChannel;
        }
      } else {
        timeoutId = setTimeout(() => waitForConnectedAndSubscribe(), 100);
      }
    };

    waitForConnectedAndSubscribe();

    return () => {
      this.messageHandlers[incomingChannel] = [
        ...(this.messageHandlers[incomingChannel] || []).filter((f) => f !== parser),
      ];

      if (!this.messageHandlers[incomingChannel].length) {
        delete this.topics[incomingChannel];
      }

      this.cleanupMessageHandlerStats(incomingChannel);
      this.clearOrderBookInitialDataTimeout(incomingChannel);
      this.stopOrderBookRestFallback(incomingChannel);

      orderBook.asks = [];
      orderBook.bids = [];

      if (timeoutId) {
        clearTimeout(timeoutId);
        timeoutId = null;
      }

      if (this.isConnected && !this.messageHandlers[incomingChannel].length) {
        this.clearOrderBookStaleTimeout(incomingChannel);
        delete this.orderBookRecoveryAttempts[incomingChannel];
        delete this.lastOrderBookRecoveryAtByTopic[incomingChannel];
        const payload = { type: 'unsubscribe', channel: subscribeChannel };
        this.sendPayload(payload);
      }
    };
  };

  dispose = () => {
    this.clearTickerRestFallbackWatchdog();
    this.stopTickerRestFallback();
    this.clearPendingSubscribeTimeouts();
    for (const timeoutId of Object.values(this.ohlcvNoDataTimeouts)) {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
    }
    this.ohlcvNoDataTimeouts = {};
    this.clearAllOrderBookInitialDataTimeouts();
    this.clearAllOrderBookStaleTimeouts();
    this.stopAllOrderBookRestFallback();
    this.orderBookRecoveryAttempts = {};
    this.lastOrderBookRecoveryAtByTopic = {};
    super.dispose();
  };
}
