import type { Axios } from 'axios';
import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';
import type { AccountGreeks, ExchangeName, OptionChainQuery, OptionChainRow } from '../../types';
import type { ClientOrderIdRule } from '../clientOrderId';
import type { BybitIncomeContext } from './bybit.income';
import type { BybitPlaceOrderOpts } from './bybit.order-params';

import rateLimit from 'axios-rate-limit';
import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import debounce from 'lodash/debounce';
import omit from 'lodash/omit';
import orderBy from 'lodash/orderBy';
import times from 'lodash/times';
import { nanoid } from 'nanoid';
import { forEachSeries, mapSeries } from 'p-iteration';

import { sleep } from '@tealstreet/sleep/src/sleep';
import { aggregateBybitClosedPnl } from '../../journal/bybit.position-adapter';
import {
  Balance,
  Candle,
  ClosedPnlEntry,
  CoinBalance,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalOrder,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  OHLCVSOptions,
  Order,
  OrderBook,
  OrderHistoryOptions,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  Timeframe,
  UpdateOrderOpts,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { v } from '../../utils/get-key';
import { inverseObj } from '../../utils/inverseObj';
import { loop } from '../../utils/loop';
import { mergeUpdatesUsingLastUpdate } from '../../utils/mergeUpdatesUsingLastUpdate';
import { omitUndefined } from '../../utils/omitUndefined';
import { adjust, subtract } from '../../utils/safe-math';
import { splitTimeRange } from '../../utils/splitTimeRange';
import { isUnknownOutcome } from '../../utils/unknownSendOutcome';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { patternClientOrderIdRule } from '../clientOrderId';
import { createAPI } from './bybit.api';
import { mapBybitClosedPnl } from './bybit.closed-pnl';
import { bybitFeeOverlay } from './bybit.fee-overlay';
import { fetchBybitIncomeHistory, fetchBybitIncomeHistoryRange } from './bybit.income';
import { isBybitInverseSymbol, positionUpnlInQuote } from './bybit.inverse';
import {
  buildOptionChain,
  isBybitOptionSymbol,
  mapBybitCoinGreeks,
  mapBybitOptionGreeks,
  mapBybitOptionInstrument,
  mapBybitOptionTicker,
  optionExpiries,
} from './bybit.options';
import { mapBybitHistoricalOrder } from './bybit.order-history';
import { getBybitOrderExecutionParams, hasBybitSingleOrderParams } from './bybit.order-params';
import { BYBIT_RATE_LIMIT_TABLE, BybitAccountType, BybitProductType, BybitVipLevel } from './bybit.rateLimits';
import { getBybitResponseMeta } from './bybit.response';
import { BYBIT_BRAZIL_SITE_ID } from './bybit.site';
import {
  BASE_URL,
  BASE_WS_URL,
  BybitApiKeyInfo,
  DEMO_URL,
  DEMO_WS_URL,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  INTERVAL,
  ORDER_HISTORY_MAX_WINDOW_MS,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TIME_IN_FORCE,
  ORDER_TIME_IN_FORCE_OUTBOUND,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  POSITION_SIDE,
  POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE,
} from './bybit.types';
import { BybitPrivateWebsocket } from './bybit.ws-private';
import { BybitPublicWebsocket } from './bybit.ws-public';
import { syncUnifiedBalanceUpnlFromPositions } from './syncUnifiedBalanceUpnlFromPositions';

const REINIT_RETRY_MS = 15_000;
const REINIT_RETRY_MAX_MS = 5 * 60_000;

const MAX_CLOSED_PNL_PAGES = 20;

const INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1_000;
const PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS = 250;
const PRIVATE_SNAPSHOT_REFRESH_MAX_WAIT_MS = 1_200;
const BYBIT_POSITION_TPSL_MODES = new Set(['Full', 'Partial']);
const BYBIT_LEVERAGE_NOT_MODIFIED_CODE = 110043;
const BYBIT_PRELAUNCH_MARKETS_STARTUP_GRACE_MS = 1_500;
const BYBIT_TRADING_MARKETS_RETRY_DELAY_MS = 1_000;
export const BYBIT_MARKETS_FAILURE_SENTRY_COOLDOWN_MS = 5 * 60_000;
const BYBIT_TRADING_AGREEMENT_RET_CODES = new Set([110123, 110125]);
// How long a just-placed order is shielded from wholesale REST order-snapshot
// replacement while no REST/WS sighting has confirmed it. Covers api-demo's
// order-indexing lag with margin; short enough that a phantom row for an order
// that filled before its first sighting cannot linger.
const RECENTLY_PLACED_ORDER_GRACE_MS = 8_000;
const REDUCE_ONLY_LIMIT_RECONCILE_DELAYS_MS = [1_000, 3_000, RECENTLY_PLACED_ORDER_GRACE_MS + 250];
const RETIRED_ORDER_SYMBOL_TTL_MS = 60_000;
const FILL_RECONCILIATION_MAX_PAGES = 20;

export type BybitCreateOrderResponse = {
  data: any;
  payload: Record<string, any>;
  terminal?: boolean;
  recorded?: boolean;
  recoveredOrder?: Record<string, any>;
  reconciled?: boolean;
  authoritativeSeen?: boolean;
};

/**
 * Only the message survives structured-clone out of the exchange worker, so the
 * reason has to ride in it or Sentry sees nothing but a constant string. Kept to
 * a fixed vocabulary: the raw retMsg carries proxy IPs and ban timestamps, which
 * would split one issue into thousands.
 */
export const describeBybitMarketsFailure = (cause: unknown): string => {
  return getBybitMarketsFailureTelemetry(cause).reason;
};

const stringOrUndefined = (value: unknown): string | undefined =>
  typeof value === 'string' && value ? value : undefined;

const numberOrUndefined = (value: unknown): number | undefined => {
  const numeric = typeof value === 'number' ? value : typeof value === 'string' ? Number(value) : NaN;
  return Number.isFinite(numeric) ? numeric : undefined;
};

const stringFromUnknown = (value: unknown): string | undefined =>
  typeof value === 'string' && value.trim() ? value.trim() : undefined;

const getBybitErrorField = (payload: unknown, ...keys: string[]): unknown => {
  if (!payload || typeof payload !== 'object') return undefined;
  const record = payload as Record<string, unknown>;
  for (const key of keys) {
    if (record[key] !== undefined && record[key] !== null) {
      return record[key];
    }
  }
  return undefined;
};

export const formatBybitTradingAgreementError = (payload: unknown, exchangeName?: ExchangeName): string | undefined => {
  const retCode = numberOrUndefined(getBybitErrorField(payload, 'retCode', 'ret_code', 'code'));
  const retMsg =
    stringFromUnknown(payload) ?? stringFromUnknown(getBybitErrorField(payload, 'retMsg', 'ret_msg', 'message', 'msg'));
  const normalized = retMsg?.toLowerCase() ?? '';
  const isAgreementError =
    (retCode !== undefined && BYBIT_TRADING_AGREEMENT_RET_CODES.has(retCode)) ||
    (normalized.includes('trading terms') && (normalized.includes('must agree') || normalized.includes('must sign'))) ||
    (normalized.includes('agreement') && normalized.includes('trading this contract'));

  if (!isAgreementError) return undefined;

  const action =
    exchangeName === 'hyrotrader'
      ? 'For HyroTrader, wait briefly and retry; if it persists, HyroTrader must sign the agreement on the master Bybit account.'
      : 'Sign the agreement from the Bybit master account before retrying.';
  const original = retMsg
    ? `${retMsg.replace(/[.\s]+$/, '')}${retCode !== undefined ? ` (retCode ${retCode})` : ''}`
    : retCode !== undefined
      ? `retCode ${retCode}`
      : undefined;

  return `Bybit Trading Terms not accepted for this contract. ${action}${
    original ? ` Original Bybit error: ${original}.` : ''
  }`;
};

export const getBybitMarketsFailureTelemetry = (cause: unknown) => {
  const err = cause as
    | {
        response?: {
          status?: number;
          data?: { retMsg?: unknown; ret_msg?: unknown; retCode?: unknown; ret_code?: unknown };
        };
        config?: { url?: unknown; method?: unknown };
        code?: unknown;
        message?: unknown;
      }
    | null
    | undefined;

  const retMsg = String(err?.response?.data?.retMsg ?? err?.response?.data?.ret_msg ?? '').toLowerCase();
  const message = String(err?.message ?? '').toLowerCase();
  const httpStatus = err?.response?.status;
  const errorCode = stringOrUndefined(err?.code);
  const retCode = numberOrUndefined(err?.response?.data?.retCode ?? err?.response?.data?.ret_code);
  const endpoint = stringOrUndefined(err?.config?.url);
  const method = stringOrUndefined(err?.config?.method)?.toUpperCase();

  let reason = 'unknown';
  if (retMsg.includes('too many') || retMsg.includes('rate limit') || httpStatus === 429) reason = 'rate-limited';
  else if (retMsg.includes('banned') || httpStatus === 403) reason = 'blocked';
  else if (message.includes('timeout') || message.includes('timed out') || errorCode === 'ECONNABORTED')
    reason = 'timeout';
  else if (typeof httpStatus === 'number') reason = `HTTP ${httpStatus}`;
  else if (message.includes('network')) reason = 'network';

  return {
    reason,
    httpStatus,
    errorCode,
    retCode,
    endpoint,
    method,
  };
};

const isRetryableBybitTradingMarketsFailure = (cause: unknown): boolean => {
  const { reason, httpStatus } = getBybitMarketsFailureTelemetry(cause);
  return (
    reason === 'timeout' ||
    reason === 'network' ||
    httpStatus === 408 ||
    (httpStatus !== undefined && httpStatus >= 500 && httpStatus < 600)
  );
};

class BybitTradingMarketsRequestError extends Error {
  readonly safeCexSentryReportKey: string;
  readonly safeCexSentryReportCooldownMs = BYBIT_MARKETS_FAILURE_SENTRY_COOLDOWN_MS;
  readonly safeCexSentryTags: Record<string, string | number>;
  readonly safeCexSentryExtra: Record<string, string | number>;

  constructor(
    readonly cause: unknown,
    retryCount = 0,
  ) {
    const telemetry = getBybitMarketsFailureTelemetry(cause);
    super(`Failed to fetch Bybit Trading markets (${telemetry.reason})`);
    this.name = 'BybitTradingMarketsRequestError';
    this.safeCexSentryReportKey = `bybit:trading-markets:${telemetry.reason}`;
    this.safeCexSentryTags = {
      worker_error_family: 'bybit_markets_startup',
      bybit_markets_failure_reason: telemetry.reason,
      bybit_markets_retry_count: retryCount,
      ...(telemetry.httpStatus !== undefined ? { bybit_markets_http_status: telemetry.httpStatus } : {}),
      ...(telemetry.errorCode ? { bybit_markets_error_code: telemetry.errorCode } : {}),
      ...(telemetry.retCode !== undefined ? { bybit_markets_ret_code: telemetry.retCode } : {}),
    };
    this.safeCexSentryExtra = {
      bybitMarketsFailureReason: telemetry.reason,
      bybitMarketsRetryCount: retryCount,
      ...(telemetry.httpStatus !== undefined ? { bybitMarketsHttpStatus: telemetry.httpStatus } : {}),
      ...(telemetry.errorCode ? { bybitMarketsErrorCode: telemetry.errorCode } : {}),
      ...(telemetry.retCode !== undefined ? { bybitMarketsRetCode: telemetry.retCode } : {}),
      ...(telemetry.endpoint ? { bybitMarketsEndpoint: telemetry.endpoint } : {}),
      ...(telemetry.method ? { bybitMarketsMethod: telemetry.method } : {}),
    };
  }
}

const isBybitPositionTpslMode = (mode: unknown) => typeof mode === 'string' && BYBIT_POSITION_TPSL_MODES.has(mode);

// An order that filled or was already cancelled between our decision to cancel
// and the request landing. The chaser races this constantly: it cancels at the
// touch, and the order can complete in the round trip. Treating it as a hard
// failure made the chaser stop itself and abandon the chase.
//
// `HyrotraderExchange` extends this via `isAlreadyGoneCancelMessage` to also
// accept the simulator's "Order does not exist or is no longer open" phrasing
// without regressing the bybit/bybitdemo wording.
const isBybitOrderAlreadyGoneCancelMessage = (message: unknown) => {
  const normalized = `${message || ''}`.toLowerCase();
  return normalized.includes('order not exists') || normalized.includes('too late to cancel');
};

const isBybitOrderNotModifiedMessage = (message: unknown) => `${message || ''}`.toLowerCase().includes('not modified');

const isBybitLeverageNotModified = (data: Record<string, any>) => {
  const retCode = Number(v(data, 'retCode'));
  const retMsg = `${v(data, 'retMsg') || ''}`.toLowerCase();
  return (
    retCode === BYBIT_LEVERAGE_NOT_MODIFIED_CODE || (retMsg.includes('leverage') && retMsg.includes('not modified'))
  );
};

const getBybitPayloadMessage = (payload: unknown): string | undefined =>
  stringFromUnknown(payload) ?? stringFromUnknown(getBybitErrorField(payload, 'retMsg', 'ret_msg', 'message', 'msg'));

const parseBybitFiniteNumber = (value: unknown): number | undefined => {
  const number = typeof value === 'number' ? value : parseFloat(`${value ?? ''}`);
  return Number.isFinite(number) ? number : undefined;
};

const sumBybitCoinFields = (coins: Record<string, any>[], fields: string[]): number | undefined => {
  let sawFiniteValue = false;
  const sum = coins.reduce((acc, coin) => {
    const coinSum = fields.reduce((fieldAcc, field) => {
      const value = parseBybitFiniteNumber(coin[field]);
      if (value === undefined) {
        return fieldAcc;
      }
      sawFiniteValue = true;
      return fieldAcc + value;
    }, 0);
    return acc + coinSum;
  }, 0);

  return sawFiniteValue ? sum : undefined;
};

const deriveBybitAvailableBalanceFromCoins = (coins: Record<string, any>[]): number | undefined => {
  let sawFiniteValue = false;
  const total = coins.reduce((acc, coin) => {
    const walletValue =
      parseBybitFiniteNumber(coin.usdValue) ??
      parseBybitFiniteNumber(coin.walletBalance) ??
      parseBybitFiniteNumber(coin.equity);
    if (walletValue === undefined) {
      return acc;
    }

    sawFiniteValue = true;
    const used =
      (parseBybitFiniteNumber(coin.totalPositionIM) ?? 0) +
      (parseBybitFiniteNumber(coin.totalOrderIM) ?? 0) +
      (parseBybitFiniteNumber(coin.locked) ?? 0) +
      (parseBybitFiniteNumber(coin.bonus) ?? 0);

    return acc + Math.max(walletValue - used, 0);
  }, 0);

  return sawFiniteValue ? total : undefined;
};

const firstFiniteNumber = (...values: Array<number | undefined>): number => {
  for (const value of values) {
    if (value !== undefined && Number.isFinite(value)) {
      return value;
    }
  }
  return 0;
};

const getBybitTriggerBy = (opts: PlaceOrderOpts) => {
  if (opts.type === OrderType.Conditional) {
    if (opts.triggerType === OrderStopTriggerType.MarkPrice) return 'MarkPrice';
    if (opts.triggerType === OrderStopTriggerType.IndexPrice) return 'IndexPrice';
    return 'LastPrice';
  }
  if (opts.type === OrderType.StopLoss) {
    return opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'LastPrice' : 'MarkPrice';
  }
  if (opts.type === OrderType.TakeProfit) {
    return opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MarkPrice' : 'LastPrice';
  }
  return undefined;
};

const getBybitTriggerDirectionPrice = (opts: PlaceOrderOpts, ticker: Ticker) => {
  if (opts.type === OrderType.Conditional) {
    if (opts.triggerType === OrderStopTriggerType.MarkPrice) return ticker.mark;
    if (opts.triggerType === OrderStopTriggerType.IndexPrice) return ticker.index;
    return ticker.last;
  }
  if (opts.type === OrderType.StopLoss) {
    return opts.slTriggerType === OrderStopTriggerType.LastPrice ? ticker.last : ticker.mark;
  }
  if (opts.type === OrderType.TakeProfit) {
    return opts.tpTriggerType === OrderStopTriggerType.LastPrice ? ticker.last : ticker.mark;
  }
  return ticker.last;
};

export class BybitExchange extends BaseExchange {
  protected readonly clientOrderIdRule: ClientOrderIdRule | undefined = patternClientOrderIdRule(
    'Bybit',
    /^[A-Za-z0-9_-]{1,36}$/,
  );

  xhr: Axios;
  unlimitedXHR: Axios;

  get supportsTradeDerivedCandles() {
    return true;
  }

  // Bulk kline subscribe batch size. Bybit livenet comfortably accepts up to
  // ~50 topics per `subscribe` frame; Hyro's simulator rejects anything over
  // 32 and overrides this to 10 (see `HyrotraderExchange`). Keeping the base
  // default at 50 preserves the pre-Hyro Bybit subscribe cadence.
  get publicKlineSubscribeChunkSize() {
    return 50;
  }

  // The "already gone" classifier for cancel responses. Bybit and bybitdemo
  // both reply with `"order not exists"` / `"too late to cancel"` when a
  // cancel loses the race to a fill, so the base classifier matches those.
  // Hyro's simulator uses different phrasing for the same semantic
  // (`"Order does not exist or is no longer open"`) and overrides this hook
  // in `HyrotraderExchange` to accept both. Returning `true` from this hook
  // causes bulk-cancel paths to retire the row silently instead of raising
  // a user-facing error.
  protected isAlreadyGoneCancelMessage(message: unknown): boolean {
    return isBybitOrderAlreadyGoneCancelMessage(message);
  }

  validateCandleInterval(interval: string) {
    if (!this.supportsTradeDerivedCandles && interval.endsWith('s')) {
      const error = new Error('HyroTrader seconds candles require a supported trade stream');
      this.emitAndLogError(error);
      throw error;
    }
  }

  publicWebsocket: BybitPublicWebsocket;
  privateWebsocket: BybitPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // Bybit no longer offers Classic accounts. Start in UTA 2.0 Pro mode so
  // account validation never probes the retired CONTRACT wallet API first.
  private unifiedMarginStatus: number = 6;
  private marginType: MarginType = MarginType.Cross;

  private needsReinit = false;
  private lastReinitAt = 0;
  private reinitRetryMs = REINIT_RETRY_MS;
  // A thrown request (an unavailable or timed-out remote signer) must trigger the
  // full refetch exactly like a venue error does.
  private reinitOnThrow = <T>(request: Promise<T>): Promise<T> =>
    request.catch((error: unknown) => {
      this.needsReinit = true;
      throw error;
    });
  private readonly debouncedPrivateSnapshotRefresh = debounce(
    async () => {
      if (this.isDisposed || this.isPublicDataOnly) {
        return;
      }

      try {
        await this.fetchAndSetPrivateSnapshotHelper();
      } catch (err: any) {
        this.emitAndLogError(err);
      }
    },
    PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS,
    { maxWait: PRIVATE_SNAPSHOT_REFRESH_MAX_WAIT_MS },
  );

  positionMode: PositionMode | null = null;

  lastWebsocketPositionsUpdate: number | undefined;

  private apiKeyInfo: BybitApiKeyInfo | null = null;

  private batchLimit: number = 10;
  private readonly EXECUTION_QUERY_MAX_RANGE_MS = 7 * 24 * 60 * 60 * 1000 - 1;
  private readonly DEFAULT_EXECUTIONS_LOOKBACK_MS = 7 * 24 * 60 * 60 * 1000;

  private getExecutionQueryWindows = (
    from?: number,
    to?: number,
  ): Array<{
    startTime?: number;
    endTime?: number;
  }> => {
    if (from === undefined || to === undefined) {
      return [{ startTime: from, endTime: to }];
    }

    if (to - from <= this.EXECUTION_QUERY_MAX_RANGE_MS) {
      return [{ startTime: from, endTime: to }];
    }

    const windows: Array<{ startTime: number; endTime: number }> = [];
    let windowEnd = to;
    while (windowEnd > from) {
      const windowStart = Math.max(from, windowEnd - this.EXECUTION_QUERY_MAX_RANGE_MS);
      windows.push({ startTime: windowStart, endTime: windowEnd });
      if (windowStart <= from) {
        break;
      }
      windowEnd = windowStart - 1;
    }

    return windows;
  };

  get accountType() {
    return this.unifiedMarginStatus === 1 ? 'CONTRACT' : 'UNIFIED';
  }

  get cachedApiKeyInfo() {
    return this.apiKeyInfo;
  }

  private formatBybitVenueError(payload: unknown, fallback?: unknown): string {
    return (
      formatBybitTradingAgreementError(payload, this.name) ??
      getBybitPayloadMessage(payload) ??
      getBybitPayloadMessage(fallback) ??
      'Unknown Bybit error'
    );
  }

  private usesBybitDemoEnvironment() {
    return this.name === 'bybitdemo' || this.isDemo;
  }

  getBybitRestBaseUrl(network: 'livenet' | 'testnet') {
    return this.usesBybitDemoEnvironment() ? DEMO_URL[network] : BASE_URL[network];
  }

  getBybitPublicWsUrl(category: 'linear' | 'inverse', network: 'livenet' | 'testnet') {
    return BASE_WS_URL.public[category][network];
  }

  getBybitPrivateWsUrl(network: 'livenet' | 'testnet') {
    return (this.usesBybitDemoEnvironment() ? DEMO_WS_URL : BASE_WS_URL).private[network];
  }

  protected requiresOrderLinkId() {
    return false;
  }

  private generatedOrderLinkId() {
    return this.requiresOrderLinkId() ? nanoid() : undefined;
  }

  protected validateOrderCapabilities(_opts: BybitPlaceOrderOpts) {}

  getSettleCoinsForCategory(category: string) {
    if (category === 'linear') {
      if (this.accountType === 'UNIFIED') return ['USDT', 'USDC'];
      return ['USDT'];
    }
    this.emitAndLogError(`Unknown category ${category}`);
    throw new Error(`Unknown category ${category}`);
  }

  getCategoryForSymbol = (symbol: string) => {
    if (isBybitOptionSymbol(symbol)) return 'option';
    return isBybitInverseSymbol(symbol) ? 'inverse' : 'linear';
  };

  /**
   * The shared /v5/private socket carries every category on the account. This connection keeps only the ones it
   * trades, so a linear-only account never stores, sizes or values an inverse or option row.
   */
  tradesCategoryOf = (symbol: string): boolean => {
    const category = this.getCategoryForSymbol(symbol);
    if (category === 'option') return this.optionsEnabled();
    if (category === 'inverse') return this.inverseEnabled();
    return true;
  };

  /** Options join this connection only when the account opted in; subclasses that cannot trade them return false. */
  protected optionsEnabled() {
    return Boolean(this.options.enableOptionMarkets);
  }

  /** Inverse contracts join this connection only when the account opted in; subclasses that cannot trade them return false. */
  protected inverseEnabled() {
    // A shared public data source has no inverse markets, so an account reading one cannot size inverse positions.
    return Boolean(this.options.enableInverseMarkets) && !this.options.externalPublicData;
  }

  /** Inverse quantities are USD contracts; the store holds `remaining` in the coin at `price` and the contracts in `extra`. */
  private remainingInStoreUnits(symbol: string, remainingNative: number, price: number) {
    if (!isBybitInverseSymbol(symbol) || !(price > 0)) return { remaining: remainingNative };
    return { remaining: Math.round((remainingNative / price) * 1e8) / 1e8, extra: { remainingNative } };
  }

  private publicSocketFor(symbol: string): BybitPublicWebsocket {
    return (this.getCategoryForSymbol(symbol) === 'inverse' && this.inversePublicWebsocket) || this.publicWebsocket;
  }

  private get inversePublicWebsocket(): BybitPublicWebsocket | undefined {
    return this.publicWebsocketSecondary as BybitPublicWebsocket | undefined;
  }

  getTickerSymbolsForCategory(category: 'linear' | 'inverse') {
    const marketSymbols = this.store.markets
      .filter((market) => {
        if (market.option) return false;
        if (category === 'linear') {
          return market.quote === 'USDT' || market.quote === 'USDC';
        }

        return market.quote !== 'USDT' && market.quote !== 'USDC';
      })
      .map((market) => market.symbol);

    return this.getTickerSymbolsForSubscription(marketSymbols);
  }

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = this.createXhrWithSecondaryKey(opts, createAPI, { rateLimit: (xhr) => rateLimit(xhr, { maxRPS: 5 }) });
    this.unlimitedXHR = createAPI(opts, this);

    this.publicWebsocket = new BybitPublicWebsocket(this, 'linear');
    if (this.inverseEnabled()) this.publicWebsocketSecondary = new BybitPublicWebsocket(this, 'inverse');
    this.privateWebsocket = new BybitPrivateWebsocket(this);
  }

  dispose = () => {
    this.debouncedPrivateSnapshotRefresh.cancel();
    for (const orderId of this.recentlyPlacedOrderReconciliationTimers.keys()) {
      this.clearRecentlyPlacedOrderReconciliationTimers(orderId);
    }
    super.dispose();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
  };

  private scheduleSetTradingStopSnapshotRefresh = () => {
    this.debouncedPrivateSnapshotRefresh();
  };

  /**
   * Orders placed moments ago that no REST snapshot or private-WS frame has
   * confirmed yet. `/v5/order/realtime` lags order creation (worst on
   * `api-demo`), so any wholesale order-snapshot replacement — private-WS
   * reconnect housekeeping, a startup reinit, the SET_TRADING_STOP backstop —
   * can fetch a list WITHOUT an order placed moments earlier and replace it out
   * of the store. The chaser supervisor then sees its live order "disappear",
   * stops itself, and cancels that order on the exchange: the demo "chaser does
   * nothing" bug.
   *
   * An id leaves this map on its FIRST sighting anywhere (REST snapshot,
   * private-WS order frame, or our own cancel) or after
   * `RECENTLY_PLACED_ORDER_GRACE_MS`. Once an order has been seen, a later
   * absence means filled/cancelled and MUST be honored — otherwise the chaser
   * would re-place on top of a fill and overfill the user's target.
   */
  private recentlyPlacedOrders = new Map<string, { placedAt: number; order: Order }>();
  private recentlyPlacedOrderReconciliationTimers = new Map<string, ReturnType<typeof setTimeout>[]>();

  // Protected method (not a private arrow) so Bybit-compatible subclasses whose
  // private WS reliability differs from the base (e.g. HyrotraderExchange, whose
  // simulator-backed private WS has not yet been independently verified against
  // Bybit mainnet) can override without touching Bybit's own live/demo gate.
  protected shouldUseLocalRestOrderLifecycleFallback(): boolean {
    return this.isDemo || this.options.bybitSiteId === BYBIT_BRAZIL_SITE_ID;
  }

  /** First sighting via private WS or an explicit cancel — stop protecting the id. */
  markOrderSeen = (orderId: string) => {
    this.recentlyPlacedOrders.delete(orderId);
    this.clearRecentlyPlacedOrderReconciliationTimers(orderId);
  };

  private clearRecentlyPlacedOrderReconciliationTimers = (orderId: string) => {
    const timers = this.recentlyPlacedOrderReconciliationTimers.get(orderId);
    if (!timers) return;

    timers.forEach((timer) => clearTimeout(timer));
    this.recentlyPlacedOrderReconciliationTimers.delete(orderId);
  };

  private shouldReconcileLocalReduceOnlyLimitOrder = (order: Order) => {
    return (
      this.shouldUseLocalRestOrderLifecycleFallback() && order.type === OrderType.Limit && order.reduceOnly === true
    );
  };

  private scheduleLocalReduceOnlyLimitOrderReconciliation = (order: Order) => {
    if (!this.shouldReconcileLocalReduceOnlyLimitOrder(order)) return;

    this.clearRecentlyPlacedOrderReconciliationTimers(order.id);
    const timers = REDUCE_ONLY_LIMIT_RECONCILE_DELAYS_MS.map((delayMs) =>
      setTimeout(() => {
        void this.reconcileLocalReduceOnlyLimitOrder(order.id);
      }, delayMs),
    );
    this.recentlyPlacedOrderReconciliationTimers.set(order.id, timers);
  };

  private getFilledOrderReconciliationTarget = (order: Order) => {
    const amounts = [order.amount, order.remaining + (order.filled || 0)].filter(Number.isFinite);
    return amounts.length > 0 ? Math.max(...amounts) : 0;
  };

  private reconcileLocalReduceOnlyLimitOrder = async (orderId: string) => {
    const entry = this.recentlyPlacedOrders.get(orderId);
    if (!entry || this.isDisposed) return;

    const order = this.store.orders.find((o) => o.id === orderId) ?? entry.order;
    if (!this.shouldReconcileLocalReduceOnlyLimitOrder(order)) return;

    try {
      const fillsByOrderId = await this.fetchFillsForOrderIds([orderId]);
      if (this.isDisposed) return;

      const filled = fillsByOrderId.get(orderId) ?? 0;
      const targetAmount = this.getFilledOrderReconciliationTarget(order);
      const fullFillTolerance = Math.max(targetAmount * 1e-8, 1e-12);
      if (targetAmount > 0 && filled >= targetAmount - fullFillTolerance) {
        this.markOrderSeen(orderId);
        this.store.removeOrders([{ id: orderId }]);
        return;
      }

      const now = virtualClock.getCurrentTimeMs();
      if (now - entry.placedAt < RECENTLY_PLACED_ORDER_GRACE_MS) {
        return;
      }

      const orders = await this.fetchOrders();
      if (this.isDisposed) return;
      this.store.update({
        orders,
        loaded: { ...this.store.loaded, orders: true },
      });
    } catch (error: any) {
      this.log(
        `Failed to reconcile local Bybit reduce-only limit order ${orderId}: ${error?.message || error}`,
        LogSeverity.Warning,
      );
    }
  };

  protected recordRecentlyPlacedOrderFromOpts = (
    orderId: string,
    opts: PlaceOrderOpts,
    payload: Record<string, any>,
  ) => {
    const qty = parseFloat(payload.qty as string) || opts.amount || 0;
    const price = parseFloat((payload.price ?? payload.triggerPrice) as string) || opts.price || opts.triggerPrice || 0;

    this.recordRecentlyPlacedOrder({
      id: orderId,
      status: OrderStatus.Open,
      symbol: opts.symbol,
      type: opts.type,
      side: opts.side,
      price,
      amount: qty,
      filled: 0,
      ...this.remainingInStoreUnits(opts.symbol, qty, price),
      reduceOnly: (opts.type === OrderType.Conditional ? false : opts.reduceOnly) || false,
      timeInForce: opts.timeInForce || OrderTimeInForce.GoodTillCancel,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    });
  };

  private recordRecentlyPlacedOrder = (order: Order) => {
    // Bybit market orders return an orderId, but they execute as IOC and do
    // not represent resting open orders. On demo, injecting them locally turns
    // a completed close into a phantom open row because private order frames
    // may never arrive to retire it.
    if (order.type === OrderType.Market) {
      return;
    }

    this.recentlyPlacedOrders.set(order.id, { placedAt: virtualClock.getCurrentTimeMs(), order });
    // Demo drops private order events. Bybit Brazil accounts can have the same
    // effective symptom in browsers until a header-capable WS route is available:
    // REST writes succeed, but direct browser WS cannot send x-site-id.
    if (this.shouldUseLocalRestOrderLifecycleFallback()) {
      this.store.addOrUpdateOrders([order]);
      this.scheduleLocalReduceOnlyLimitOrderReconciliation(order);
    }
  };

  /**
   * Retire orders we know are gone, locally, because on demo nothing else will.
   *
   * `handleOrderTopic` is the only path that removes an order row, and it needs
   * a private `order` frame. Demo drops those, and the periodic `tick` refetches
   * positions and balance but deliberately NOT orders
   * (`fetchAndSetPrivateSnapshotHelper(false)`), so a cancelled row has nothing
   * to retire it: `recordRecentlyPlacedOrder` injected it into the store at
   * placement and it then outlives the real order indefinitely. The symptoms are
   * a stuck resting order the venue no longer has, and a cancel click that seems
   * to do nothing — the UI removes the row optimistically and the next store
   * broadcast puts it straight back.
   *
   * Every chaser reprice cancels and replaces, so without this the debris
   * accumulates one row per attempt.
   *
   * Live Bybit keeps its WS-driven removal untouched.
   */
  protected retireCancelledOrders = (orders: Array<Pick<Order, 'id' | 'symbol'>>) => {
    if (orders.length === 0) return;

    const retiredAt = virtualClock.getCurrentTimeMs();
    for (const order of orders) {
      // A cancelled id must never be resurrected by the snapshot shield,
      // whichever cancel path we came through.
      this.markOrderSeen(order.id);
      // Retiring the row removes the only record of the order's symbol, which
      // fill reconciliation needs immediately afterwards. See
      // `resolveOrderSymbolForFills`.
      if (order.symbol) {
        this.retiredOrderSymbols.set(order.id, { symbol: order.symbol, retiredAt });
      }
    }

    if (this.shouldUseLocalRestOrderLifecycleFallback()) {
      this.store.removeOrders(orders.map(({ id }) => ({ id })));
    }
  };

  private applyLocalOrderUpdateAfterRestWrite = (order: Order, update: Partial<Order>) => {
    if (!this.shouldUseLocalRestOrderLifecycleFallback()) return;

    const amended = typeof update.amount === 'number';
    const remainingNative = amended
      ? Math.max(0, update.amount! - (order.filled || 0))
      : (order.extra?.remainingNative ?? order.remaining);
    // Inverse rows hold `remaining` in the coin at the order price, so a price-only amend changes it too.
    const units = isBybitInverseSymbol(order.symbol)
      ? this.remainingInStoreUnits(order.symbol, remainingNative, update.price ?? order.price)
      : amended
        ? { remaining: remainingNative }
        : {};
    const nextOrder: Order = {
      ...order,
      ...update,
      ...units,
      ...('extra' in units && units.extra ? { extra: { ...order.extra, ...units.extra } } : {}),
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    const recentlyPlaced = this.recentlyPlacedOrders.get(order.id);
    if (recentlyPlaced) {
      this.recentlyPlacedOrders.set(order.id, { ...recentlyPlaced, order: nextOrder });
    }

    this.store.addOrUpdateOrders([nextOrder]);
  };

  /**
   * Symbol memory for orders we just retired, so `fetchFillsForOrderIds` can
   * still resolve a category after the row has left the store.
   *
   * The chaser cancels, then reconciles fills, then replaces. On demo the cancel
   * now removes the row, so a store-only symbol lookup returned nothing, every
   * requested id was skipped, and the empty result read as "no fills" —
   * replacing on top of a fill and overfilling the target.
   */
  private retiredOrderSymbols = new Map<string, { symbol: string; retiredAt: number }>();

  private resolveOrderSymbolForFills = (orderId: string) => {
    const live = this.store.orders.find((order) => order.id === orderId)?.symbol;
    if (live) return live;

    const now = virtualClock.getCurrentTimeMs();
    for (const [id, entry] of this.retiredOrderSymbols) {
      if (now - entry.retiredAt > RETIRED_ORDER_SYMBOL_TTL_MS) {
        this.retiredOrderSymbols.delete(id);
      }
    }

    return this.retiredOrderSymbols.get(orderId)?.symbol ?? this.recentlyPlacedOrders.get(orderId)?.order.symbol;
  };

  /**
   * Re-add not-yet-sighted recent orders that a REST snapshot omitted. Runs on
   * the successful `fetchOrders` result so every wholesale replacement honors
   * it, whatever triggered the fetch.
   */
  private preserveRecentlyPlacedOrders = (fetched: Order[]): Order[] => {
    if (this.recentlyPlacedOrders.size === 0) return fetched;

    const now = virtualClock.getCurrentTimeMs();
    const fetchedIds = new Set(fetched.map((o) => o.id));
    const preserved: Order[] = [];

    for (const [id, entry] of this.recentlyPlacedOrders) {
      if (fetchedIds.has(id)) {
        // REST has indexed it — normal reconciliation owns it from here on.
        this.markOrderSeen(id);
        continue;
      }
      if (now - entry.placedAt > RECENTLY_PLACED_ORDER_GRACE_MS) {
        this.markOrderSeen(id);
        continue;
      }
      preserved.push(this.store.orders.find((o) => o.id === id) ?? entry.order);
    }

    return preserved.length > 0 ? [...fetched, ...preserved] : fetched;
  };

  getAccount = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT);

    if (data.retCode !== 0) {
      throw new Error(data.retMsg);
    }

    return {
      userId: data?.result?.userID,
      affiliateId: data?.result?.inviterID,
    };
  };

  validateAccount = async () => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_MARGIN);

      if (data.retMsg !== 'OK') {
        this.emitAndLogError(data.retMsg);

        if (data.retMsg.includes('timestamp and recv_window param')) {
          return { error: 'Check your computer time and date' };
        }

        return { error: data.retMsg };
      }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }

    // Key validated above; balance is best-effort for the new-account gate (a
    // separate wallet-balance call) and never fails validation.
    let balance: Balance | undefined;
    try {
      balance = await this.fetchBalance();
    } catch {
      // best-effort
    }
    return { error: '', balance };
  };

  start = async () => {
    await virtualClock.start();

    // Reset batch limit on exchange start
    this.batchLimit = 10;

    const publicDataOnly = this.isPublicDataOnly;

    if (this.needsAccountMarketOverlay) {
      await this.loadAccountMarketOverlay(async () => bybitFeeOverlay(await this.fetchFees('linear')));
      if (this.isDisposed) return;
    }

    const useExternalPublicData = await this.initExternalPublicData();

    // Margin mode and unified-account status are private, and every position
    // is stamped with them. They used to be fetched inside the public-data
    // branch below, so an account sourcing public data from elsewhere kept
    // reporting cross margin regardless of its real setting.
    // Handled where it is created: a rejected key fails this before the market fetch
    // below finishes, and a rejection with no handler yet ends a Node process.
    const privateProbe = publicDataOnly
      ? null
      : this.fetchMarginAccountInfos().then(
          () => undefined,
          (err: any) => String(err?.message || err) || 'request failed',
        );

    // Fetch API key info asynchronously without blocking startup
    if (!publicDataOnly) {
      this.fetchAndCacheApiKeyInfo().catch(() => {
        // Already handled in fetchAndCacheApiKeyInfo
      });
    }

    if (!useExternalPublicData) {
      // Fetch markets, tickers, and margin info in parallel
      const [_, rawTickers] = await Promise.all([
        this.fetchMarkets().then((markets) => {
          if (this.isDisposed) return [];
          this.log(`Loaded ${markets.length} Bybit markets`);
          this.store.update({
            markets,
            loaded: { ...this.store.loaded, markets: true },
          });

          // start public websocket streams as soon as markets loaded.
          this.publicWebsocket.connectAndSubscribe();
          this.inversePublicWebsocket?.connectAndSubscribe();

          return markets;
        }),
        this.fetchTickersWithoutParse(),
      ]);

      const tickers = this.parseTickers(rawTickers);

      if (this.isDisposed) return;

      this.store.update({
        tickers,
        loaded: { ...this.store.loaded, tickers: true },
      });
    }

    // Awaited here rather than inside the branch above: it must run on both
    // public-data paths, and it is what stamps margin mode onto positions.
    const privateProbeFailure = privateProbe ? await privateProbe : undefined;

    // A rejected key comes back as HTTP 200 with a non-OK retMsg, so the probe is the
    // only thing that knows. `authFailureStreak` cannot be used: markets and tickers
    // are signed too, and their success calls recordAuthSuccess() after the probe.
    if (privateProbeFailure) {
      this.log(
        `Bybit market data is ready, but the private API was rejected (${privateProbeFailure}) — trading is unavailable`,
        LogSeverity.Warning,
      );
    } else {
      this.log(`Ready to trade on Bybit`);
    }

    // start ticking live data
    // balance, tickers, positions
    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    // Additional safety check to prevent private websocket connection for demo/empty credentials
    // (remoteSigner mode has empty secret but still needs private WS)
    if (!this.options.key || (!this.options.secret && !this.options.remoteSigner && !this.options.payloadSigner)) {
      this.log('Skipping private websocket connection - no API credentials', LogSeverity.Warning);
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    await this.fetchOrdersPositionsAndBalance();

    this.tick();
  };

  fetchOrdersPositionsAndBalance = async () => {
    try {
      const [, , rawBalance] = await Promise.all([
        this.fetchOrders().then((orders) => {
          if (this.isDisposed) return [];
          this.store.update({
            orders,
            loaded: { ...this.store.loaded, orders: true },
          });
          return orders;
        }),
        this.fetchPositions().then((fetchedPositions) => {
          if (this.isDisposed) return [];
          const positions =
            this.lastWebsocketPositionsUpdate !== undefined &&
            virtualClock.getCurrentTimeMs() - this.lastWebsocketPositionsUpdate <
              INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY
              ? mergeUpdatesUsingLastUpdate(fetchedPositions, this.store.positions)
              : fetchedPositions;
          this.store.addOrUpdatePositionsFull(positions);
          this.store.update({
            loaded: { ...this.store.loaded, positions: true },
          });
          return positions;
        }),
        this.fetchRawBalance(),
      ]);

      if (this.isDisposed) return;
      const balance = this.formatBalance(rawBalance);
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err);
    }
  };

  fetchAndSetPrivateSnapshot = async () => {
    await this.fetchAndSetPrivateSnapshotHelper();
  };

  /**
   * The three fetches run CONCURRENTLY, and on the remote-signing path that is
   * the difference between keeping the tick and missing it.
   *
   * Every one of these is an authenticated REST call, and with a browser-side
   * signer each costs a container -> browser -> container round trip. Measured
   * live on 2026-09-11 during CPI: p50 102-272ms per round trip depending on
   * shard, p95 up to 443ms. Run in sequence that is 544ms of signing for the
   * two-call tick alone, against a 500ms `tickInterval` — so the tick was
   * permanently late and the store kept being refreshed from data that had
   * already aged out, which is what "flapping" looks like from the outside.
   *
   * Concurrency does not change the number of signatures, only how long they
   * take end to end. The store writes stay in their original order below,
   * because positions still has to see `lastWebsocketPositionsUpdate` as it
   * stands when the fetch resolves.
   */
  fetchAndSetPrivateSnapshotHelper = async (fetchOrders = true) => {
    try {
      const ordersPromise = fetchOrders ? this.fetchOrders() : undefined;
      const positionsPromise = this.fetchPositions();
      const balancePromise = this.fetchBalance();
      // Nothing here is allowed to reject unhandled just because a sibling
      // rejected first: all three are awaited below, inside this try.
      ordersPromise?.catch(() => undefined);
      positionsPromise.catch(() => undefined);
      balancePromise.catch(() => undefined);

      if (ordersPromise) {
        const orders = await ordersPromise;
        if (this.isDisposed) return;

        this.store.update({
          orders,
          loaded: { ...this.store.loaded, orders: true },
        });
      }

      const fetchedPositions = await positionsPromise;
      if (this.isDisposed) return;
      const positions =
        this.lastWebsocketPositionsUpdate !== undefined &&
        virtualClock.getCurrentTimeMs() - this.lastWebsocketPositionsUpdate <
          INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY
          ? mergeUpdatesUsingLastUpdate(fetchedPositions, this.store.positions)
          : fetchedPositions;

      this.store.addOrUpdatePositionsFull(positions);
      this.store.update({
        loaded: { ...this.store.loaded, positions: true },
      });

      const balance = await balancePromise;

      if (this.isDisposed) return;
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    } catch (err: any) {
      this.emitAndLogError(err);
    }
  };

  fetchMarginAccountInfos = async () => {
    const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT_MARGIN);

    if (data.retMsg !== 'OK') {
      // Thrown, not returned: this is the only signal for the readiness log below
      // that cannot be reset by a later request. Emitting first is required —
      // see exchanges/CLAUDE.md, "Pre-flight validation throws must emit first".
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }

    this.marginType = data?.result?.marginMode === 'ISOLATED_MARGIN' ? MarginType.Isolated : MarginType.Cross;
    this.unifiedMarginStatus = data?.result?.unifiedMarginStatus;
  };

  tick = async () => {
    if (!this.isDisposed) {
      // Wait for remote signer if configured but unavailable
      if (!(await this.waitForRemoteSigner())) return;

      // Cleared BEFORE the refetch so a failure inside can set it again; backed off so
      // a persistent failure (a key missing a permission) is not refetched every tick.
      if (this.needsReinit && Date.now() - this.lastReinitAt >= this.reinitRetryMs) {
        this.needsReinit = false;
        this.lastReinitAt = Date.now();
        await this.fetchOrdersPositionsAndBalance();
        this.reinitRetryMs = this.needsReinit ? Math.min(this.reinitRetryMs * 2, REINIT_RETRY_MAX_MS) : REINIT_RETRY_MS;
      }

      try {
        await this.fetchAndSetPrivateSnapshotHelper(false);
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), this.getTickInterval());
      }
    }
  };

  fetchBalance = async () => {
    const rawBalance = await this.fetchRawBalance();
    if (this.isDisposed) return this.store.balance;
    return this.formatBalance(rawBalance);
  };

  syncBalanceUpnlFromPositions = () => {
    const balance = syncUnifiedBalanceUpnlFromPositions(this.store.balance, this.store.positions, this.accountType);
    if (!balance || balance === this.store.balance) {
      return;
    }

    this.store.update({
      balance,
    });
  };

  // for UTA 2.0
  forceUnifiedBalance = false;

  fetchRawBalance = async (): Promise<any> => {
    const { data } = await this.xhr.get(ENDPOINTS.BALANCE, {
      params: { accountType: this.forceUnifiedBalance ? 'UNIFIED' : this.accountType },
    });

    if (v(data, 'retMsg') !== 'OK' && +v(data, 'retCode') !== 0) {
      if (v(data, 'retMsg') === 'accountType only support UNIFIED.' && !this.forceUnifiedBalance) {
        this.forceUnifiedBalance = true;
        this.unifiedMarginStatus = 6;
        return this.fetchRawBalance();
      }
      this.emitAndLogError(v(data, 'retMsg'));
      return this.store.balance?.info || [];
    }
    return data.result.list;
  };

  formatBalance = (rawBalance: any) => {
    if (!Array.isArray(rawBalance) || rawBalance.length === 0) {
      this.log('Invalid or empty balance data received', LogSeverity.Warning);
      return this.store.balance;
    }
    const [firstAccount] = rawBalance;
    const coins = Array.isArray(firstAccount.coin) ? firstAccount.coin : [];
    const upnl = this.store.positions.reduce((acc, p) => acc + positionUpnlInQuote(p), 0);
    const totalEquity = parseBybitFiniteNumber(firstAccount.totalEquity);
    const totalWalletBalance = parseBybitFiniteNumber(firstAccount.totalWalletBalance);
    const totalMarginBalance = parseBybitFiniteNumber(firstAccount.totalMarginBalance);
    const totalAvailableBalance = parseBybitFiniteNumber(firstAccount.totalAvailableBalance);
    const totalInitialMargin = parseBybitFiniteNumber(firstAccount.totalInitialMargin);
    const totalMaintenanceMargin = parseBybitFiniteNumber(firstAccount.totalMaintenanceMargin);
    const usedFromTopLevel =
      totalInitialMargin !== undefined || totalMaintenanceMargin !== undefined
        ? (totalMaintenanceMargin ?? 0) + (totalInitialMargin ?? 0)
        : undefined;
    const usedFromCoins = sumBybitCoinFields(coins, ['totalPositionIM', 'totalOrderIM']);
    const derivedTopAvailable =
      totalMarginBalance !== undefined
        ? totalInitialMargin !== undefined
          ? Math.max(totalMarginBalance - totalInitialMargin, 0)
          : totalMarginBalance
        : undefined;
    const derivedCoinAvailable = deriveBybitAvailableBalanceFromCoins(coins);
    const total = subtract(firstFiniteNumber(totalEquity, totalWalletBalance), upnl);

    const balance: Balance = {
      total,
      upnl,
      used: firstFiniteNumber(usedFromTopLevel, usedFromCoins),
      free: firstFiniteNumber(
        totalAvailableBalance,
        derivedTopAvailable,
        derivedCoinAvailable,
        totalWalletBalance,
        totalEquity,
      ),
      coins: coins.reduce(
        (prev: { [coin: string]: CoinBalance }, coin: any) => {
          const amount = firstFiniteNumber(
            parseBybitFiniteNumber(coin.equity),
            parseBybitFiniteNumber(coin.walletBalance),
          );
          prev[coin.coin] = {
            base: coin.coin,
            amount,
            notional: firstFiniteNumber(parseBybitFiniteNumber(coin.usdValue), amount),
          };
          return prev;
        },
        {} as { [coin: string]: CoinBalance },
      ),
      info: firstAccount,
    };

    return balance;
  };

  fetchApiKeyInfo = async (): Promise<BybitApiKeyInfo> => {
    const { data } = await this.xhr.get(ENDPOINTS.ACCOUNT);

    if (+v(data, 'retCode') !== 0) {
      this.emitAndLogError(v(data, 'retMsg'));
      throw new Error(v(data, 'retMsg'));
    }

    // Store the API key info locally
    this.apiKeyInfo = data.result;

    return data.result;
  };

  protected fetchAndCacheApiKeyInfo = async (): Promise<void> => {
    try {
      await this.fetchApiKeyInfo();
      if (this.apiKeyInfo) {
        this.updateBatchLimitBasedOnVipLevel();
        this.store.updateUserData({
          bybitApiKeyInfo: this.apiKeyInfo,
        });
      }
      this.log(
        `API key info cached. VIP Level: ${this.apiKeyInfo?.vipLevel || 'Unknown'}, Batch Limit: ${this.batchLimit}`,
      );
    } catch {
      // Don't fail if the API key info fetch fails
      this.log('Failed to fetch API key info, continuing without it');
    }
  };

  private updateBatchLimitBasedOnVipLevel = (): void => {
    if (!this.apiKeyInfo?.vipLevel) {
      return;
    }

    // Find the rate limit entry for this VIP level
    const rateLimitEntry = BYBIT_RATE_LIMIT_TABLE.find(
      (entry) => entry.level === (this.apiKeyInfo!.vipLevel as BybitVipLevel),
    );
    if (!rateLimitEntry) {
      this.log(`No rate limit entry found for VIP level: ${this.apiKeyInfo.vipLevel}`);
      return;
    }

    // Get the futures rate limit based on account type
    const accountType = this.accountType === 'UNIFIED' ? BybitAccountType.UTAPro : BybitAccountType.ClassicAndUTA;
    const futuresRateLimit = rateLimitEntry[BybitProductType.Futures][accountType];

    // Update batch limit if rate limit is >= 20
    if (futuresRateLimit >= 20) {
      this.batchLimit = 20;
      this.log(
        `VIP level ${this.apiKeyInfo.vipLevel} allows ${futuresRateLimit} req/s for futures. Setting batch limit to 20.`,
      );
    } else {
      this.log(
        `VIP level ${this.apiKeyInfo.vipLevel} allows ${futuresRateLimit} req/s for futures. Keeping batch limit at ${this.batchLimit}.`,
      );
    }
  };

  fetchOrders = async () => {
    const settleCoins = this.getSettleCoinsForCategory('linear');
    const orders = await mapSeries(settleCoins, async (settleCoin) => {
      return this.fetchOrdersHelper('linear', settleCoin);
    });
    if (this.inverseEnabled()) orders.push(await this.fetchOrdersHelper('inverse'));
    if (this.optionsEnabled())
      orders.push(await this.fetchCategoryRowsOrKeep('option', 'orders', () => this.fetchOrdersHelper('option')));
    // Every consumer of this result wholesale-replaces store.orders, so shield
    // just-placed orders REST hasn't indexed yet (worst on api-demo).
    return this.preserveRecentlyPlacedOrders(orders.flat());
  };

  /**
   * A failed read of an opted-in category keeps that category's rows the store holds, so one bad poll does not
   * wipe them, or move the balance by their PnL, until the next one.
   */
  private fetchCategoryRowsOrKeep = async <T extends { symbol: string }>(
    category: 'inverse' | 'option',
    what: 'orders' | 'positions',
    fetch: () => Promise<T[]>,
  ): Promise<T[]> => {
    try {
      return await fetch();
    } catch (err: any) {
      this.log(`Failed to refresh Bybit ${category} ${what}: ${err?.message ?? err}`, LogSeverity.Warning);
      return (this.store[what] as unknown as T[]).filter((row) => this.getCategoryForSymbol(row.symbol) === category);
    }
  };

  fetchOrdersHelper = async (category: string, settleCoin?: string) => {
    const recursiveFetch = async (
      cursor: string = '',
      orders: Array<Record<string, any>> = [],
    ): Promise<Array<Record<string, any>>> => {
      const { data } = await this.reinitOnThrow(
        this.xhr.get(ENDPOINTS.UNFILLED_ORDERS, {
          params: {
            category,
            settleCoin,
            limit: 50,
            cursor,
          },
        }),
      );

      if (v(data, 'retMsg') !== 'OK') {
        this.emitAndLogError(v(data, 'retMsg'));
        this.needsReinit = true;
        throw new Error(v(data, 'retMsg'));
      }

      const ordersList = Array.isArray(data?.result?.list) ? data.result.list : [];

      if (ordersList.length === 0) {
        return orders;
      }

      if (data.result.nextPageCursor) {
        return recursiveFetch(data.result.nextPageCursor, [...orders, ...ordersList]);
      }

      return [...orders, ...ordersList];
    };

    const bybitOrders = await recursiveFetch();
    const orders: Order[] = bybitOrders.flatMap(this.mapOrder.bind(this));

    return orders;
  };

  fetchPositions = async (symbols?: string[]) => {
    if (symbols && symbols?.length > 0) {
      return await this.fetchPositionsHelper(symbols);
    } else {
      const settleCoins = this.getSettleCoinsForCategory('linear');
      const positions = await mapSeries(settleCoins, async (settleCoin) => {
        return this.fetchPositionsHelper(undefined, 'linear', settleCoin);
      });
      if (this.inverseEnabled()) {
        positions.push(
          await this.fetchCategoryRowsOrKeep('inverse', 'positions', () =>
            this.fetchPositionsHelper(undefined, 'inverse', undefined, true),
          ),
        );
      }
      if (this.optionsEnabled()) {
        positions.push(
          await this.fetchCategoryRowsOrKeep('option', 'positions', () =>
            this.fetchPositionsHelper(undefined, 'option', undefined, true),
          ),
        );
      }
      return positions.flat();
    }
  };

  private getPositionUpdatedTime = (position: Position): number => {
    const updatedTime = Number(position.info?.updatedTime ?? 0);
    return Number.isFinite(updatedTime) ? updatedTime : 0;
  };

  private getPositionReliabilityScore = (position: Position): number => {
    const info = position.info as Record<string, any> | undefined;
    const seq = Number(info?.seq);
    const positionStatus = typeof info?.positionStatus === 'string' ? info.positionStatus.trim() : '';
    const avgPrice = info?.avgPrice;

    let score = 0;
    if (Number.isFinite(position.contracts) && position.contracts !== 0) score += 1000;
    if (this.getPositionUpdatedTime(position) > 0) score += 100;
    if (Number.isFinite(seq) && seq >= 0) score += 50;
    if (positionStatus) score += 20;
    if (avgPrice !== undefined && avgPrice !== null && `${avgPrice}`.trim() !== '') score += 5;

    return score;
  };

  protected dedupePositionsById = (positions: Position[]): Position[] => {
    if (positions.length < 2) {
      return positions;
    }

    const ids: string[] = [];
    const positionsById = new Map<
      string,
      {
        position: Position;
        score: number;
        updatedTime: number;
      }
    >();

    for (const position of positions) {
      const score = this.getPositionReliabilityScore(position);
      const updatedTime = this.getPositionUpdatedTime(position);
      const existing = positionsById.get(position.id);

      if (!existing) {
        ids.push(position.id);
        positionsById.set(position.id, { position, score, updatedTime });
        continue;
      }

      if (score > existing.score || (score === existing.score && updatedTime > existing.updatedTime)) {
        positionsById.set(position.id, { position, score, updatedTime });
      }
    }

    return ids.map((id) => positionsById.get(id)!.position);
  };

  fetchPositionsHelper = async (symbols?: string[], category?: string, settleCoin?: string, failClosed = false) => {
    const fetchAllPages = async (
      params: Record<string, unknown>,
      failClosed: boolean,
      cursor = '',
      accumulated: Array<Record<string, any>> = [],
    ): Promise<Array<Record<string, any>>> => {
      const { data } = await this.reinitOnThrow(
        this.xhr.get(ENDPOINTS.POSITIONS, {
          params: { ...params, limit: 200, cursor },
        }),
      );
      if (v(data, 'retMsg') !== 'OK') {
        this.emitAndLogError(v(data, 'retMsg'));
        if (failClosed) throw new Error(v(data, 'retMsg'));
        this.needsReinit = true;
        return [];
      }

      const page = Array.isArray(data?.result?.list) ? data.result.list : [];
      const positions = [...accumulated, ...page];
      const nextPageCursor = data?.result?.nextPageCursor;
      if (nextPageCursor && nextPageCursor !== cursor) {
        return fetchAllPages(params, failClosed, nextPageCursor, positions);
      }
      return positions;
    };

    if (symbols && symbols.length > 0) {
      const positions: Position[][] = await mapSeries(symbols, async (symbol) => {
        const bybitPositions = await fetchAllPages({ symbol, category: this.getCategoryForSymbol(symbol) }, true);
        return this.dedupePositionsById(bybitPositions.map(this.mapPosition.bind(this)));
      });
      return positions.flat();
    } else {
      const bybitPositions = await fetchAllPages({ category, settleCoin }, failClosed);
      return this.dedupePositionsById(bybitPositions.map(this.mapPosition.bind(this)));
    }
  };

  fetchTickersWithoutParse = async () => {
    const rawTickers = await this.fetchTickersRaw();
    if (this.isDisposed) return [];
    return rawTickers;
  };

  fetchTickers = async () => {
    const rawTickers = await this.fetchTickersRaw();
    if (this.isDisposed) return this.store.tickers;
    return this.parseTickers(rawTickers);
  };

  fetchTickersRaw = async () => {
    const tickers = [
      ...(await this.fetchTickersRawHelper('linear')),
      ...(this.inverseEnabled() ? await this.fetchTickersRawHelper('inverse') : []),
    ];
    if (tickers.some((t: any) => t === null)) {
      this.emitAndLogError('Failed to fetch tickers');
      return null;
    }
    return tickers;
  };

  fetchTickersRawHelper = async (category: string) => {
    const { data } = await this.xhr.get(ENDPOINTS.TICKERS, {
      params: { category },
    });

    if (v(data, 'retMsg') !== 'OK') {
      this.emitAndLogError(v(data, 'retMsg'));
      return [null];
    }

    return data.result.list;
  };

  parseTickers = (rawTickers: any) => {
    if (!rawTickers) return this.store.tickers;

    // Build via indexed for-loop + push instead of `reduce + [...acc, t]`.
    // The prior reduce-with-spread shape was O(N²): for 600 tickers it
    // copied ~180K element references just to build the result array.
    const tickers: Ticker[] = [];
    for (let i = 0; i < rawTickers.length; i++) {
      const t = rawTickers[i];
      const market = this.store.marketsBySymbol[t.symbol];
      if (!market) continue;

      const lastPrice = parseFloat(t.lastPrice);
      // An inverse ticker's volume24h is in USD contracts and its turnover24h in the coin.
      const volume24h = parseFloat(market.isInverse ? t.turnover24h : t.volume24h);
      tickers.push({
        id: market.id,
        symbol: market.symbol,
        bid: parseFloat(t.bid1Price),
        ask: parseFloat(t.ask1Price),
        last: lastPrice,
        mark: parseFloat(t.markPrice),
        index: parseFloat(t.indexPrice),
        percentage: parseFloat(t.price24hPcnt) * 100,
        openInterest: parseFloat(t.openInterest),
        fundingRate: market.isExpiring ? 0 : parseFloat(t.fundingRate),
        fundingTime: market.isExpiring ? 0 : parseFloat(t.nextFundingTime),
        volume: volume24h,
        quoteVolume: market.isInverse ? parseFloat(t.volume24h) : volume24h * lastPrice,
      });
    }
    return tickers;
  };

  fetchMarkets = async () => {
    try {
      const markets = await this.fetchMarketsHelper('linear');
      if (this.inverseEnabled()) markets.push(...(await this.fetchMarketsHelper('inverse')));
      return this.optionsEnabled() ? [...markets, ...(await this.fetchOptionMarkets())] : markets;
    } catch (e) {
      if (e instanceof BybitTradingMarketsRequestError && this.store.markets.length > 0) {
        this.log('Failed to refresh Bybit markets, using cached markets', LogSeverity.Warning);
        return this.store.markets;
      }
      throw e;
    }
  };

  /** Best-effort: a failed refresh keeps the option markets already loaded, so positions keep their market. */
  fetchOptionMarkets = async (): Promise<Market[]> => {
    try {
      const { data: coins } = await this.xhr.get(ENDPOINTS.OPTION_BASE_COINS);
      if (+v(coins, 'retCode') !== 0) throw new Error(v(coins, 'retMsg'));
      const baseCoins: string[] = (coins.result?.list ?? []).map((c: Record<string, any>) => c.baseCoin);
      const markets = await mapSeries(baseCoins, async (baseCoin) => {
        const rows: Array<Record<string, any>> = [];
        let cursor = '';
        do {
          const { data } = await this.xhr.get(ENDPOINTS.MARKETS, {
            params: { category: 'option', baseCoin, limit: 1000, cursor },
          });
          if (+v(data, 'retCode') !== 0) throw new Error(v(data, 'retMsg'));
          rows.push(...(data.result?.list ?? []));
          cursor = data.result?.nextPageCursor ?? '';
        } while (cursor);
        return rows.flatMap((row) => mapBybitOptionInstrument(row) ?? []);
      });
      return markets.flat();
    } catch (err: any) {
      this.log(
        `Failed to load Bybit option markets, keeping the last ones: ${err?.message ?? err}`,
        LogSeverity.Warning,
      );
      return this.store.markets.filter((m) => m.option);
    }
  };

  fetchOptionChain = async (query: OptionChainQuery): Promise<OptionChainRow[]> => {
    this.assertOptionsEnabled();
    const { data } = await this.xhr.get(ENDPOINTS.TICKERS, {
      params: { category: 'option', baseCoin: query.underlying },
    });
    if (+v(data, 'retCode') !== 0) throw new Error(v(data, 'retMsg'));
    const quotes = (data.result?.list ?? []).map(mapBybitOptionTicker);
    return buildOptionChain(this.store.markets, quotes, query);
  };

  fetchOptionExpiries = async (underlying: string): Promise<number[]> => {
    this.assertOptionsEnabled();
    return optionExpiries(this.store.markets, underlying);
  };

  fetchAccountGreeks = async (): Promise<AccountGreeks[]> => {
    this.assertOptionsEnabled();
    const { data } = await this.xhr.get(ENDPOINTS.COIN_GREEKS);
    if (+v(data, 'retCode') !== 0) throw new Error(v(data, 'retMsg'));
    return (data.result?.list ?? []).map(mapBybitCoinGreeks);
  };

  private assertOptionsEnabled() {
    if (!this.optionsEnabled()) throw new Error('Options are not enabled on this Bybit account');
  }

  private refuseOptionSymbol(symbol: string | undefined, what: string) {
    if (symbol && isBybitOptionSymbol(symbol)) throw new Error(`Bybit has no ${what} for options (${symbol})`);
  }

  fetchFees = async (
    category: string,
  ): Promise<
    ReadonlyArray<{
      symbol: string;
      takerFeeRate: string;
      makerFeeRate: string;
    }>
  > => {
    try {
      const { data } = await this.xhr.get(ENDPOINTS.FEE_RATE, {
        params: { category },
      });
      return data?.result?.list || [];
    } catch (_e) {
      return [];
    }
  };

  private fetchTradingMarketsResponse = async (category: string) => {
    try {
      return await this.xhr.get(ENDPOINTS.MARKETS, {
        params: { category, limit: 1000 },
      });
    } catch (error) {
      if (!isRetryableBybitTradingMarketsFailure(error)) {
        throw new BybitTradingMarketsRequestError(error);
      }

      const reason = describeBybitMarketsFailure(error);
      this.log(
        `Failed to fetch Bybit Trading markets (${reason}), retrying once in ${BYBIT_TRADING_MARKETS_RETRY_DELAY_MS}ms`,
        LogSeverity.Warning,
      );
      await sleep(BYBIT_TRADING_MARKETS_RETRY_DELAY_MS);

      try {
        return await this.xhr.get(ENDPOINTS.MARKETS, {
          params: { category, limit: 1000 },
        });
      } catch (retryError) {
        throw new BybitTradingMarketsRequestError(retryError, 1);
      }
    }
  };

  fetchMarketsHelper = async (category: string) => {
    const fees = this.isPublicDataOnly ? [] : await this.fetchFees(category);

    let preLaunchTimedOut = false;
    const preLaunchMarketsPromise = this.xhr
      .get(ENDPOINTS.MARKETS, {
        params: { category, limit: 1000, status: 'PreLaunch' },
      })
      .then(({ data: data2 }) => {
        if (v(data2, 'retMsg') === 'OK') {
          return data2.result.list || [];
        }
        if (!preLaunchTimedOut) {
          this.log('Failed to fetch Bybit PreLaunch markets, continuing with Trading markets', LogSeverity.Warning);
        }
        return [];
      })
      .catch(() => {
        if (!preLaunchTimedOut) {
          this.log('Failed to fetch Bybit PreLaunch markets, continuing with Trading markets', LogSeverity.Warning);
        }
        return [];
      });

    const tradingResponse = await this.fetchTradingMarketsResponse(category);

    const { data } = tradingResponse;
    if (v(data, 'retMsg') !== 'OK') {
      const msg = v(data, 'retMsg') || 'Failed to fetch Bybit Trading markets';
      this.emitAndLogError(msg);
      throw new Error(msg);
    }

    const preLaunchMarkets: Array<Record<string, any>> = await Promise.race([
      preLaunchMarketsPromise,
      sleep(BYBIT_PRELAUNCH_MARKETS_STARTUP_GRACE_MS).then(() => {
        preLaunchTimedOut = true;
        this.log('Timed out waiting for Bybit PreLaunch markets, continuing with Trading markets', LogSeverity.Warning);
        return [];
      }),
    ]);

    const markets: Market[] = [...data.result.list, ...preLaunchMarkets].map((market: Record<string, any>) => {
      const fee = fees.find((f) => f.symbol === market.symbol);
      return {
        // old id isn't unique with expiring usdc futs
        // id: `${market.baseCoin}/${market.quoteCoin}:${market.settleCoin}`,
        id: market.symbol,
        symbol: market.symbol,
        base: market.baseCoin,
        quote: market.quoteCoin,
        active: market.status === 'Trading' || market.status === 'PreLaunch',
        precision: {
          amount: parseFloat(market.lotSizeFilter.qtyStep),
          price: parseFloat(market.priceFilter.tickSize),
        },
        limits: {
          amount: {
            min: parseFloat(market.lotSizeFilter.minOrderQty),
            max: parseFloat(market.lotSizeFilter.maxOrderQty),
          },
          leverage: {
            min: parseFloat(market.leverageFilter.minLeverage),
            max: parseFloat(market.leverageFilter.maxLeverage),
          },
        },
        isExpiring: Boolean(parseFloat(market.deliveryTime)),
        ...(category === 'inverse' ? { isInverse: true } : {}),
        ...(fee
          ? {
              takerFee: parseFloat(fee.takerFeeRate),
              makerFee: parseFloat(fee.makerFeeRate),
            }
          : {}),
        info: market,
      };
    });

    return markets;
  };

  /**
   * Exchange-reported closed PnL — the low-multiplicity anchor, as opposed to positions
   * inferred by polling in the browser.
   *
   * This used to GET Tealstreet's own `/api/trades-history/bybit/closed-pnl`, backed by
   * `users_app_history_bybit_closed_pnl` — a table with zero rows and no producer — so it
   * always returned `[]` while looking implemented. It now calls bybit.
   */
  /**
   * RAW closed-PnL rows. Public so the journal reads this endpoint through a method rather
   * than the harness scraping the URL. Bybit caps a query at 7 days, so the window walk lives
   * here once for both consumers.
   */
  fetchClosedPositionRows = async (opts: HistoricalTradeOptions): Promise<Array<Record<string, any>>> => {
    if (this.isPublicDataOnly) return [];
    this.refuseOptionSymbol(opts.symbol, 'closed PnL');

    const category = this.getCategoryForSymbol(opts.symbol);
    const to = opts.to ?? virtualClock.getCurrentTimeMs();
    const from = opts.from ?? to - this.DEFAULT_EXECUTIONS_LOOKBACK_MS;
    const collected: Array<Record<string, any>> = [];

    for (const window of this.getExecutionQueryWindows(from, to)) {
      let cursor: string | undefined;
      let page = 0;

      do {
        const { data } = await this.xhr.get(ENDPOINTS.USER_CLOSED_PNL, {
          params: omitUndefined({
            category,
            symbol: opts.symbol,
            startTime: window.startTime,
            endTime: window.endTime,
            limit: 100,
            cursor,
          }),
        });

        if (data.retMsg !== 'OK' && +data.retCode !== 0) {
          // Throws rather than returning what it has: a partial history presented as a
          // complete one is the failure this whole surface exists to prevent.
          throw new Error(`bybit closed-PnL fetch failed: ${data.retMsg}`);
        }

        collected.push(...((data.result?.list || []) as Array<Record<string, any>>));
        cursor = data.result?.nextPageCursor || undefined;
        page += 1;
      } while (cursor && page < MAX_CLOSED_PNL_PAGES);
    }

    return collected;
  };

  /**
   * Journal records from the REPORTED closed axis.
   *
   * Uses `aggregateBybitClosedPnl`, which folds bybit's per-partial-close rows into round
   * trips — bybit is the one venue emitting one row per partial rather than per round trip.
   * Its known merge caveat (two cycles reopening at an identical entry price) rides on the
   * venue grading, not here.
   */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) => {
    const to = opts.to ?? virtualClock.getCurrentTimeMs();
    const from = opts.from ?? to - this.DEFAULT_EXECUTIONS_LOOKBACK_MS;
    const rows = await this.fetchClosedPositionRows({ from, to } as HistoricalTradeOptions);

    // LEG JOIN, and it is not optional. bybit's closed-PnL rows carry no hedge leg — only
    // the ORDER axis does, via `positionIdx`. Without it every row buckets as `both`, longs
    // and shorts merge, and the count comes out short: measured 127 records against the
    // gate's 150, restored to 150 by the join.
    //
    // The venue's separate non-zero gross residual (-0.0596) is NOT evidence for this and did
    // not move when the join landed — it is the aggregation artefact described in
    // `docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md`. Two signals were claimed here once; only
    // the count was ever measuring the leg.
    const legs = new Map<string, PositionSide | 'both'>();
    try {
      for (const order of await this.fetchHistoricalOrders({ from, to })) {
        if (order?.id && order.positionSide) legs.set(String(order.id), order.positionSide);
      }
    } catch {
      // A failed leg join must not silently produce merged records. Rethrow: an
      // under-counted history presented as complete is the failure this surface prevents.
      throw new Error('bybit: could not read the order axis to resolve hedge legs');
    }

    return aggregateBybitClosedPnl(rows, {
      account: opts.account,
      resolveSettleCurrency: (symbol: string) => this.store?.marketsBySymbol?.[symbol]?.quote || 'USDT',
      resolveLeg: legs.size ? (orderId: string) => legs.get(orderId) : undefined,
    });
  };

  fetchClosedPnl = async (opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }
    this.refuseOptionSymbol(opts.symbol, 'closed PnL');

    const category = this.getCategoryForSymbol(opts.symbol);
    const to = opts.to ?? virtualClock.getCurrentTimeMs();
    const from = opts.from ?? to - this.DEFAULT_EXECUTIONS_LOOKBACK_MS;
    const entries: ClosedPnlEntry[] = [];

    try {
      // Bybit caps a closed-PnL query at a 7-day range, same as executions.
      for (const window of this.getExecutionQueryWindows(from, to)) {
        let cursor: string | undefined;
        let page = 0;

        do {
          const { data } = await this.xhr.get(ENDPOINTS.USER_CLOSED_PNL, {
            params: omitUndefined({
              category,
              symbol: opts.symbol,
              startTime: window.startTime,
              endTime: window.endTime,
              limit: 100,
              cursor,
            }),
          });

          if (data.retMsg !== 'OK' && +data.retCode !== 0) {
            this.log(`Failed to fetch closed PnL: ${data.retMsg}`, LogSeverity.Error);
            break;
          }

          const rows = (data.result?.list || []) as Array<Record<string, any>>;
          for (const row of rows) {
            const entry = mapBybitClosedPnl(row, opts.accountName || '', isBybitInverseSymbol(row.symbol));
            if (entry) entries.push(entry);
          }

          cursor = data.result?.nextPageCursor || undefined;
          page += 1;

          // Results are newest-first, so a cap that bites drops the OLDEST rows of the
          // window — a hole in the middle of the history rather than a short tail.
          if (cursor && page >= MAX_CLOSED_PNL_PAGES) {
            this.log(
              `Closed-PnL paging hit the ${MAX_CLOSED_PNL_PAGES}-page cap for ${opts.symbol}; older rows in this window were not fetched`,
              LogSeverity.Warning,
            );
          }
        } while (cursor && page < MAX_CLOSED_PNL_PAGES);
      }

      return entries.sort((a, b) => b.closedTime - a.closedTime);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.retMsg || err?.message);
      return [];
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    this.validateCandleInterval(opts.interval);
    const limit = Math.min(opts.limit || 500, 1000);
    const interval = INTERVAL[opts.interval];
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const params = {
      category: this.getCategoryForSymbol(opts.symbol),
      symbol: opts.symbol,
      start: start.valueOf(),
      end: end.valueOf(),
      interval,
      limit,
    };

    const response = await this.xhr.get(ENDPOINTS.KLINE, { params });
    const { data } = response;

    // Validate response — without this, errors silently return empty candle arrays.
    // data can be null (transport/timeout issues) or have retCode !== 0 (Bybit API errors).
    if (!data) {
      console.error(
        `[BYBIT-KLINE] null data! response keys=${Object.keys(response).join(',')} status=${response.status} adapter=${typeof this.xhr.defaults.adapter}`,
      );
      throw new Error(`Bybit KLINE: null response data (status=${response.status})`);
    }
    if (data.retCode !== undefined && data.retCode !== 0) {
      throw new Error(`Bybit KLINE error: ${data.retMsg || 'unknown'} (retCode=${data.retCode})`);
    }
    if (!data.result?.list) {
      throw new Error(`Bybit KLINE: no candle list in response (keys=${Object.keys(data).join(',')})`);
    }

    const candles: Candle[] = orderBy(
      data.result.list.map(([open_time, open, high, low, close, volume]: string[]) => {
        return {
          timestamp: parseFloat(open_time) / 1000,
          open: parseFloat(open),
          high: parseFloat(high),
          low: parseFloat(low),
          close: parseFloat(close),
          volume: parseFloat(volume),
        };
      }),
      ['timestamp'],
      ['asc'],
    );

    return candles;
  };

  private mapOrderHistoryRowsToExecutions = (
    rows: Array<Record<string, any>>,
    symbol: string,
    orderId?: string,
    from?: number,
    to?: number,
  ): Execution[] => {
    const parseTimestampMs = (raw: any): number | undefined => {
      const value = Number(raw);
      if (!Number.isFinite(value) || value <= 0) return undefined;
      return value > 100_000_000_000 ? value : value * 1000;
    };

    const parseFeeFromDetail = (detail: any): number | undefined => {
      if (!detail) return undefined;
      let parsed = detail;
      if (typeof detail === 'string') {
        try {
          parsed = JSON.parse(detail);
        } catch {
          return undefined;
        }
      }
      if (!parsed || typeof parsed !== 'object') return undefined;

      let feeSum = 0;
      for (const value of Object.values(parsed)) {
        const fee = Number.parseFloat(String(value));
        if (Number.isFinite(fee)) {
          feeSum += Math.abs(fee);
        }
      }
      return feeSum > 0 ? feeSum : undefined;
    };

    const mapped: Execution[] = [];
    for (const row of rows) {
      const rowOrderId = `${row.orderId ?? ''}`;
      if (orderId && rowOrderId && rowOrderId !== `${orderId}`) {
        continue;
      }

      const rowStatus = `${row.orderStatus ?? ''}`.toLowerCase();
      if (rowStatus && !rowStatus.includes('filled')) {
        continue;
      }

      const qty = Number.parseFloat(`${row.cumExecQty ?? row.execQty ?? row.qty ?? '0'}`);
      if (!Number.isFinite(qty) || qty <= 0) {
        continue;
      }

      let price = Number.parseFloat(`${row.avgPrice ?? row.avgFilledPrice ?? row.price ?? '0'}`);
      if (!Number.isFinite(price) || price <= 0) {
        const cumExecValue = Number.parseFloat(`${row.cumExecValue ?? row.cumExecAmt ?? row.cumExecAmount ?? '0'}`);
        if (Number.isFinite(cumExecValue) && cumExecValue > 0 && qty > 0) {
          price = cumExecValue / qty;
        }
      }
      if (!Number.isFinite(price) || price <= 0) {
        continue;
      }

      const side = `${row.side ?? ''}`.toLowerCase();
      const isBuy = side === 'buy';
      const isSell = side === 'sell';
      if (!isBuy && !isSell) {
        continue;
      }

      const timestampMs = parseTimestampMs(row.updatedTime ?? row.createdTime ?? row.execTime ?? row.transactionTime);
      if (!timestampMs) {
        continue;
      }
      if (from !== undefined && timestampMs < from) {
        continue;
      }
      if (to !== undefined && timestampMs > to) {
        continue;
      }

      const directFee = Number.parseFloat(`${row.cumExecFee ?? row.execFee ?? row.fee ?? ''}`);
      const commission = Number.isFinite(directFee) ? Math.abs(directFee) : parseFeeFromDetail(row.cumFeeDetail);

      mapped.push(
        omitUndefined({
          timestamp: Math.floor(timestampMs / 1000),
          symbol: row.symbol || symbol,
          orderId: rowOrderId || undefined,
          price,
          qty,
          side: isBuy ? OrderSide.Buy : OrderSide.Sell,
          commission,
          commissionAsset: row.feeCurrency || row.feeCcy || row.settleCoin || row.quoteCoin || undefined,
        }),
      );
    }

    const dedupedByKey = new Map<string, Execution>();
    for (const execution of mapped) {
      const dedupeKey = `${execution.orderId || ''}:${execution.timestamp}:${execution.side}:${execution.price}:${execution.qty}`;
      if (!dedupedByKey.has(dedupeKey)) {
        dedupedByKey.set(dedupeKey, execution);
      }
    }

    return orderBy(Array.from(dedupedByKey.values()), ['timestamp'], ['desc']);
  };

  private fetchExecutionsFallbackFromOrderHistory = async (
    opts: ExecutionsOptions,
    fallbackLookbackMs: number = this.DEFAULT_EXECUTIONS_LOOKBACK_MS,
  ): Promise<Execution[]> => {
    const { symbol, orderId, from, to, limit = 50 } = opts;
    if (!symbol) return [];

    const fallbackTo = to ?? virtualClock.getCurrentTimeMs();
    const fallbackFrom = from ?? (to === undefined ? fallbackTo - fallbackLookbackMs : undefined);
    const executions: Execution[] = [];
    const MAX_RECURSION_DEPTH = 5;
    const queryWindows = this.getExecutionQueryWindows(fallbackFrom, fallbackTo);
    const recursiveFetch = async (
      queryWindow: { startTime?: number; endTime?: number },
      cursor?: string,
      depth: number = 0,
    ): Promise<void> => {
      if (depth >= MAX_RECURSION_DEPTH) {
        return;
      }

      const params: Record<string, any> = {
        category: this.getCategoryForSymbol(symbol),
        symbol,
        limit: Math.min(limit || 50, 100),
      };
      if (orderId) params.orderId = orderId;
      if (queryWindow.startTime !== undefined) params.startTime = queryWindow.startTime;
      if (queryWindow.endTime !== undefined) params.endTime = queryWindow.endTime;
      if (cursor) params.cursor = cursor;

      const { data } = await this.xhr.get(ENDPOINTS.USER_ORDERS, { params });
      const retMsg = `${v(data, 'retMsg') || 'UNKNOWN'}`;
      if (retMsg !== 'OK') {
        this.log(`Failed order-history fallback fetch executions: ${retMsg}`, LogSeverity.Warning);
        return;
      }

      const rows = Array.isArray(data?.result?.list) ? data.result.list : [];
      executions.push(...this.mapOrderHistoryRowsToExecutions(rows, symbol, orderId, from, to));

      if (data?.result?.nextPageCursor && executions.length < (limit || 50)) {
        await recursiveFetch(queryWindow, data.result.nextPageCursor, depth + 1);
      }
    };

    for (const queryWindow of queryWindows) {
      if (executions.length >= (limit || 50)) {
        break;
      }
      await recursiveFetch(queryWindow);
    }
    return orderBy(executions, ['timestamp'], ['desc']).slice(0, limit || 50);
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    // Return empty array if in public data only mode
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 50 } = opts;

    if (!symbol) {
      // Bybit requires symbol for execution queries
      return [];
    }

    const executions: Execution[] = [];
    let primaryRowsCount = 0;
    const MAX_RECURSION_DEPTH = 5;
    const queryWindows = this.getExecutionQueryWindows(from, to);
    const recursiveFetch = async (
      queryWindow: { startTime?: number; endTime?: number },
      cursor?: string,
      depth: number = 0,
    ): Promise<void> => {
      // Limit recursion depth to prevent excessive API calls
      if (depth >= MAX_RECURSION_DEPTH) {
        this.log(`fetchHistoricalFills reached max recursion depth of ${MAX_RECURSION_DEPTH}`, LogSeverity.Warning);
        return;
      }

      const params: Record<string, any> = {
        category: this.getCategoryForSymbol(symbol),
        symbol,
        limit: Math.min(limit || 50, 100),
      };

      if (orderId) {
        params.orderId = orderId;
      }
      if (queryWindow.startTime !== undefined) {
        params.startTime = queryWindow.startTime;
      }
      if (queryWindow.endTime !== undefined) {
        params.endTime = queryWindow.endTime;
      }
      if (cursor) {
        params.cursor = cursor;
      }

      try {
        const { data } = await this.xhr.get(ENDPOINTS.USER_TRADES, { params });
        const retMsg = `${v(data, 'retMsg') || 'UNKNOWN'}`;
        if (retMsg !== 'OK') {
          this.log(`Failed to fetch executions: ${retMsg}`, LogSeverity.Warning);
          return;
        }

        const executionsList = Array.isArray(data?.result?.list) ? data.result.list : [];
        primaryRowsCount += executionsList.length;

        for (const exec of executionsList) {
          // Bybit USER_TRADES can include non-fill rows (Funding/Settle/Delivery).
          // Keep chart/table fills strictly to execution rows that represent actual trades.
          // Use a blacklist for known non-trade rows instead of a strict whitelist to stay
          // resilient if Bybit introduces new trade-like execType values.
          const execType = `${exec.execType ?? ''}`.trim().toLowerCase();
          const isKnownNonTradeExecution =
            execType === 'funding' ||
            execType === 'settle' ||
            execType === 'settlement' ||
            execType === 'delivery' ||
            execType === 'sessionsettlepnl';
          if (isKnownNonTradeExecution) {
            continue;
          }

          const qty = parseFloat(exec.execQty ?? exec.closedSize ?? exec.qty ?? '0');
          let price = parseFloat(exec.execPrice ?? exec.avgPrice ?? exec.price ?? '0');
          if (!Number.isFinite(price) || price <= 0) {
            const execValue = parseFloat(exec.execValue ?? exec.closedPnl ?? '0');
            if (Number.isFinite(execValue) && execValue > 0 && Number.isFinite(qty) && qty > 0) {
              price = execValue / qty;
            }
          }
          const side = `${exec.side ?? exec.orderSide ?? ''}`.trim().toLowerCase();
          const isBuy = side === 'buy';
          const isSell = side === 'sell';
          if (!Number.isFinite(qty) || !Number.isFinite(price) || qty <= 0 || price <= 0 || (!isBuy && !isSell)) {
            continue;
          }

          const rawExecTime = Number(exec.execTime ?? exec.tradeTime ?? exec.updatedTime ?? exec.createdTime);
          if (!Number.isFinite(rawExecTime) || rawExecTime <= 0) {
            continue;
          }
          const timestamp = rawExecTime > 100_000_000_000 ? Math.floor(rawExecTime / 1000) : Math.floor(rawExecTime);

          executions.push({
            id: exec.execId,
            timestamp,
            symbol: exec.symbol || symbol,
            orderId: exec.orderId,
            price,
            qty,
            side: isBuy ? OrderSide.Buy : OrderSide.Sell,
            commission: Math.abs(parseFloat(exec.execFee ?? exec.fee ?? '0')),
            commissionAsset: exec.feeCurrency || exec.feeCoin || exec.quoteCoin,
            // `isMaker` is the only one of the three /v5/execution/list carries. A live
            // key dump (2026-08, `--fills-probe`) returned neither `execPnl` nor
            // `closedPnl` nor any position side — `closedSize` is the closest-named
            // field and is a size, not a PnL. Realized PnL comes from
            // /v5/position/closed-pnl, whose figure is NET of fees unlike the gross
            // convention `Execution.realizedPnl` documents.
            maker: typeof exec.isMaker === 'boolean' ? exec.isMaker : undefined,
          });
        }

        // Continue fetching if there's more data and we haven't reached the limit
        if (data?.result?.nextPageCursor && executions.length < (limit || 50)) {
          await recursiveFetch(queryWindow, data.result.nextPageCursor, depth + 1);
        }
      } catch (error: any) {
        this.log(`Error fetching executions: ${error.message}`, LogSeverity.Warning);
        return;
      }
    };

    for (const queryWindow of queryWindows) {
      if (executions.length >= (limit || 50)) {
        break;
      }
      await recursiveFetch(queryWindow);
    }
    if (executions.length > 0) {
      return executions;
    }

    try {
      const fallbackWarning = `fetchHistoricalFills primary path produced no valid fills for ${symbol}; falling back to ${ENDPOINTS.USER_ORDERS} (rawRows=${primaryRowsCount}, windows=${queryWindows.length}, limit=${limit}, hasOrderId=${Boolean(orderId)})`;
      this.log(fallbackWarning, LogSeverity.Warning);
      if (typeof console !== 'undefined' && typeof console.warn === 'function') {
        console.warn(`[${this.name}] ${fallbackWarning}`);
      }
      const fallbackExecutions = await this.fetchExecutionsFallbackFromOrderHistory(opts);
      if (fallbackExecutions.length > 0 || from !== undefined || orderId) {
        return fallbackExecutions;
      }

      const EXTENDED_FALLBACK_LOOKBACK_MS = 30 * 24 * 60 * 60 * 1000;
      if (to === undefined && this.DEFAULT_EXECUTIONS_LOOKBACK_MS < EXTENDED_FALLBACK_LOOKBACK_MS) {
        const extendedWarning = `fetchHistoricalFills short fallback returned no rows for ${symbol}; retrying ${Math.floor(
          EXTENDED_FALLBACK_LOOKBACK_MS / (24 * 60 * 60 * 1000),
        )}d order-history lookup for recovery`;
        this.log(extendedWarning, LogSeverity.Warning);
        if (typeof console !== 'undefined' && typeof console.warn === 'function') {
          console.warn(`[${this.name}] ${extendedWarning}`);
        }
        return await this.fetchExecutionsFallbackFromOrderHistory(opts, EXTENDED_FALLBACK_LOOKBACK_MS);
      }

      if (fallbackExecutions.length === 0) {
        this.log(`fetchHistoricalFills fallback returned no rows for ${symbol}`, LogSeverity.Warning);
      }
      return fallbackExecutions;
    } catch (fallbackErr: any) {
      this.log(
        `Failed order-history fallback fetch executions: ${fallbackErr?.message || fallbackErr}`,
        LogSeverity.Warning,
      );
      return executions;
    }
  };

  /**
   * Per-order executed quantity read straight from REST, keyed by order id.
   *
   * The chaser needs this on Bybit Demo, where the private WebSocket may never
   * deliver an order update: without a fill source the manager cannot tell a
   * filled order from a vanished one.
   *
   * Fails closed in both directions, because the chaser replaces orders on the
   * strength of this result — an under-report reads as "no fills" and overfills
   * the user's target:
   * - An id whose symbol cannot be resolved throws rather than being skipped.
   * - Executions are paged to exhaustion, not read one page deep.
   */
  fetchFillsForOrderIds = async (orderIds: string[]): Promise<Map<string, number>> => {
    const fillsByOrderId = new Map<string, number>();
    if (orderIds.length === 0) {
      return fillsByOrderId;
    }

    const requestedOrderIds = [...new Set(orderIds.map((orderId) => String(orderId)))];

    try {
      await mapSeries(requestedOrderIds, async (orderId) => {
        const symbol = this.resolveOrderSymbolForFills(orderId);
        if (!symbol) {
          throw new Error(`cannot resolve a symbol for order ${orderId}`);
        }

        const category = this.getCategoryForSymbol(symbol);
        let cursor = '';

        for (let page = 1; page <= FILL_RECONCILIATION_MAX_PAGES; page++) {
          const params: Record<string, any> = { category, symbol, orderId, limit: 50 };
          if (cursor) params.cursor = cursor;

          const { data } = await this.xhr.get(ENDPOINTS.USER_TRADES, { params });
          const retMsg = `${v(data, 'retMsg') || 'UNKNOWN'}`;
          if (retMsg !== 'OK') {
            throw new Error(retMsg);
          }

          const executionsList = Array.isArray(data?.result?.list) ? data.result.list : [];
          for (const exec of executionsList) {
            if (`${exec.orderId ?? ''}` !== orderId) {
              continue;
            }

            const qty = parseFloat(exec.execQty ?? exec.closedSize ?? exec.qty ?? '0');
            if (Number.isFinite(qty) && qty > 0) {
              fillsByOrderId.set(orderId, (fillsByOrderId.get(orderId) || 0) + qty);
            }
          }

          const nextCursor = `${data?.result?.nextPageCursor || ''}`;
          if (!nextCursor || nextCursor === cursor) {
            return;
          }
          cursor = nextCursor;
        }

        throw new Error(`fill history for order ${orderId} exceeded ${FILL_RECONCILIATION_MAX_PAGES} pages`);
      });

      return fillsByOrderId;
    } catch (error: any) {
      this.log(`Error fetching fills for order IDs: ${error.message}`, LogSeverity.Warning);
      throw error;
    }
  };

  /**
   * Fetch income/transaction history from Bybit.
   * Maps Bybit transaction log to the generic IncomeRecord format.
   *
   * @param opts - Options for filtering (symbol, type, time range)
   * @returns Array of income records
   */
  /**
   * Historical orders, INCLUDING ones that never filled — the only axis that sees them.
   * A live 50-row sample was 24 Filled, 16 Deactivated, 10 Cancelled, so most of this
   * account's order activity is invisible to fills, executions and closed positions alike.
   *
   * `symbol` is optional here, unlike `HistoricalTradeOptions`: an account-wide sweep is
   * the normal shape for backfilling this axis, and requiring a symbol would force
   * per-symbol fan-out across every market.
   */
  fetchHistoricalOrders = async (opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const MAX_PAGES = 20;
    const PAGE_LIMIT = 50;

    try {
      const orders: HistoricalOrder[] = [];
      const seen = new Set<string>();
      let page = 0;

      // MUST be windowed. Bybit REJECTS a range wider than 7 days on this endpoint
      // (retCode 10001) rather than clamping it, and the `break` below swallows that into
      // an empty list — so a 30-day ask returned ZERO orders and every consumer read it as
      // "this account placed no orders". Measured: the canonical audit's hedge-leg join
      // resolved 0 legs unchunked and 452 once split, silently degrading every hedged
      // position to `both` with no error anywhere.
      // `splitTimeRange` rather than the sibling `getExecutionQueryWindows`: referencing
      // another class-field arrow from inside this one makes `data`'s inference circular
      // (TS7022). The module-level helper has no `this` and is shared with binance, which
      // has the identical 7-day hard-reject endpoint.
      const windows =
        opts?.from !== undefined && opts?.to !== undefined
          ? splitTimeRange(opts.from, opts.to, ORDER_HISTORY_MAX_WINDOW_MS)
          : [{ startTime: opts?.from, endTime: opts?.to }];
      const categories = opts?.symbol
        ? [this.getCategoryForSymbol(opts.symbol)]
        : this.inverseEnabled()
          ? ['linear', 'inverse']
          : ['linear'];
      const queries = categories.flatMap((category) => windows.map((window) => ({ category, window })));

      for (const { category, window } of queries) {
        let cursor: string | undefined = undefined;
        for (page = 0; page < MAX_PAGES; page += 1) {
          // `params` is annotated rather than inferred: `cursor` feeds into it and is then
          // reassigned FROM the response, so letting omitUndefined's generic carry cursor's
          // type into the request makes `data` circular (TS7022).
          const params: Record<string, any> = omitUndefined({
            category,
            symbol: opts?.symbol,
            startTime: window.startTime,
            endTime: window.endTime,
            limit: PAGE_LIMIT,
            cursor,
          });
          const { data } = await this.xhr.get(ENDPOINTS.USER_ORDERS, { params });

          if (data.retMsg !== 'OK' && +data.retCode !== 0) {
            this.log(`Failed to fetch order history: ${data.retMsg}`, LogSeverity.Error);
            break;
          }

          const rows = (data.result?.list || []) as Array<Record<string, any>>;
          if (rows.length === 0) break;

          for (const row of rows) {
            const order = mapBybitHistoricalOrder(row);
            if (!order || seen.has(order.id)) continue;
            seen.add(order.id);
            orders.push(order);
          }

          cursor = data.result?.nextPageCursor || undefined;
          if (!cursor) break;
        }
      }

      if (page >= MAX_PAGES) {
        // Newest-first, so a cap that bites drops the OLDEST orders of the window.
        this.log(
          `fetchHistoricalOrders stopped at the ${MAX_PAGES}-page cap; older orders were not fetched`,
          LogSeverity.Warning,
        );
      }

      return orders
        .sort((a, b) => b.createdTime - a.createdTime)
        .slice(0, opts?.limit && opts.limit > 0 ? opts.limit : undefined);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.retMsg || err?.message);
      return [];
    }
  };

  // Both income methods live in bybit.income.ts so the inverse class, which extends
  // BaseExchange rather than this one, can reuse them with its own category. Kept as
  // instance arrow properties: capability detection reads the subclass property
  // shadowing the branded base stub, and a prototype method would not shadow it.
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    const records = (await mapSeries(this.incomeContexts, (ctx) => fetchBybitIncomeHistory(ctx, opts))).flat();
    if (!this.inverseEnabled()) return records;
    const newestFirst = records.sort((a, b) => b.timestamp - a.timestamp);
    return opts?.limit ? newestFirst.slice(0, opts.limit) : newestFirst;
  };

  /**
   * Fetch income history for a date range.
   * Bybit limits queries to 7 days per request, so we chunk the range.
   */
  fetchIncomeHistoryRange = async (startTime: number, endTime: number, incomeType?: string): Promise<IncomeRecord[]> =>
    (
      await mapSeries(this.incomeContexts, (ctx) => fetchBybitIncomeHistoryRange(ctx, startTime, endTime, incomeType))
    ).flat();

  private get incomeContexts(): BybitIncomeContext[] {
    const categories: Array<BybitIncomeContext['category']> = this.inverseEnabled()
      ? ['linear', 'inverse']
      : ['linear'];
    return categories.map((category) => ({ xhr: this.xhr, log: this.log, category, accountType: 'UNIFIED' }));
  }

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicSocketFor(opts.symbol).listenOHLCV(opts, callback);
  };

  listenOHLCVs = (
    opts: OHLCVSOptions,
    callback: (candle: Candle & { symbol: string; interval: Timeframe }) => void,
  ) => {
    const bySocket = new Map<BybitPublicWebsocket, string[]>();
    for (const symbol of opts.symbols) {
      const socket = this.publicSocketFor(symbol);
      bySocket.set(socket, [...(bySocket.get(socket) ?? []), symbol]);
    }
    for (const [socket, symbols] of bySocket) socket.listenOHLCVs({ ...opts, symbols }, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicSocketFor(symbol).listenOrderBook(symbol, callback);
  };

  _hasFullSizedPositionOppositeSide = (symbol: string, side: OrderSide, amount: number) => {
    // An order's amount is in USD contracts on an inverse market, so compare it with the position's native size.
    const size = (p: Position) => (isBybitInverseSymbol(symbol) ? (p.extra?.contractsNative ?? 0) : p.contracts);
    return side === OrderSide.Buy
      ? this.store.positions.some(
          (p) => p.symbol === symbol && p.side === PositionSide.Short && p.contracts !== 0 && size(p) >= amount,
        )
      : this.store.positions.some(
          (p) => p.symbol === symbol && p.side === PositionSide.Long && p.contracts !== 0 && size(p) >= amount,
        );
  };

  private getOrderExecutionParams = (opts: PlaceOrderOpts) => {
    try {
      return getBybitOrderExecutionParams(opts, this.getCategoryForSymbol(opts.symbol));
    } catch (error: any) {
      const message = error?.message || `${error}`;
      this.emitAndLogError(message);
      throw error;
    }
  };

  placeOrder = async (opts: BybitPlaceOrderOpts) => {
    const clientOrderId = this.resolveCallerClientOrderId(opts);
    this.validateOrderCapabilities(opts);
    const category = this.getCategoryForSymbol(opts.symbol);
    if (category === 'option') return this.placeOptionOrder(opts, clientOrderId);
    const orderExecutionParams = this.getOrderExecutionParams(opts);

    if (
      this._hasFullSizedPositionOppositeSide(opts.symbol, opts.side, opts.amount) &&
      (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)
    ) {
      return this.placeStopLossOrTakeProfit(opts);
    }

    if (opts.type === OrderType.TrailingStopLoss) {
      return this.placeTrailingStopLoss(opts);
    }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const orderPositionOpts = opts.type === OrderType.Conditional ? { ...opts, reduceOnly: false } : opts;
    const positionIdx = await this.getOrderPositionIdx(orderPositionOpts);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);

    const price = opts.price ? adjust(opts.price, pPrice) : null;
    const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
    const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;
    const timeInForce = ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce || OrderTimeInForce.GoodTillCancel];
    const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : null;

    let triggerDirection: number | undefined = undefined;
    if (triggerPrice) {
      const ticker = this.store.tickersBySymbol[opts.symbol]!;
      const priceToCompare = getBybitTriggerDirectionPrice(opts, ticker);
      if (triggerPrice > priceToCompare) {
        triggerDirection = 1;
      } else {
        triggerDirection = 2;
      }
    }

    let type = opts.type;
    if (opts.type === OrderType.Conditional) {
      if (!triggerPrice) {
        throw new Error(`Trigger price is required for conditional Bybit orders (${opts.symbol})`);
      }
      type = price ? OrderType.Limit : OrderType.Market;
    }
    if (triggerPrice && (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)) {
      type = OrderType.Market;
    }

    const req = omitUndefined({
      category,
      symbol: opts.symbol,
      side: inverseObj(ORDER_SIDE)[opts.side],
      orderType: inverseObj(ORDER_TYPE)[type],
      qty: `${amount}`,
      price:
        orderExecutionParams.bboSideType === undefined &&
        (opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price))
          ? `${price}`
          : undefined,
      stopLoss: opts.stopLoss ? `${stopLoss}` : undefined,
      takeProfit: opts.takeProfit ? `${takeProfit}` : undefined,
      reduceOnly: opts.type === OrderType.Conditional ? false : opts.reduceOnly || false,
      triggerPrice: opts.triggerPrice ? `${triggerPrice}` : undefined,
      triggerDirection: triggerDirection ? `${triggerDirection}` : undefined,
      triggerBy: getBybitTriggerBy(opts),
      slTriggerBy: opts.stopLoss
        ? opts.slTriggerType === OrderStopTriggerType.LastPrice
          ? 'LastPrice'
          : 'MarkPrice'
        : undefined,
      tpTriggerBy: opts.takeProfit
        ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
          ? 'MarkPrice'
          : 'LastPrice'
        : undefined,
      timeInForce:
        opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price) ? timeInForce : undefined,
      closeOnTrigger: opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit,
      positionIdx,
      ...orderExecutionParams,
    });

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;

    const lotSize = adjust((amount - rest) / lots, pAmount);

    const payloads = times(lots, (idx) => {
      // We want to remove stopLoss and takeProfit from the rest of the orders
      // because they are already set on the first one
      const payload = idx > 0 ? omit(req, ['stopLoss', 'takeProfit', 'slTriggerBy', 'tpTriggerBy']) : req;

      return { ...payload, qty: `${lotSize}`, orderLinkId: clientOrderId ?? this.generatedOrderLinkId() };
    });

    if (rest) payloads.push({ ...req, qty: `${rest}`, orderLinkId: clientOrderId ?? this.generatedOrderLinkId() });

    this.assertSingleNativeClientOrderId(clientOrderId, payloads.length);
    const fullfilled = await this.submitCreateOrders(payloads, opts);

    let errMsg = '';
    fullfilled.forEach((resp) => {
      if (v(resp.data, 'retMsg') !== 'OK') {
        const formattedError = this.formatBybitVenueError(resp.data);
        this.emitAndLogError(formattedError);
        errMsg = formattedError;
      }
    });

    if (errMsg !== '') {
      throw new Error(errMsg);
    }

    return fullfilled.map((resp) => {
      const orderId = resp.data.result.orderId;
      if (orderId && !resp.terminal && !resp.recorded) {
        this.recordRecentlyPlacedOrderFromOpts(orderId, opts, resp.payload);
      }
      return orderId;
    });
  };

  /**
   * Bybit option orders: limit or market only, no triggers or attached TP/SL, and
   * `orderLinkId` is required. `orderIv` takes precedence over `price` at Bybit, so the
   * price is left out when an implied volatility is given.
   */
  /**
   * Every refusal an option order can meet before a request. `placeOrders` runs it for
   * the whole batch first, so one unplaceable option rejects the batch before any write.
   */
  private assertOptionOrderPlaceable = (opts: BybitPlaceOrderOpts) => {
    if (!this.optionsEnabled()) this.rejectPreflight('Options are not enabled on this Bybit account');
    if (hasBybitSingleOrderParams(opts)) {
      this.rejectPreflight(`Bybit execution parameters are not supported on option orders (${opts.symbol})`);
    }
    if (opts.type !== OrderType.Limit && opts.type !== OrderType.Market) {
      this.rejectPreflight(`Bybit options take limit and market orders only, not ${opts.type} (${opts.symbol})`);
    }
    for (const field of ['triggerPrice', 'stopLoss', 'takeProfit'] as const) {
      if (opts[field] !== undefined) this.rejectPreflight(`Bybit options do not support ${field} (${opts.symbol})`);
    }
    if (!this.store.marketsBySymbol[opts.symbol]) this.rejectPreflight(`Market ${opts.symbol} not found`);
    const byIv = opts.impliedVolatility !== undefined;
    if (opts.type === OrderType.Market && byIv) {
      this.rejectPreflight(`Bybit takes impliedVolatility on limit option orders only (${opts.symbol})`);
    }
    if (opts.type === OrderType.Limit && !byIv && !opts.price) {
      this.rejectPreflight(`A limit option order needs a price or an impliedVolatility (${opts.symbol})`);
    }
  };

  private placeOptionOrder = async (opts: BybitPlaceOrderOpts, clientOrderId: string | undefined) => {
    this.assertOptionOrderPlaceable(opts);
    const market = this.store.marketsBySymbol[opts.symbol]!;
    const isLimit = opts.type === OrderType.Limit;
    const byIv = opts.impliedVolatility !== undefined;
    const payload = omitUndefined({
      category: 'option',
      symbol: opts.symbol,
      side: inverseObj(ORDER_SIDE)[opts.side],
      orderType: inverseObj(ORDER_TYPE)[opts.type],
      qty: `${adjust(opts.amount, market.precision.amount)}`,
      price: isLimit && !byIv ? `${adjust(opts.price!, market.precision.price)}` : undefined,
      orderIv: byIv ? `${opts.impliedVolatility}` : undefined,
      timeInForce: isLimit
        ? ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce || OrderTimeInForce.GoodTillCancel]
        : undefined,
      reduceOnly: opts.reduceOnly || false,
      orderLinkId: clientOrderId ?? nanoid(),
    });

    const [resp] = await this.submitCreateOrders([payload], opts);
    if (!resp) throw new Error(`Bybit did not answer the option order for ${opts.symbol}`);
    if (v(resp.data, 'retMsg') !== 'OK') {
      const formattedError = this.formatBybitVenueError(resp.data);
      this.emitAndLogError(formattedError);
      throw new Error(formattedError);
    }
    const orderId = resp.data.result.orderId;
    if (orderId && !resp.terminal && !resp.recorded)
      this.recordRecentlyPlacedOrderFromOpts(orderId, opts, resp.payload);
    return [orderId];
  };

  protected async submitCreateOrders(
    payloads: Record<string, any>[],
    opts: PlaceOrderOpts,
  ): Promise<BybitCreateOrderResponse[]> {
    const responses = await mapSeries(payloads, async (payload) => {
      try {
        const { data } = await this.xhr.post(ENDPOINTS.CREATE_ORDER, payload);
        return { data, payload };
      } catch (err: any) {
        this.emitAndLogError(this.formatBybitVenueError(err?.response?.data, err?.message));
        // A lost response on an order carrying the caller's client id may hide a live
        // order: throw, so the caller looks it up rather than reading `[]` as "not placed".
        if (payloads.length === 1 && opts.clientOrderId && isUnknownOutcome(err)) throw err;
        return undefined;
      }
    });
    return responses.filter((response): response is NonNullable<typeof response> => response !== undefined);
  }

  placeStopLossOrTakeProfit = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    const pAmount = market.precision.amount;
    const amount = adjust(opts.amount, pAmount);

    const calculatedAmount = amount && amount !== 0 ? amount : undefined;

    const payload: Record<string, any> = {
      category: this.getCategoryForSymbol(opts.symbol),
      symbol: opts.symbol,
      positionIdx: await this.getStopOrderPositionIdx(opts),
      tpslMode: calculatedAmount ? 'Partial' : 'Full',
    };

    if (opts.type === OrderType.StopLoss) {
      payload.stopLoss = `${opts.price}`;
      payload.slTriggerBy = opts.slTriggerType === OrderStopTriggerType.LastPrice ? 'LastPrice' : 'MarkPrice';

      if (calculatedAmount) {
        payload.slSize = `${calculatedAmount}`;
      }
    }

    if (opts.type === OrderType.TakeProfit) {
      payload.takeProfit = `${opts.price}`;
      payload.tpTriggerBy = opts.tpTriggerType === OrderStopTriggerType.MarkPrice ? 'MarkPrice' : 'LastPrice';

      if (calculatedAmount) {
        payload.tpSize = `${calculatedAmount}`;
      }
    }

    const { data } = await this.xhr.post(ENDPOINTS.SET_TRADING_STOP, payload);

    if (isBybitOrderNotModifiedMessage(data.retMsg)) {
      this.scheduleSetTradingStopSnapshotRefresh();
      return [];
    }

    if (data.retMsg !== 'OK') {
      const formattedError = this.formatBybitVenueError(data);
      this.emitAndLogError(formattedError);
      throw new Error(formattedError);
    }

    // SET_TRADING_STOP returns no orderId. Force an order snapshot so partial
    // TP/SL rows from /v5/order/realtime can replace optimistic UI rows even
    // when private WS looks healthy but is degraded or geo-flaky.
    this.scheduleSetTradingStopSnapshotRefresh();
    return [];
  };

  placeTrailingStopLoss = async (opts: PlaceOrderOpts) => {
    const ticker = this.store.tickersBySymbol[opts.symbol];
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!ticker || !market) {
      throw new Error(`Ticker ${opts.symbol} not found`);
    }

    const distance = adjust(
      Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
      market.precision.price,
    );

    const payload: Record<string, any> = {
      category: this.getCategoryForSymbol(opts.symbol),
      symbol: opts.symbol,
      positionIdx: await this.getStopOrderPositionIdx(opts),
      trailingStop: `${distance}`,
    };

    const { data } = await this.xhr.post(ENDPOINTS.SET_TRADING_STOP, payload);

    if (isBybitOrderNotModifiedMessage(data.retMsg)) {
      this.scheduleSetTradingStopSnapshotRefresh();
      return [];
    }

    if (data.retMsg !== 'OK') {
      const formattedError = this.formatBybitVenueError(data);
      this.emitAndLogError(formattedError);
      throw new Error(formattedError);
    }

    // SET_TRADING_STOP returns no orderId; see placeStopLossOrTakeProfit.
    this.scheduleSetTradingStopSnapshotRefresh();
    return [];
  };

  placeOrders = async (orders: BybitPlaceOrderOpts[]): Promise<string[]> => {
    if (orders.length === 0) {
      return [];
    }

    // Explicit execution controls and caller client IDs must reject atomically
    // before any request; the special-order loop continues after transport errors.
    this.assertCallerClientOrderIds(orders);
    for (const order of orders) {
      if (isBybitOptionSymbol(order.symbol)) this.assertOptionOrderPlaceable(order);
      else this.getOrderExecutionParams(order);
    }

    // Separate orders that need special handling
    const specialOrders: PlaceOrderOpts[] = [];
    const regularOrders: PlaceOrderOpts[] = [];

    for (const order of orders) {
      // Check if order needs special handling (stop loss/take profit with existing position or trailing stop)
      if (
        (this._hasFullSizedPositionOppositeSide(order.symbol, order.side, order.amount) &&
          (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit)) ||
        order.type === OrderType.TrailingStopLoss ||
        hasBybitSingleOrderParams(order) ||
        isBybitOptionSymbol(order.symbol)
      ) {
        specialOrders.push(order);
      } else {
        regularOrders.push(order);
      }
    }

    const allOrderIds: string[] = [];

    // Handle special orders individually using existing placeOrder method
    for (const order of specialOrders) {
      try {
        const orderIds = await this.placeOrder(order);
        allOrderIds.push(...orderIds);
      } catch (err: any) {
        this.emitAndLogError(`Failed to place special order: ${err.message}`);
        // Continue with other orders instead of failing completely
      }
    }

    // Handle regular orders using batch API
    if (regularOrders.length > 0) {
      const batchOrderIds = await this.placeBatchOrders(regularOrders);
      allOrderIds.push(...batchOrderIds);
    }

    return allOrderIds;
  };

  private placeBatchOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    // Group orders by category for batch processing
    const ordersByCategory = orders.reduce(
      (acc, order) => {
        const category = this.getCategoryForSymbol(order.symbol);
        if (!acc[category]) acc[category] = [];
        acc[category].push(order);
        return acc;
      },
      {} as Record<string, PlaceOrderOpts[]>,
    );

    const allOrderIds: string[] = [];

    // Process each category separately
    for (const [category, categoryOrders] of Object.entries(ordersByCategory)) {
      const categoryOrderIds = await this.placeBatchOrdersForCategory(category, categoryOrders);
      allOrderIds.push(...categoryOrderIds);
    }

    return allOrderIds;
  };

  private placeBatchOrdersForCategory = async (category: string, orders: PlaceOrderOpts[]): Promise<string[]> => {
    // Bybit rate limit: max 10 orders per second, so we limit batches to 10 orders
    // even though the API technically supports up to 20 orders per batch
    const batches = chunk(orders, this.batchLimit);
    const allOrderIds: string[] = [];

    for (let i = 0; i < batches.length; i++) {
      const batch = batches[i];

      // Rate limiting: Add 1 second delay between batches to respect 10 orders/second limit
      if (i > 0) {
        await sleep(1_200);
      }

      try {
        const batchOrderIds = await this.processBatch(category, batch);
        allOrderIds.push(...batchOrderIds);
      } catch (err: any) {
        this.emitAndLogError(`Failed to place batch orders: ${err.message}`);
        // Fallback to individual order placement for this batch
        for (const order of batch) {
          try {
            const orderIds = await this.placeOrder(order);
            allOrderIds.push(...orderIds);
          } catch (individualErr: any) {
            this.emitAndLogError(`Failed to place individual order: ${individualErr.message}`);
          }
        }
      }
    }

    return allOrderIds;
  };

  private processBatch = async (category: string, orders: PlaceOrderOpts[]): Promise<string[]> => {
    // Convert orders to batch request format
    const batchRequests = await Promise.all(
      orders.map(async (order) => ({
        order,
        request: await this.formatOrderForBatch(order),
      })),
    );

    // Filter out any null requests (orders that couldn't be formatted)
    const validRequests = batchRequests.filter(
      (entry): entry is { order: PlaceOrderOpts; request: Record<string, any> } => entry.request !== null,
    );

    if (validRequests.length === 0) {
      return [];
    }

    const payload = {
      category,
      // Keep TP/SL on every request so scaled batches can attach one TP/SL pair per order.
      request: validRequests.map(({ request }) => request),
    };

    const { data } = await this.xhr.post(ENDPOINTS.CREATE_BATCH_ORDER, payload);

    if (data.retMsg !== 'OK') {
      const formattedError = this.formatBybitVenueError(data);
      this.emitAndLogError(formattedError);
      throw new Error(formattedError);
    }

    // Check for individual order errors
    const errors: string[] = [];
    const orderIds: string[] = [];

    if (data.retExtInfo?.list) {
      data.retExtInfo.list.forEach((extInfo: any, index: number) => {
        if (extInfo.code !== 0) {
          errors.push(`Order ${index + 1}: ${this.formatBybitVenueError(extInfo)}`);
        }
      });
    }

    if (data.result?.list) {
      data.result.list.forEach((result: any, index: number) => {
        const request = validRequests[index];
        if (result.orderId) {
          orderIds.push(result.orderId);
          if (request) {
            this.recordRecentlyPlacedOrderFromOpts(result.orderId, request.order, request.request);
          }
        }
      });
    }

    if (errors.length > 0) {
      this.emitAndLogError(`Some orders failed: ${errors.join(', ')}`);
    }

    return orderIds;
  };

  private formatOrderForBatch = async (opts: PlaceOrderOpts): Promise<Record<string, any> | null> => {
    try {
      const market = this.store.marketsBySymbol[opts.symbol];

      if (!market) {
        this.emitAndLogError(`Market ${opts.symbol} not found`);
        return null;
      }

      const orderPositionOpts = opts.type === OrderType.Conditional ? { ...opts, reduceOnly: false } : opts;
      const positionIdx = await this.getOrderPositionIdx(orderPositionOpts);

      const pPrice = market.precision.price;
      const pAmount = market.precision.amount;

      const amount = adjust(opts.amount, pAmount);
      const price = opts.price ? adjust(opts.price, pPrice) : null;
      const stopLoss = opts.stopLoss ? adjust(opts.stopLoss, pPrice) : null;
      const takeProfit = opts.takeProfit ? adjust(opts.takeProfit, pPrice) : null;
      const timeInForce = ORDER_TIME_IN_FORCE_OUTBOUND[opts.timeInForce || OrderTimeInForce.GoodTillCancel];
      const triggerPrice = opts.triggerPrice ? adjust(opts.triggerPrice, pPrice) : null;

      let triggerDirection: number | undefined = undefined;
      if (triggerPrice) {
        const ticker = this.store.tickersBySymbol[opts.symbol]!;
        const priceToCompare = getBybitTriggerDirectionPrice(opts, ticker);
        if (triggerPrice > priceToCompare) {
          triggerDirection = 1;
        } else {
          triggerDirection = 2;
        }
      }

      let type = opts.type;
      if (opts.type === OrderType.Conditional) {
        if (!triggerPrice) {
          throw new Error(`Trigger price is required for conditional Bybit orders (${opts.symbol})`);
        }
        type = price ? OrderType.Limit : OrderType.Market;
      }
      if (triggerPrice && (opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit)) {
        type = OrderType.Market;
      }

      const req = omitUndefined({
        symbol: opts.symbol,
        side: inverseObj(ORDER_SIDE)[opts.side],
        orderType: inverseObj(ORDER_TYPE)[type],
        qty: `${amount}`,
        price: opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price) ? `${price}` : undefined,
        stopLoss: opts.stopLoss ? `${stopLoss}` : undefined,
        takeProfit: opts.takeProfit ? `${takeProfit}` : undefined,
        reduceOnly: opts.type === OrderType.Conditional ? false : opts.reduceOnly || false,
        triggerPrice: opts.triggerPrice ? `${triggerPrice}` : undefined,
        triggerDirection: triggerDirection ? `${triggerDirection}` : undefined,
        triggerBy: getBybitTriggerBy(opts),
        slTriggerBy: opts.stopLoss
          ? opts.slTriggerType === OrderStopTriggerType.LastPrice
            ? 'LastPrice'
            : 'MarkPrice'
          : undefined,
        tpTriggerBy: opts.takeProfit
          ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
            ? 'MarkPrice'
            : 'LastPrice'
          : undefined,
        timeInForce:
          opts.type === OrderType.Limit || (opts.type === OrderType.Conditional && price) ? timeInForce : undefined,
        closeOnTrigger: opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit,
        positionIdx,
        orderLinkId: opts.clientOrderId,
      });

      return req;
    } catch (err: any) {
      this.emitAndLogError(`Failed to format order for batch: ${err.message}`);
      return null;
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    // Virtual TP/SL are bracket orders attached to a parent (e.g. limit order); we update via REPLACE_ORDER on parent, not cancel+place
    const isVirtualSLorTP = order.id.endsWith('__stop_loss') || order.id.endsWith('__take_profit');

    if (order.type === OrderType.TrailingStopLoss) {
      await this.cancelOrders([order]);
      const orderIds = await this.placeTrailingStopLoss({ ...order, ...update });
      this.applyLocalOrderUpdateAfterRestWrite(order, update);
      return orderIds;
    }

    // Cancel+replace for non-virtual, position-bound TP/SL (created via SET_TRADING_STOP).
    // These have tpslMode set to 'Full' or 'Partial' in their info.
    // Free-floating conditional stops (no tpslMode) are handled via REPLACE_ORDER below.
    // projectedType ensures we re-place with correct TP vs SL based on new price vs mark,
    // avoiding duplicate-order issues with Bybit AMEND/EDIT.
    if (
      !isVirtualSLorTP &&
      (order.type === OrderType.TakeProfit || order.type === OrderType.StopLoss) &&
      isBybitPositionTpslMode(order.info?.tpslMode)
    ) {
      const mark = this.store.tickersBySymbol[order.symbol]?.mark;
      if (mark) {
        const newPrice = 'price' in update ? update.price : order.price;
        const projectedType =
          order.side === OrderSide.Sell
            ? newPrice > mark
              ? OrderType.TakeProfit
              : OrderType.StopLoss
            : newPrice <= mark
              ? OrderType.TakeProfit
              : OrderType.StopLoss;
        await this.cancelOrders([order]);
        const orderIds = await this.placeStopLossOrTakeProfit({ ...order, ...update, type: projectedType });
        this.applyLocalOrderUpdateAfterRestWrite(order, { ...update, type: projectedType });
        return orderIds;
      }
    }

    const updatedOrderIds = [] as string[];

    // Special use-case when position is not open yet
    // and we want to update the stop loss or take profit
    // we need to do it on the original ask/bid order

    let payload: Record<string, any> = {};

    if ('price' in update && isVirtualSLorTP) {
      const og = this.store.orders.find(
        (o) => o.id === order.id.replace('__stop_loss', '').replace('__take_profit', ''),
      );

      if (og) {
        const key = order.id.endsWith('__stop_loss') ? 'stopLoss' : 'takeProfit';

        payload = {
          category: this.getCategoryForSymbol(order.symbol),
          orderId: og.id,
          symbol: order.symbol,
          [key]: `${update.price}`,
        };

        try {
          const { data } = await this.xhr.post(ENDPOINTS.REPLACE_ORDER, payload);

          if (data.retMsg === 'OK') {
            updatedOrderIds.push(data.result.orderId);
            this.applyLocalOrderUpdateAfterRestWrite(order, update);
          } else if (data.retMsg === 'order not modified') {
            // do nothing, it's ok
          } else {
            this.emitAndLogError(data.retMsg);
            throw new Error(data.retMsg);
          }
        } catch (err: any) {
          this.emitAndLogError(err, err?.response?.data?.retMsg || err.message);
          throw err;
        }
      }
    }

    // Free-floating conditional stops: non-virtual SL/TP without tpslMode.
    // Position-bound stops have tpslMode set and are handled above via cancel+replace.
    const isConditionalOrder =
      !isVirtualSLorTP &&
      (order.type === OrderType.StopLoss ||
        order.type === OrderType.TakeProfit ||
        order.type === OrderType.Conditional) &&
      !isBybitPositionTpslMode(order.info?.tpslMode);

    // If we want to update the price or amount of a limit order
    // we can do it directly on the order
    if (order.type === OrderType.Limit || isConditionalOrder) {
      payload = {
        category: this.getCategoryForSymbol(order.symbol),
        orderId: order.id,
        symbol: order.symbol,
      };

      if ('amount' in update) payload.qty = `${update.amount}`;
      if ('price' in update) {
        if (isConditionalOrder) {
          payload.triggerPrice = `${update.price}`;
        } else {
          payload.price = `${update.price}`;
        }
      }

      const { data } = await this.xhr.post(ENDPOINTS.REPLACE_ORDER, payload);

      if (data.retMsg === 'OK') {
        updatedOrderIds.push(data.result.orderId);
        this.applyLocalOrderUpdateAfterRestWrite(order, update);
      } else if (data.retMsg === 'order not modified') {
        // do nothing, it's ok
      } else {
        this.emitAndLogError(data.retMsg);
      }
    }

    // Fallback for position-bound TP/SL when cancel+replace couldn't run
    // (e.g., mark price unavailable). Includes tpslMode and size for partial stops.
    if (
      !isConditionalOrder &&
      !isVirtualSLorTP &&
      (order.type === OrderType.StopLoss || order.type === OrderType.TakeProfit)
    ) {
      const isPartial = order.amount !== 0;
      payload = {
        category: this.getCategoryForSymbol(order.symbol),
        symbol: order.symbol,
        positionIdx: await this.getOrderPositionIdx(order),
        tpslMode: isPartial ? 'Partial' : 'Full',
      };

      if ('price' in update) {
        if (order.type === OrderType.StopLoss) {
          payload.stopLoss = `${update.price}`;
        } else if (order.type === OrderType.TakeProfit) {
          payload.takeProfit = `${update.price}`;
        }
      }

      if (isPartial) {
        const size = 'amount' in update ? update.amount : order.amount;
        if (order.type === OrderType.StopLoss) {
          payload.slSize = `${size}`;
        } else if (order.type === OrderType.TakeProfit) {
          payload.tpSize = `${size}`;
        }
      }

      const { data } = await this.xhr.post(ENDPOINTS.SET_TRADING_STOP, payload);

      if (data.retMsg === 'order not modified') {
        // do nothing, it's ok
      } else if (data.retMsg !== 'OK') {
        this.emitAndLogError(data.retMsg);
      } else {
        this.applyLocalOrderUpdateAfterRestWrite(order, update);
      }
    }

    return updatedOrderIds;
  };

  cancelOrders = async (orders: Order[]) => {
    // Drop virtual TP/SL display rows — their IDs are synthetic
    // (`{parentId}__stop_loss` / `__take_profit`) and Bybit's cancel API
    // returns "order not exists or too late to cancel" for them. The bracket
    // fields hang off the parent limit order, so the parent's cancellation
    // already drops them. Without this filter, batch cancels containing
    // bracket-attached limits throw entirely on UNIFIED accounts.
    // An explicit cancel counts as a sighting: the snapshot-preserve grace
    // window must never resurrect an order we deliberately cancelled.
    for (const order of orders) this.markOrderSeen(order.id);
    const filteredOrders = orders.filter((o) => !o.id.endsWith('__stop_loss') && !o.id.endsWith('__take_profit'));
    if (filteredOrders.length === 0) return;

    // orders by category
    const ordersByCategory = filteredOrders.reduce(
      (acc, order) => {
        const category = this.getCategoryForSymbol(order.symbol);
        if (!acc[category]) acc[category] = [];
        acc[category].push(order);
        return acc;
      },
      {} as Record<string, Order[]>,
    );

    await forEachSeries(Object.entries(ordersByCategory), async ([category, orders]) => {
      await this.cancelOrdersHelper(orders, category);
    });
  };

  cancelOrdersHelper = async (orders: Order[], category: string) => {
    // group into lots of 10
    const lots = chunk(orders, 10);

    await forEachSeries(lots, async (orders, i) => {
      // TODO proper rate limiting
      if (i > 0) await sleep(1000);

      const { data } = await this.unlimitedXHR.post(ENDPOINTS.CANCEL_ORDERS, {
        category,
        request: orders.map((o) => ({ symbol: o.symbol, orderId: o.id })),
      });
      if (data.retMsg !== 'OK') {
        if (this.isAlreadyGoneCancelMessage(data.retMsg)) {
          // Already gone is still gone — retire the rows.
          this.retireCancelledOrders(orders);
          return;
        }
        this.emitAndLogError(data.retMsg);
        throw new Error(data.retMsg);
      }
      const foundErr = data.retExtInfo?.list.find(
        (l: any) => l.code !== 0 && !this.isAlreadyGoneCancelMessage(l.msg),
      );
      if (foundErr) {
        this.emitAndLogError(foundErr.msg);
        throw new Error(foundErr.msg);
      }

      this.retireCancelledOrders(orders);
    });
  };

  cancelSymbolOrders = async (symbol: string, cancelTpSl = true) => {
    const tpOrSLOrders = cancelTpSl
      ? []
      : this.store.orders.filter((o) => o.symbol === symbol && o.type !== OrderType.Limit);

    // Snapshot the rows this cancel is about to invalidate, before the TP/SL
    // re-placement below introduces new ones that must survive.
    const cancelledRows = this.store.orders
      .filter((o) => o.symbol === symbol)
      .map(({ id, symbol: orderSymbol }) => ({ id, symbol: orderSymbol }));

    const { data } = await this.unlimitedXHR.post(ENDPOINTS.CANCEL_SYMBOL_ORDERS, {
      category: this.getCategoryForSymbol(symbol),
      symbol,
    });

    // we need to re-create TP/SL after cancel all
    // before bybit was not canceling them
    if (tpOrSLOrders.length > 0) {
      await this.placeOrders(
        tpOrSLOrders.map((o) => ({
          ...o,
          side: o.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        })),
      );
    }

    const cancelAllRetMsg = v(data, 'retMsg');
    if (cancelAllRetMsg !== 'OK') {
      // Cancel-all races can land after every open order has already filled,
      // in which case the venue replies with a gone-phrase instead of OK.
      // Treat that as a soft success (still retire the rows) rather than a
      // user-visible error, matching the single-cancel path.
      if (!this.isAlreadyGoneCancelMessage(cancelAllRetMsg)) {
        this.emitAndLogError(cancelAllRetMsg);
        return;
      }
    }

    this.retireCancelledOrders(cancelledRows);
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    this.refuseOptionSymbol(symbol, 'leverage');
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    let foundPositions = this.store.positions.filter((p) => p.symbol === symbol);
    if (foundPositions.length === 0) {
      foundPositions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(foundPositions);
    }
    const currentLeverage = foundPositions[0]?.leverage ?? 1;

    const leverage = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);

    if (currentLeverage !== leverage) {
      const { data } = await this.xhr.post(ENDPOINTS.SET_LEVERAGE, {
        category: this.getCategoryForSymbol(symbol),
        symbol,
        buyLeverage: `${leverage}`,
        sellLeverage: `${leverage}`,
      });

      if (v(data, 'retMsg') !== 'OK' && !isBybitLeverageNotModified(data)) {
        this.emitAndLogError(v(data, 'retMsg'));
        throw new Error(v(data, 'retMsg'));
      }

      const positions = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(positions);
    }
  };

  changePositionMode = async (hedged: boolean, symbol?: string) => {
    this.refuseOptionSymbol(symbol, 'position mode');
    if (symbol && isBybitInverseSymbol(symbol)) {
      throw new Error(`Bybit position mode can only be changed on linear markets here (${symbol})`);
    }
    symbol = undefined;
    if (symbol) {
      // linear only supports USDT for changing pos modes
      const category = this.getCategoryForSymbol(symbol);
      await this.changePositionModeHelper(hedged, category, 'USDT');
    } else {
      // linear only supports linear usdt for bulk changing pos modes
      await this.changePositionModeHelper(hedged, 'linear', 'USDT');
    }
  };

  changePositionModeHelper = async (hedged: boolean, category?: string, coin?: string, symbol?: string) => {
    const { data } = await this.xhr.post(ENDPOINTS.SET_POSITION_MODE, {
      category,
      coin,
      symbol,
      mode: hedged ? 3 : 0,
    });

    const positions = await this.fetchPositions();
    this.store.update({ positions });
    if (symbol) {
      const symbolPos = await this.fetchPositions([symbol]);
      this.store.addOrUpdatePositions(symbolPos);
    }

    if (data.retMsg === 'All symbols switched successfully.') {
    } else if (
      data.retMsg === 'Partial symbols switched successfully, excluding symbols with open orders or positions.'
    ) {
      this.emitAndLogError(data.retMsg);
    } else {
      this.emitAndLogError(data.retMsg);
      throw new Error(data.retMsg);
    }
  };

  changeMarginType = async (isolated: boolean, _symbol?: string) => {
    const { data } = await this.xhr.post(ENDPOINTS.SET_MARGIN_MODE, {
      setMarginMode: isolated ? 'ISOLATED_MARGIN' : 'REGULAR_MARGIN',
    });

    // Success is retCode 0 with no reasons. Do not compare retMsg: this
    // endpoint answers a successful switch with "Request accepted", not "OK".
    const reasons = Array.isArray(data?.result?.reasons) ? data.result.reasons : [];
    if (+v(data, 'retCode') !== 0 || reasons.length > 0) {
      const reasonMessage = reasons
        .map((reason: any) => reason?.reasonMsg)
        .filter(Boolean)
        .join(', ');
      const errorMsg = reasonMessage || v(data, 'retMsg') || 'Failed to switch margin type';
      this.emitAndLogError(errorMsg);
      throw new Error(errorMsg);
    }

    this.marginType = isolated ? MarginType.Isolated : MarginType.Cross;
    const positions = await this.fetchPositions();
    this.store.update({ positions });
  };

  mapPosition(p: Record<string, any>) {
    const entryPrice = parseFloat(v(p, 'avgPrice') || v(p, 'entryPrice') || 0);
    let contracts = parseFloat(p.size ?? 0);
    const side =
      p.positionIdx === 1
        ? PositionSide.Long
        : p.positionIdx === 2
          ? PositionSide.Short
          : POSITION_SIDE[p.side] || PositionSide.Long;

    let unrealizedPnl = parseFloat(v(p, 'unrealisedPnl') || 0);
    const market = this.store.marketsBySymbol[p.symbol];
    const contractsNative = contracts;
    let notional = parseFloat(v(p, 'positionValue') || 0);
    const inverse = isBybitInverseSymbol(p.symbol);
    if (inverse) {
      // `size` is USD contracts. `contracts` is the coin amount at the mark, and `unrealizedPnl` stays in the coin,
      // both as on `bybitinverse`.
      const ticker = this.store.tickersBySymbol[p.symbol];
      const referencePrice = ticker?.mark || parseFloat(v(p, 'markPrice') || 0) || ticker?.last || entryPrice;
      if (referencePrice > 0) contracts = Math.round((contractsNative / referencePrice) * 1e8) / 1e8;
      notional = contractsNative;
    } else if (!p.symbol.endsWith('USDT')) {
      const ticker = this.store.tickersBySymbol[p.symbol];
      if (market && market.quote === 'USDC' && ticker) {
        const direction = side === PositionSide.Long ? 1 : -1;
        unrealizedPnl = direction * (ticker.mark * contracts - entryPrice * contracts);
      }
    }

    const positionMode =
      p.positionIdx === undefined ? undefined : p.positionIdx === 0 ? PositionMode.OneWay : PositionMode.Hedged;
    // const marginType =
    //   p.tradeMode === undefined ? undefined : p.tradeMode === 1 ? MarginType.Isolated : MarginType.Cross;

    const marginType = this.marginType;
    const positionIM = parseFloat(v(p, 'positionIM') || 0);
    const position: Position = {
      id: createPositionId(p.symbol, side, positionMode),
      symbol: p.symbol,
      side,
      entryPrice,
      notional,
      leverage: parseFloat(p.leverage),
      unrealizedPnl,
      realizedPnl: parseFloat(v(p, 'curRealisedPnl') || 0),
      contracts,
      liquidationPrice: parseFloat(v(p, 'liqPrice') || 0),
      marginSize: positionIM > 0 ? positionIM : undefined,
      positionMode,
      marginType,
      extra: {
        contractsNative,
        ...(inverse ? { unrealizedPnlNative: unrealizedPnl } : {}),
      },
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };
    if (isBybitOptionSymbol(p.symbol)) {
      const greeks = mapBybitOptionGreeks(p);
      // Options carry no leverage; 1 (not 0) keeps notional / leverage finite for margin math.
      return { ...position, leverage: 1, liquidationPrice: 0, option: greeks ? { greeks } : {} };
    }

    return position;
  }

  private inferProtectiveOrderSide(o: Record<string, any>): OrderSide | undefined {
    const positionIdx = Number(v(o, 'positionIdx'));
    if (positionIdx === 1) return OrderSide.Sell;
    if (positionIdx === 2) return OrderSide.Buy;

    const symbol = v(o, 'symbol');
    if (!symbol) return undefined;

    const openPositions = this.store.positions.filter(
      (position) => position.symbol === symbol && Math.abs(position.contracts) > 0,
    );
    if (openPositions.length !== 1) return undefined;

    return openPositions[0].side === PositionSide.Long ? OrderSide.Sell : OrderSide.Buy;
  }

  mapOrder(o: Record<string, any>) {
    const stopOrderType = v(o, 'stopOrderType');
    const reduceOnly = v(o, 'reduceOnly') === true || v(o, 'reduceOnly') === 'true';
    const closeOnTrigger = v(o, 'closeOnTrigger') === true || v(o, 'closeOnTrigger') === 'true';
    const tpslMode = v(o, 'tpslMode');
    const isPositionTpslMode = isBybitPositionTpslMode(tpslMode);
    const createTypeOrderType = isPositionTpslMode
      ? POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE[v(o, 'createType')]
      : undefined;
    const isStop = Boolean((stopOrderType && stopOrderType !== 'UNKNOWN') || createTypeOrderType);

    const triggerPrice = v(o, 'triggerPrice');
    const oPrice = isStop ? triggerPrice || o.price : o.price;
    const oType = createTypeOrderType || (isStop ? stopOrderType : v(o, 'orderType'));
    const isOpeningTriggerOrder =
      isStop &&
      !reduceOnly &&
      !closeOnTrigger &&
      !isPositionTpslMode &&
      (parseFloat(triggerPrice ?? '0') > 0 || v(o, 'createType') === 'CreateByStopOrder');
    const type = isOpeningTriggerOrder ? OrderType.Conditional : ORDER_TYPE[oType];
    const isProtectiveOrder =
      reduceOnly ||
      closeOnTrigger ||
      isPositionTpslMode ||
      type === OrderType.StopLoss ||
      type === OrderType.TakeProfit ||
      type === OrderType.TrailingStopLoss;
    const side = ORDER_SIDE[o.side] ?? (isProtectiveOrder ? this.inferProtectiveOrderSide(o) : undefined);
    const timeInForce = ORDER_TIME_IN_FORCE[o.timeInForce];

    const remainingNative = o.leavesQty ? parseFloat(o.leavesQty) : subtract(o.qty ?? 0, v(o, 'cumExecQty') ?? 0);
    const price = parseFloat(oPrice);
    const isInverse = isBybitInverseSymbol(o.symbol);

    const orders: Order[] = [
      {
        id: o.orderId,
        status: ORDER_STATUS[v(o, 'orderStatus')],
        symbol: o.symbol,
        type,
        side,
        ...this.remainingInStoreUnits(o.symbol, remainingNative, price),
        extra: { remainingNative },
        price,
        amount: (oType === 'StopLoss' || oType === 'TakeProfit') && tpslMode === 'Full' ? 0 : parseFloat(o.qty ?? 0),
        filled: parseFloat(v(o, 'cumExecQty') ?? 0),
        reduceOnly,
        close: closeOnTrigger,
        info: o,
        timeInForce,
        lastUpdate: virtualClock.getCurrentTimeMs(),
        ...(type === OrderType.Conditional && { triggerType: ORDER_TRIGGER_TYPE[v(o, 'triggerBy')] }),
        ...(type === OrderType.TakeProfit && { tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'triggerBy')] }),
        ...(type === OrderType.StopLoss && { slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'triggerBy')] }),
      },
    ];

    const sl = parseFloat(v(o, 'stopLoss'));
    const tp = parseFloat(v(o, 'takeProfit'));

    const inverseSide = orders[0].side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;

    if (sl > 0) {
      orders.push({
        ...orders[0],
        id: `${o.orderId}__stop_loss`,
        type: OrderType.StopLoss,
        side: inverseSide,
        price: sl,
        filled: 0,
        ...(isInverse ? this.remainingInStoreUnits(o.symbol, remainingNative, sl) : { remaining: orders[0].amount }),
        lastUpdate: virtualClock.getCurrentTimeMs(),
        slTriggerType: ORDER_TRIGGER_TYPE[v(o, 'slTriggerBy')],
      });
    }

    if (tp > 0) {
      orders.push({
        ...orders[0],
        id: `${o.orderId}__take_profit`,
        type: OrderType.TakeProfit,
        side: inverseSide,
        price: tp,
        filled: 0,
        ...(isInverse ? this.remainingInStoreUnits(o.symbol, remainingNative, tp) : { remaining: orders[0].amount }),
        lastUpdate: virtualClock.getCurrentTimeMs(),
        tpTriggerType: ORDER_TRIGGER_TYPE[v(o, 'tpTriggerBy')],
      });
    }

    return orders;
  }

  protected fetchPositionMode = async (symbol: string): Promise<PositionMode> => {
    const foundPosition = this.store.positions.find((p) => p.symbol === symbol);
    if (foundPosition?.positionMode) return foundPosition.positionMode;

    const { data } = await this.xhr.get(ENDPOINTS.POSITIONS, {
      params: {
        category: this.getCategoryForSymbol(symbol),
        symbol,
      },
    });

    const responseMeta = getBybitResponseMeta(data);
    const positionsList = Array.isArray(data?.result?.list) ? data.result.list : [];

    if (!responseMeta.isSuccess && positionsList.length === 0) {
      const fallbackMessage =
        responseMeta.retMsg ||
        (responseMeta.retCode !== undefined
          ? `Bybit positions request failed with code ${responseMeta.retCode}`
          : 'Bybit positions request returned malformed status payload');

      this.emitAndLogError(fallbackMessage, 'Bybit fetchPositionMode failed', {
        endpoint: ENDPOINTS.POSITIONS,
        symbol,
        retCode: responseMeta.retCode,
        retMsg: responseMeta.retMsg,
        hasExplicitStatus: responseMeta.hasExplicitStatus,
      });
      return PositionMode.OneWay;
    }

    const formattedData = positionsList.map(this.mapPosition.bind(this));
    this.store.addOrUpdatePositions(formattedData);
    return formattedData?.length > 0 ? formattedData[0].positionMode : PositionMode.OneWay;
  };

  private getOrderPositionIdx = async (opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'symbol'>) => {
    const positionMode = await this.fetchPositionMode(opts.symbol);
    if (positionMode === PositionMode.OneWay) return 0;

    let positionIdx = opts.side === OrderSide.Buy ? 1 : 2;
    if (opts.reduceOnly) positionIdx = positionIdx === 1 ? 2 : 1;

    return positionIdx;
  };

  private getStopOrderPositionIdx = async (opts: Pick<PlaceOrderOpts, 'reduceOnly' | 'side' | 'symbol'>) => {
    const positionIdx = await this.getOrderPositionIdx(omit(opts, ['reduceOnly']));
    return { 0: 0, 1: 2, 2: 1 }[positionIdx];
  };
}
