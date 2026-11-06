import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';
import type { ClientOrderIdRule } from '../../../exchanges/clientOrderId';

import rateLimit from 'axios-rate-limit';
import flatten from 'lodash/flatten';
import sumBy from 'lodash/sumBy';
import { forEachSeries, mapSeries } from 'p-iteration';

import { sleep } from '@tealstreet/sleep/src/sleep';
import { createApiWithSecondaryKey } from '../../../createApiWithSecondaryKey';
import { wooOrdersToPositionRecords } from '../../../journal/woo.position-adapter';
import { getTransport } from '../../../transport/registry';
import {
  Balance,
  Candle,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  HistoricalOrder,
  HistoricalTrade,
  HistoricalTradeOptions,
  IncomeHistoryOptions,
  IncomeRecord,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
  OrderFillEvent,
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
  timeframeToSeconds,
  UpdateOrderOpts,
  Writable,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { v } from '../../../utils/get-key';
import { getCandleStartTime } from '../../../utils/getCandleStartTime';
import { inverseObj } from '../../../utils/inverseObj';
import { loop } from '../../../utils/loop';
import { omitUndefined } from '../../../utils/omitUndefined';
import { adjust, subtract } from '../../../utils/safe-math';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange, CallableMethodsInWorker } from '../../../exchanges/base';
import { createAPI } from './woo.api';
import { buildWooIncomeParams, mapWooIncomeRecord, wooIncomeIdentity } from './woo.income';
import { buildWooLegacyFillsParams } from './woo.legacy-fills';
import { mapWooHistoricalOrder } from './woo.order-history';
import { assertWooAlgoSnapshotRow, splitWooQuantity } from './woo.order-state';
import {
  ACCOUNT_RESPONSE,
  ENDPOINTS,
  FETCH_ORDER_RESPONSE,
  GET_FUTURES_LEVERAGE_RESPONSE,
  GET_KLINE_RESPONSE,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  ORDER_SIDE,
  ORDER_TRIGGER_TYPE,
  ORDER_TYPE,
  PLACE_ALGO_ORDER_RESPONSE,
  PLACE_ORDER_RESPONSE,
} from './woo.types';
import { mapWooHedgeTransactionSide, normalizeSymbol, reverseSymbol } from './woo.utils';
import { WooPrivateWebscoket } from './woo.ws-private';
import { WooPublicWebsocket } from './woo.ws-public';

// WOO ETH perps cap crossing prices near 1% through fair price; stay below it
// while still crossing enough spread to fill market-order IOC emulation.
const DEFAULT_MARKET_IOC_SLIPPAGE_PCT = 0.9;
const DEFAULT_MARKET_IOC_PRICE_RANGE_PCT = 1;
const POSITION_METADATA_CACHE_TTL_MS = 30_000;
const POSITION_METADATA_RATE_LIMIT_BACKOFF_MS = 10_000;
const POSITION_MODE_READBACK_ATTEMPTS = 3;
const POSITION_MODE_READBACK_DELAY_MS = 250;

type WooPositionMetadata = Pick<Position, 'symbol' | 'side' | 'leverage' | 'positionMode' | 'marginType'>;

type CachedWooPositionMetadata = {
  metadata: WooPositionMetadata[];
  updatedAt: number;
};

type WooAccountInfoEnvelope = {
  success?: unknown;
  data?: unknown;
  message?: unknown;
  msg?: unknown;
  meta?: { message?: unknown };
};

const isPositiveFinitePrice = (price: number | undefined): price is number =>
  typeof price === 'number' && Number.isFinite(price) && price > 0;

const applyMarketIocSlippage = (price: number, side: OrderSide, slippagePct: number) => {
  const slippageFactor = Math.max(0, slippagePct) / 100;
  return price * (side === OrderSide.Buy ? 1 + slippageFactor : 1 - slippageFactor);
};

const chooseMarketIocOrderPrice = (
  ticker: Ticker | undefined,
  side: OrderSide,
  slippagePct: number,
  priceRangePct: number,
  touchPrice?: number,
) => {
  const sideQuote = touchPrice ?? (side === OrderSide.Buy ? ticker?.ask : ticker?.bid);
  const fairPrice = [ticker?.mark, ticker?.index, ticker?.midPrice, ticker?.last].find(isPositiveFinitePrice);

  if (isPositiveFinitePrice(fairPrice)) {
    const fairCrossingPrice = applyMarketIocSlippage(fairPrice, side, slippagePct);

    if (isPositiveFinitePrice(sideQuote)) {
      const sideQuoteLimitFactor = Math.max(0, priceRangePct) / 100;
      const sideQuoteWithinWOOPriceCap =
        side === OrderSide.Buy
          ? sideQuote <= fairPrice * (1 + sideQuoteLimitFactor)
          : sideQuote >= fairPrice * (1 - sideQuoteLimitFactor);
      const fairCrossingPriceMissesTouch =
        side === OrderSide.Buy ? fairCrossingPrice < sideQuote : fairCrossingPrice > sideQuote;

      if (sideQuoteWithinWOOPriceCap && fairCrossingPriceMissesTouch) {
        return sideQuote;
      }
    }

    return fairCrossingPrice;
  }

  const fallbackPrice =
    (isPositiveFinitePrice(sideQuote) ? sideQuote : undefined) ||
    (side === OrderSide.Buy
      ? isPositiveFinitePrice(ticker?.bid)
        ? ticker?.bid
        : undefined
      : isPositiveFinitePrice(ticker?.ask)
        ? ticker?.ask
        : undefined);

  return fallbackPrice === undefined ? undefined : applyMarketIocSlippage(fallbackPrice, side, slippagePct);
};

const getWooMarketPriceRangePct = (market: Market | undefined) => {
  const priceRange = Number(market?.info?.price_range);
  return Number.isFinite(priceRange) && priceRange > 0 ? priceRange * 100 : DEFAULT_MARKET_IOC_PRICE_RANGE_PCT;
};

const isWooRateLimitError = (err: any) =>
  err?.response?.status === 429 ||
  err?.response?.data?.code === -1003 ||
  err?.response?.data?.error_code === 90003 ||
  String(err?.response?.data?.message || err?.message || '')
    .toLowerCase()
    .includes('rate limit');

const isWooPositionModeAlreadySetError = (err: any) =>
  String(err?.response?.data?.message || err?.message || '')
    .toLowerCase()
    .includes('position mode remains unchanged');

const isPostOnlyCrossingPrice = (side: OrderSide, price: number, touchPrice: number) =>
  side === OrderSide.Buy ? price >= touchPrice : price <= touchPrice;

const isPostOnlyCrossingFairPrice = (side: OrderSide, price: number, ticker: Ticker | undefined) => {
  const fairPrice = [ticker?.mark, ticker?.index, ticker?.midPrice, ticker?.last].find(isPositiveFinitePrice);
  if (!isPositiveFinitePrice(fairPrice)) {
    return undefined;
  }

  return {
    fairPrice,
    wouldCross: side === OrderSide.Buy ? price >= fairPrice : price <= fairPrice,
  };
};

// Hedge-mode `side` values on /v3/trade/transactionHistory. Keys are
// whitespace-normalized and upper-cased before lookup.
// Mapping lives in woo.utils.ts as a pure function so the invariant is
// testable — the parser below is an instance field and unreachable from tests.

export class WOOXExchange extends BaseExchange {
  // WOO client_order_id is numeric; only a canonical JS-safe integer string survives the round trip.
  protected readonly clientOrderIdRule: ClientOrderIdRule = {
    label: 'WOO',
    // Any safe integer: a 15-digit space keeps minted IDs from colliding.
    mint: () => String(100_000_000_000_000 + Math.floor(Math.random() * 900_000_000_000_000)),
    requirement: 'be a canonical positive decimal integer within the JavaScript safe-integer range',
    isValid: (clientOrderId) => /^[1-9]\d*$/.test(clientOrderId) && Number.isSafeInteger(Number(clientOrderId)),
  };

  /**
   * Journal records fold from the ORDER axis, not fills.
   *
   * `/v1/orders?realized_pnl=true` is the only woo route carrying `realized_pnl`,
   * `total_fee` and `position_side` on one row. BOTH woo fills axes report
   * `realizedPnl: null` on every row, so a fills fold refuses every interval and returns
   * nothing — which is exactly what it did before this override existed.
   */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) => {
    const to = opts.to ?? Date.now();
    const from = opts.from ?? to - 30 * 24 * 60 * 60 * 1000;

    const orders = await this.fetchHistoricalOrders({ from, to });
    return wooOrdersToPositionRecords(orders, {
      account: opts.account,
      from,
      resolveSettleCurrency: (symbol) => this.store.marketsBySymbol[symbol]?.quote || 'USDT',
    });
  };

  private static readonly MAX_FILL_POSITION_REFRESH_ATTEMPTS = 4;
  private static readonly FILL_POSITION_REFRESH_BASE_DELAY_MS = 150;
  private static readonly FUNDING_REFRESH_INTERVAL_MS = 60_000;

  xhr: Axios;
  _primaryXhr: Axios;
  _secondaryXhr: Axios;
  _proxyEnforcedXhr: Axios;

  publicWebsocket: WooPublicWebsocket;
  privateWebsocket: WooPrivateWebscoket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  // Woo store leverage per account, not per position
  // as workaround we store the account leverage here when
  // we call the ACCOUNT endpoint, and copy this value into positions
  // private accountLeverage = 1;
  private _accountPositionMode: PositionMode | null = null;
  private _fillPositionRefreshTimers = new Map<string, ReturnType<typeof setTimeout>>();
  private _fillPositionRefreshInFlight = new Set<string>();
  private _fillExpectedPositionSignatureBySymbol = new Map<string, string>();
  private _fundingRefreshTimer?: ReturnType<typeof setInterval>;
  private _fundingRefreshInFlight = false;
  private _positionMetadataCache = new Map<string, CachedWooPositionMetadata>();
  private _positionMetadataInFlight = new Map<string, Promise<WooPositionMetadata[]>>();
  private _positionMetadataRateLimitedUntil = new Map<string, number>();
  private _incomeNextAt = 0;
  private _orderRevision = 0;
  private _orderVersions = new Map<string, number>();
  private _terminalOrders = new Map<string, { status: OrderStatus; at: number }>();

  reconcileOrder = (next: Order): Order => {
    const old = this.store.orders.find((row) => row.id === next.id);
    const terminal = this._terminalOrders.get(next.id)?.status;
    const filled = Math.max(next.filled || 0, old?.filled || 0);
    const status =
      old?.status === OrderStatus.Closed || terminal === OrderStatus.Closed
        ? OrderStatus.Closed
        : next.status === OrderStatus.Closed
          ? OrderStatus.Closed
          : old?.status === OrderStatus.Canceled || terminal === OrderStatus.Canceled
            ? OrderStatus.Canceled
            : next.status;
    return { ...next, filled, remaining: Math.max(0, subtract(next.amount, filled)), status };
  };

  acceptOrder = (next: Order) => {
    const row = this.reconcileOrder(next);
    this._orderVersions.set(row.id, ++this._orderRevision);
    if (row.status === OrderStatus.Closed || row.status === OrderStatus.Canceled) {
      this._terminalOrders.set(row.id, { status: row.status, at: virtualClock.getCurrentTimeMs() });
    }
    const cutoff = virtualClock.getCurrentTimeMs() - 300_000;
    for (const [id, value] of this._terminalOrders) {
      if (value.at < cutoff) {
        this._terminalOrders.delete(id);
        this._orderVersions.delete(id);
      }
    }
    this.store.addOrUpdateOrder(row);
    return row;
  };

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    const [xhr, primaryXhr, secondaryXhr] = createApiWithSecondaryKey(opts, this, createAPI, {
      rateLimit: (xhr) =>
        rateLimit(
          rateLimit(xhr, {
            perMilliseconds: 1000,
            maxRequests: 4,
          }),
          {
            maxRequests: 20,
            perMilliseconds: 6_000,
          },
        ),
    });

    this.xhr = xhr;
    this._primaryXhr = primaryXhr;
    this._secondaryXhr = secondaryXhr;

    this._proxyEnforcedXhr = createAPI(getTransport(this.options).requireProxy(this.options), this);

    this.publicWebsocket = new WooPublicWebsocket(this);
    this.privateWebsocket = new WooPrivateWebscoket(this);
  }

  dispose = () => {
    this._fillPositionRefreshTimers.forEach((timer) => clearTimeout(timer));
    this._fillPositionRefreshTimers.clear();
    this._fillPositionRefreshInFlight.clear();
    this._fillExpectedPositionSignatureBySymbol.clear();
    if (this._fundingRefreshTimer !== undefined) {
      clearInterval(this._fundingRefreshTimer);
      this._fundingRefreshTimer = undefined;
    }
    this._fundingRefreshInFlight = false;
    super.dispose();
  };

  private startFundingRatesRefresh = () => {
    if (this.isDisposed || this._fundingRefreshTimer !== undefined) return;

    this._fundingRefreshTimer = setInterval(() => {
      void this.refreshFundingRates();
    }, WOOXExchange.FUNDING_REFRESH_INTERVAL_MS);
  };

  private parseFiniteFundingNumber = (raw: unknown): number | undefined => {
    if ((typeof raw !== 'number' && typeof raw !== 'string') || (typeof raw === 'string' && raw.trim() === '')) {
      return undefined;
    }

    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : undefined;
  };

  private getWooResponseMessage = (response: WooAccountInfoEnvelope, fallback: string) => {
    const message = [response.message, response.msg, response.meta?.message].find(
      (value): value is string => typeof value === 'string' && value.trim().length > 0,
    );
    return message || fallback;
  };

  private fetchAccountInfo = async (): Promise<ACCOUNT_RESPONSE> => {
    const { data: response } = await this.xhr.get<WooAccountInfoEnvelope>(ENDPOINTS.ACCOUNT);

    if (!response || typeof response !== 'object') {
      throw new Error('WOO X account info response has an invalid shape');
    }

    if (response.success !== true) {
      throw new Error(this.getWooResponseMessage(response, 'WOO X account info request failed'));
    }

    if (!response.data || typeof response.data !== 'object') {
      throw new Error(this.getWooResponseMessage(response, 'WOO X account info response is missing data'));
    }

    const accountInfo = response.data as ACCOUNT_RESPONSE;
    if (accountInfo.positionMode !== 'ONE_WAY' && accountInfo.positionMode !== 'HEDGE_MODE') {
      throw new Error('WOO X account info response has an invalid position mode');
    }

    return accountInfo;
  };

  private refreshFundingRates = async () => {
    if (this.isDisposed || this._fundingRefreshInFlight) return;

    this._fundingRefreshInFlight = true;
    try {
      const { data } = await this.xhr.get<unknown>(ENDPOINTS.FUNDING_RATES);
      if (this.isDisposed) return;

      if (
        !data ||
        typeof data !== 'object' ||
        (data as { success?: unknown }).success !== true ||
        !Array.isArray((data as { rows?: unknown }).rows)
      ) {
        throw new Error('WOO X funding rates response has an invalid shape');
      }

      for (const row of (data as { rows: unknown[] }).rows) {
        if (this.isDisposed) return;
        if (!row || typeof row !== 'object') continue;

        const {
          symbol: rawSymbol,
          est_funding_rate: rawFundingRate,
          next_funding_time: rawFundingTime,
        } = row as {
          symbol?: unknown;
          est_funding_rate?: unknown;
          next_funding_time?: unknown;
        };
        if (typeof rawSymbol !== 'string' || !rawSymbol.startsWith('PERP_')) continue;

        const fundingRate = this.parseFiniteFundingNumber(rawFundingRate);
        const fundingTime = this.parseFiniteFundingNumber(rawFundingTime);
        if (fundingRate === undefined || fundingTime === undefined || fundingTime <= 0) continue;

        const ticker = this.store.tickersBySymbol[normalizeSymbol(rawSymbol)];
        if (!ticker || rawSymbol !== reverseSymbol(ticker.symbol)) continue;

        if (this.isDisposed) return;
        this.store.updateTicker(ticker, { fundingRate, fundingTime });
      }
    } catch (err: any) {
      if (!this.isDisposed) {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
      }
    } finally {
      this._fundingRefreshInFlight = false;
    }
  };

  async getPositionMode(): Promise<PositionMode> {
    if (!this._accountPositionMode) {
      const data = await this.fetchAccountInfo();
      this._accountPositionMode = data.positionMode === 'ONE_WAY' ? PositionMode.OneWay : PositionMode.Hedged;
    }
    this.store.setSetting('isHedged', this._accountPositionMode === PositionMode.Hedged);
    return this._accountPositionMode;
  }

  afterMethodCalledInWorker = <M extends CallableMethodsInWorker>(
    result: ReturnType<this[M]>,
    method: M,
    ...args: Parameters<this[M]>
  ) => {
    if (method === 'changePositionMode') {
      this._accountPositionMode = args[0] ? PositionMode.Hedged : PositionMode.OneWay;
    }
  };

  private normalizeTimestampSeconds = (raw: unknown): number | undefined => {
    if (raw === undefined || raw === null || raw === '') {
      return undefined;
    }

    const parsed = Number(raw);
    if (!Number.isFinite(parsed) || parsed < 0) {
      return undefined;
    }

    return parsed >= 1_000_000_000_000 ? Math.floor(parsed / 1000) : Math.floor(parsed);
  };

  private normalizeTimestampMilliseconds = (raw: unknown): number | undefined => {
    if (raw === undefined || raw === null || raw === '') {
      return undefined;
    }

    const parsed = Number(raw);
    if (!Number.isFinite(parsed) || parsed < 0) {
      return undefined;
    }

    return parsed >= 1_000_000_000_000 ? Math.floor(parsed) : Math.floor(parsed * 1000);
  };

  private normalizeTransactionSymbol = (raw: unknown): string | undefined => {
    if (typeof raw !== 'string' || raw.length === 0) {
      return undefined;
    }

    return normalizeSymbol(raw.replace(/\//g, '_'));
  };

  private normalizeRequestedSymbol = (raw?: string): string | undefined => {
    if (!raw) return undefined;
    const stripped = raw.toUpperCase().split(':').pop() || raw.toUpperCase();
    const withoutPerpSuffix = stripped
      .replace(/\.P$/, '')
      .replace(/_PERP$/, '')
      .replace(/-PERP$/, '')
      .replace(/PERP$/, '');
    return this.normalizeTransactionSymbol(withoutPerpSuffix);
  };

  private getTransactionHistorySymbolCandidates = (symbol: string): string[] => {
    const uppercaseSymbol = symbol.toUpperCase();
    const base = uppercaseSymbol.endsWith('USDT') ? uppercaseSymbol.slice(0, -4) : uppercaseSymbol;

    const candidates = [reverseSymbol(uppercaseSymbol), `${base}_USDT_PERP`, `${base}-USDT-PERP`, uppercaseSymbol];

    return Array.from(new Set(candidates.filter(Boolean)));
  };

  private parseTransactionHistoryRow = (row: Record<string, any>) => {
    const normalizedSymbol = this.normalizeTransactionSymbol(v(row, 'symbol'));
    if (!normalizedSymbol) {
      return null;
    }

    // WOO reports `side` in two vocabularies: BUY/SELL in one-way mode, and
    // OPEN|CLOSE LONG|SHORT in hedge mode. Only mapping BUY/SELL silently
    // dropped every hedge-mode fill — measured at 210 of 500 rows on a live
    // account. The hedge strings also carry the position side, which is the
    // only place transactionHistory exposes it.
    const rawSide = `${v(row, 'side') || ''}`.toUpperCase();
    const hedgeSide = mapWooHedgeTransactionSide(rawSide);
    const side =
      (hedgeSide?.side as OrderSide | undefined) ??
      (ORDER_SIDE[rawSide] || (rawSide === 'BUY' ? OrderSide.Buy : rawSide === 'SELL' ? OrderSide.Sell : undefined));
    if (side !== OrderSide.Buy && side !== OrderSide.Sell) {
      return null;
    }

    const price = Number(
      v(row, 'executedPrice') ??
        v(row, 'price') ??
        v(row, 'averageExecutedPrice') ??
        v(row, 'average_executed_price') ??
        v(row, 'lastPrice'),
    );
    const qty = Number(v(row, 'executedQuantity') ?? v(row, 'quantity') ?? v(row, 'executed') ?? v(row, 'lastQty'));
    const timestampMilliseconds = this.normalizeTimestampMilliseconds(
      v(row, 'executedTimestamp') ||
        v(row, 'executed_time') ||
        v(row, 'tradeTime') ||
        v(row, 'timestamp') ||
        v(row, 'created_time') ||
        v(row, 'createdTime') ||
        v(row, 'updated_time') ||
        v(row, 'updatedTime'),
    );
    if (
      !Number.isFinite(price) ||
      price <= 0 ||
      !Number.isFinite(qty) ||
      qty <= 0 ||
      timestampMilliseconds === undefined
    ) {
      return null;
    }

    const rawOrderId = v(row, 'orderId') ?? v(row, 'order_id') ?? v(row, 'id');
    const orderId = rawOrderId === undefined || rawOrderId === null || rawOrderId === '' ? undefined : `${rawOrderId}`;

    // Distinct from orderId: one order fills across many rows, so the per-fill
    // id is the only idempotent dedupe key (users_app_trades keys on it).
    const rawFillId = v(row, 'id') ?? v(row, 'tradeId') ?? v(row, 'trade_id');
    const fillId = rawFillId === undefined || rawFillId === null || rawFillId === '' ? undefined : `${rawFillId}`;

    const rawCommission = Number(v(row, 'fee') ?? v(row, 'total_fee'));
    const commission = Number.isFinite(rawCommission) ? rawCommission : undefined;
    const commissionAsset = v(row, 'feeAsset') ?? v(row, 'fee_asset');

    // `Number(null)` is 0, so a null realizedPnl used to surface as a real
    // zero — indistinguishable from a genuinely flat fill. WOO returns null on
    // every transactionHistory row, so treat absent as absent.
    const rawRealizedPnlValue = v(row, 'realizedPnl') ?? v(row, 'realized_pnl');
    const rawRealizedPnl =
      rawRealizedPnlValue === null || rawRealizedPnlValue === undefined ? NaN : Number(rawRealizedPnlValue);
    const realizedPnl = Number.isFinite(rawRealizedPnl) ? rawRealizedPnl : undefined;

    const rawIsMaker = v(row, 'isMaker') ?? v(row, 'is_maker');
    const maker = rawIsMaker === undefined || rawIsMaker === null ? undefined : Boolean(Number(rawIsMaker));

    const rawPositionSide = `${v(row, 'positionSide') ?? v(row, 'position_side') ?? ''}`.toUpperCase();
    const positionSide =
      rawPositionSide === 'LONG'
        ? PositionSide.Long
        : rawPositionSide === 'SHORT'
          ? PositionSide.Short
          : // transactionHistory has no positionSide field; in hedge mode the
            // side string is the only carrier of it.
            ((hedgeSide?.positionSide as PositionSide | undefined) ?? ('both' as const));

    return {
      symbol: normalizedSymbol,
      side,
      price,
      qty,
      timestampMilliseconds,
      timestampSeconds: Math.floor(timestampMilliseconds / 1000),
      orderId,
      fillId,
      maker,
      commission,
      commissionAsset: commissionAsset || undefined,
      realizedPnl,
      positionSide,
    };
  };

  private parseTransactionRowsFromResponse = (data: any): Record<string, any>[] => {
    if (Array.isArray(data?.data?.rows)) return data.data.rows;
    if (Array.isArray(data?.rows)) return data.rows;
    if (Array.isArray(data?.data)) return data.data;
    if (Array.isArray(data)) return data;
    return [];
  };

  /**
   * `/v1/client/trades` — the legacy fills fallback, with its parameter vocabulary MEASURED.
   *
   * Every name here differs from the v3 route beside it, and each one fails SILENTLY rather
   * than erroring, which is why this was wrong in the shipping code for as long as it existed:
   *
   * - **`size`, not `limit`.** `limit` is accepted and ignored — the page comes back at its
   *   default 100 whatever you ask for.
   * - **`start_t` / `end_t` in MILLISECONDS, not `startTime` / `endTime`.** This is the one
   *   that corrupted data. Measured against row 28: a January-2020 window sent as
   *   `startTime`/`endTime` returned 100 rows of August-2026 fills, because the venue ignores
   *   those names entirely and serves the newest page. The caller then treats them as the
   *   answer for the window it asked for. The same window under `start_t`/`end_t` correctly
   *   returns 0 rows.
   * - **`page` works and rows are newest-first**, the opposite of `/v3/trade/transactionHistory`,
   *   which ignores `page` and returns oldest-first. Do not share a pager between them.
   *
   * `/v1/client/hist_trades` is NOT in this chain: it answers `500 internal error` on this
   * account for every parameter shape tried, including no parameters at all, so calling it
   * only buys a guaranteed-failing request per attempt.
   *
   * Throws rather than returning `[]`. A swallowed transport error here is indistinguishable
   * from an idle window at the call site, which is exactly how a 500 becomes "no trades".
   */
  private fetchLegacyTransactionHistoryRows = async (opts: {
    symbol?: string;
    fromMilliseconds?: number;
    toMilliseconds?: number;
    limit: number;
  }): Promise<Record<string, any>[]> => {
    const symbolCandidates = opts.symbol ? this.getTransactionHistorySymbolCandidates(opts.symbol) : [];

    let lastError: any;
    for (const symbolCandidate of [...symbolCandidates, undefined]) {
      try {
        const params = buildWooLegacyFillsParams({ ...opts, symbol: symbolCandidate });
        const { data } = await this.xhr.get<any>(ENDPOINTS.CLIENT_TRADES, { params });
        const rows = this.parseTransactionRowsFromResponse(data);
        if (rows.length > 0) return rows;
      } catch (err) {
        lastError = err;
      }
    }

    if (lastError) throw lastError;

    return [];
  };

  private fetchTransactionHistoryRows = async (opts: {
    symbol?: string;
    fromMilliseconds?: number;
    toMilliseconds?: number;
    limit: number;
  }): Promise<Record<string, any>[]> => {
    const buildParams = (symbolCandidate?: string) => {
      const params: Record<string, any> = { limit: opts.limit };
      if (symbolCandidate) {
        params.symbol = symbolCandidate;
      }
      if (opts.fromMilliseconds !== undefined) {
        params.startTime = opts.fromMilliseconds;
      }
      if (opts.toMilliseconds !== undefined) {
        params.endTime = opts.toMilliseconds;
      }
      return params;
    };

    const requestRows = async (params: Record<string, any>) => {
      const { data } = await this.xhr.get<any>(ENDPOINTS.TRANSACTION_HISTORY, { params });
      return this.parseTransactionRowsFromResponse(data);
    };

    if (!opts.symbol) {
      return requestRows(buildParams());
    }

    let lastError: any;
    for (const symbolCandidate of this.getTransactionHistorySymbolCandidates(opts.symbol)) {
      try {
        const rows = await requestRows(buildParams(symbolCandidate));
        if (rows.length > 0) {
          return rows;
        }
      } catch (err) {
        lastError = err;
      }
    }

    try {
      const rows = await requestRows(buildParams());
      if (rows.length > 0) {
        return rows;
      }
    } catch (err) {
      lastError = err;
    }

    const legacyRows = await this.fetchLegacyTransactionHistoryRows(opts);
    if (legacyRows.length > 0) {
      return legacyRows;
    }

    if (lastError) {
      throw lastError;
    }

    return [];
  };

  private getPositionSignatureForSymbol = (symbol: string) => {
    return this.store.positions
      .filter((position) => position.symbol === symbol)
      .sort((a, b) => a.id.localeCompare(b.id))
      .map((position) => `${position.id}:${position.side}:${position.contracts}:${position.entryPrice}`)
      .join('|');
  };

  private scheduleFillPositionRefresh = (symbol: string, attempt: number) => {
    if (this.isDisposed) return;

    const existingTimer = this._fillPositionRefreshTimers.get(symbol);
    if (existingTimer) {
      clearTimeout(existingTimer);
    }

    const delayMs =
      attempt === 0 ? 0 : WOOXExchange.FILL_POSITION_REFRESH_BASE_DELAY_MS * Math.pow(2, Math.max(attempt - 1, 0));

    const timer = setTimeout(() => {
      this._fillPositionRefreshTimers.delete(symbol);
      this.refreshPositionForSymbolFromFill(symbol, attempt).catch((err) => {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
      });
    }, delayMs);

    this._fillPositionRefreshTimers.set(symbol, timer);
  };

  private refreshPositionForSymbolFromFill = async (symbol: string, attempt: number) => {
    if (this.isDisposed) return;
    if (this._fillPositionRefreshInFlight.has(symbol)) return;

    this._fillPositionRefreshInFlight.add(symbol);
    let shouldRetry = false;

    try {
      const beforeSignature = this.getPositionSignatureForSymbol(symbol);
      const expectedSignature = this._fillExpectedPositionSignatureBySymbol.get(symbol);
      const positions = await this.fetchPositions([symbol]);
      if (this.isDisposed) return;

      this.store.addOrUpdatePositions(positions);
      const afterSignature = this.getPositionSignatureForSymbol(symbol);
      shouldRetry = expectedSignature ? afterSignature !== expectedSignature : beforeSignature === afterSignature;

      if (!shouldRetry) {
        this._fillExpectedPositionSignatureBySymbol.delete(symbol);
      }
    } catch (err: any) {
      shouldRetry = true;
      this.emitAndLogError(err?.response?.data?.message || err?.message);
    } finally {
      this._fillPositionRefreshInFlight.delete(symbol);
    }

    if (shouldRetry && attempt + 1 < WOOXExchange.MAX_FILL_POSITION_REFRESH_ATTEMPTS) {
      this.scheduleFillPositionRefresh(symbol, attempt + 1);
      return;
    }

    this._fillExpectedPositionSignatureBySymbol.delete(symbol);
  };

  private applyOptimisticOneWayPositionFromFill = (orderFill: {
    symbol?: string;
    side?: OrderSide;
    price?: number;
    amount?: number;
  }) => {
    if (this._accountPositionMode !== PositionMode.OneWay) {
      return false;
    }

    const symbol = orderFill?.symbol;
    const side = orderFill?.side;
    const fillPrice = Number(orderFill?.price);
    const fillQty = Number(orderFill?.amount);

    if (!symbol) return false;
    if (side !== OrderSide.Buy && side !== OrderSide.Sell) return false;
    if (!Number.isFinite(fillPrice) || fillPrice <= 0) return false;
    if (!Number.isFinite(fillQty) || fillQty <= 0) return false;

    const currentOpenPosition = this.store.positions.find(
      (position) => position.symbol === symbol && position.contracts > 0,
    );
    const signedCurrentContracts = !currentOpenPosition
      ? 0
      : currentOpenPosition.side === PositionSide.Long
        ? currentOpenPosition.contracts
        : -currentOpenPosition.contracts;
    const signedFillContracts = side === OrderSide.Buy ? fillQty : -fillQty;
    let nextSignedContracts = signedCurrentContracts + signedFillContracts;

    if (Math.abs(nextSignedContracts) < 1e-10) {
      nextSignedContracts = 0;
    }

    const nextContracts = Math.abs(nextSignedContracts);
    const nextSide = nextSignedContracts >= 0 ? PositionSide.Long : PositionSide.Short;

    if (currentOpenPosition && (nextContracts === 0 || currentOpenPosition.side !== nextSide)) {
      this.store.updatePosition(
        { id: currentOpenPosition.id },
        {
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          contracts: 0,
          liquidationPrice: 0,
        },
      );
    }

    if (nextContracts === 0) {
      return true;
    }

    const ticker = this.store.tickersBySymbol[symbol];
    const mark = Number(ticker?.mark);
    const markOrFillPrice = Number.isFinite(mark) && mark > 0 ? mark : fillPrice;

    const previousContracts = Math.abs(signedCurrentContracts);
    const sameDirection =
      Math.sign(signedCurrentContracts) === Math.sign(nextSignedContracts) && Math.sign(signedCurrentContracts) !== 0;

    let nextEntryPrice = fillPrice;
    if (currentOpenPosition && sameDirection && nextContracts <= previousContracts) {
      nextEntryPrice = currentOpenPosition.entryPrice;
    } else if (currentOpenPosition && sameDirection && previousContracts > 0) {
      nextEntryPrice =
        (currentOpenPosition.entryPrice * previousContracts + fillPrice * Math.abs(signedFillContracts)) /
        nextContracts;
    }

    const positionMode = currentOpenPosition?.positionMode || PositionMode.OneWay;
    const leverage = currentOpenPosition?.leverage || 1;
    const marginType = currentOpenPosition?.marginType || MarginType.Cross;

    const optimisticPosition: Position = {
      id: createPositionId(symbol, nextSide, positionMode),
      symbol,
      side: nextSide,
      entryPrice: nextEntryPrice,
      notional: nextContracts * markOrFillPrice,
      leverage,
      unrealizedPnl: nextContracts === 0 ? 0 : currentOpenPosition?.unrealizedPnl || 0,
      contracts: nextContracts,
      liquidationPrice: nextContracts === 0 ? 0 : currentOpenPosition?.liquidationPrice || 0,
      marginSize: undefined,
      positionMode,
      marginType,
    };

    this.store.addOrUpdatePosition(optimisticPosition);
    return true;
  };

  emitOrderExecution = (fill: OrderFillEvent, positionAmount: number) => {
    // An unseen execution still belongs in the fill stream when REST already
    // includes it. Only the unaccounted cumulative delta may move positions.
    if (positionAmount > 0) this.handleFillForPositionSync({ ...fill, amount: positionAmount });
    else this.scheduleFillPositionRefresh(fill.symbol, 0);
    this.emitter.emit('fill', fill);
  };

  private handleFillForPositionSync = (orderFill: {
    symbol?: string;
    side?: OrderSide;
    price?: number;
    amount?: number;
  }) => {
    const symbol = orderFill?.symbol;
    if (!symbol) return;

    const didApplyOptimisticPosition = this.applyOptimisticOneWayPositionFromFill(orderFill);
    const hasOpenPositionAfterOptimisticUpdate = this.store.positions.some(
      (position) => position.symbol === symbol && position.contracts > 0,
    );
    if (didApplyOptimisticPosition && hasOpenPositionAfterOptimisticUpdate) {
      this._fillExpectedPositionSignatureBySymbol.set(symbol, this.getPositionSignatureForSymbol(symbol));
    } else {
      this._fillExpectedPositionSignatureBySymbol.delete(symbol);
    }
    this.scheduleFillPositionRefresh(symbol, 0);
  };

  getAccount = async () => {
    const data = await this.fetchAccountInfo();

    return {
      userId: data.applicationId,
      affiliateId: data.referrerID || null,
    };
  };

  validateAccount = async () => {
    try {
      await this._primaryXhr.get(ENDPOINTS.ACCOUNT);
      if (this.options.secondary_key) {
        await this._secondaryXhr.get(ENDPOINTS.ACCOUNT);
      }
      // Key validated; balance is best-effort for the new-account gate.
      return { error: '', balance: await this.fetchBalance().catch(() => undefined) };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return {
        error: err?.response?.data?.message || err?.response?.data?.msg || err?.message || 'Invalid API key or secret',
      };
    }
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly = this.isPublicDataOnly;

    if (!useExternalPublicData) {
      const [, rawTickers] = await Promise.all([
        this.fetchMarkets().then((markets) => {
          if (this.isDisposed) return [];
          this.store.update({
            markets,
            loaded: { ...this.store.loaded, markets: true },
          });

          // start public websocket streams as soon as markets loaded.
          this.publicWebsocket.connectAndSubscribe();

          return markets;
        }),
        this.fetchTickersRaw(),
      ]);

      if (this.isDisposed) {
        return;
      }

      const tickers = this.parseTickers(rawTickers);
      this.store.update({ tickers, loaded: { ...this.store.loaded, tickers: true } });
      this.startFundingRatesRefresh();
    }

    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, orders: true, positions: true, balance: true },
      });
      return;
    }

    this.privateWebsocket.connectAndSubscribe();

    await Promise.all([
      this.fetchBalance().then((balance) => {
        if (this.isDisposed) return;
        this.store.update({ balance, loaded: { ...this.store.loaded, balance: true } });
        return balance;
      }),
      this.fetchPositions().then((positions) => {
        if (this.isDisposed) return;
        this.store.addOrUpdatePositionsFull(positions);
        this.store.update({ loaded: { ...this.store.loaded, positions: true } });
        return positions;
      }),
      this.fetchOrders().then((orders) => {
        if (this.isDisposed) return;
        this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
        return orders;
      }),
    ]);

    if (this.isDisposed) return;
    await this.tick();

    this.log(`Ready to trade on WOO X`);
  };

  tick = async (count = 0) => {
    if (!this.isDisposed) {
      try {
        // after fetch account in case positionMode or leverage updated
        if (count % 5 === 0) {
          const { positions, balance } = await this.fetchPositionsAndBalance();
          if (this.isDisposed) return;
          this.store.addOrUpdatePositionsFull(positions);
          this.store.update({ balance, loaded: { ...this.store.loaded, balance: true, positions: true } });
        } else {
          const positions = await this.fetchPositions();
          if (this.isDisposed) return;
          this.store.addOrUpdatePositionsFull(positions);
        }

        if ((count + 5) % 10 === 0) {
          const orders = await this.fetchOrders();
          if (this.isDisposed) return;
          this.store.update({ orders, loaded: { ...this.store.loaded, orders: true } });
        }
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      if (!this.isDisposed) {
        loop(() => this.tick(count + 1), this.getTickInterval());
      }
    }
  };

  fetchBalance = async () => {
    try {
      const data = await this.fetchAccountInfo();

      if (data.accountMode !== 'FUTURES') {
        throw new Error(`WOO X account is in ${data.accountMode} mode, only FUTURES mode is supported`);
      }

      // same with positionMode
      this._accountPositionMode = data.positionMode === 'ONE_WAY' ? PositionMode.OneWay : PositionMode.Hedged;
      this.store.setSetting('isHedged', this._accountPositionMode === PositionMode.Hedged);

      const free = Math.round(data.freeCollateral * 100) / 100;
      const total = Math.round(data.totalAccountValue * 100) / 100;
      const used = Math.round((data.totalCollateral - data.freeCollateral) * 100) / 100;

      // woo doesnt provides unrealized pnl in the account endpoint
      // we are computing this from the positions polling
      return { free, total, used, upnl: this.store.balance.upnl };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.balance;
    }
  };

  fetchMarkets = async () => {
    try {
      const {
        data: { rows },
      } = await this.xhr.get<{ rows: Array<Record<string, any>> }>(ENDPOINTS.MARKETS);

      const markets: Market[] = rows
        .filter((r) => r.symbol.startsWith('PERP_'))
        .map((r) => {
          const [, baseAsset, quoteAsset] = r.symbol.split('_');
          const imr = v(r, 'base_imr') || 0.01;
          const maxLeverage = Math.floor(1 / imr);

          return {
            id: `${baseAsset}/${quoteAsset}:USDT`,
            symbol: `${baseAsset}${quoteAsset}`,
            base: baseAsset,
            quote: quoteAsset,
            active: true,
            precision: {
              amount: v(r, 'base_tick'),
              price: v(r, 'quote_tick'),
            },
            limits: {
              amount: {
                min: v(r, 'base_min'),
                max: v(r, 'base_max'),
              },
              leverage: {
                min: 1,
                max: maxLeverage,
              },
            },
            info: r,
          };
        });

      return markets;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.markets;
    }
  };

  fetchTickers = async () => {
    const rows = await this.fetchTickersRaw();
    return this.parseTickers(rows);
  };

  fetchTickersRaw = async () => {
    try {
      const {
        data: { rows },
      } = await this.xhr.get<{ rows: Array<Record<string, any>> }>(ENDPOINTS.TICKERS);

      return rows;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return null;
    }
  };

  parseTickers = (rows: any) => {
    try {
      if (!rows) return this.store.tickers;
      if (!Array.isArray(rows)) throw new TypeError('rows.reduce is not a function');

      const tickers: Ticker[] = [];
      for (let i = 0; i < rows.length; i++) {
        const row = rows[i];
        const symbol = normalizeSymbol(row.symbol);
        const market = this.store.marketsBySymbol[symbol];

        if (!market) continue;

        const open = v(row, '24h_open');
        const close = v(row, '24h_close');

        const percentage = Math.round(((close - open) / open) * 100 * 100) / 100;

        const ticker: Ticker = {
          id: market.id,
          symbol: market.symbol,
          bid: close,
          ask: close,
          last: close,
          index: v(row, 'index_price'),
          mark: v(row, 'mark_price'),
          percentage,
          fundingRate: v(row, 'last_funding_rate'),
          fundingTime: v(row, 'next_funding_time'),
          volume: v(row, '24h_volume'),
          quoteVolume: v(row, '24h_amount'),
          openInterest: v(row, 'open_interest'),
        };

        tickers.push(ticker);
      }
      return tickers;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.tickers;
    }
  };

  fetchPositionsAndBalance = async () => {
    // call before positions to update leverage
    const balance = await this.fetchBalance();
    const positions = await this.fetchPositions();

    // woo doesnt provides unrealized pnl in the account endpoint
    // we are computing this from the positions polling
    (balance as Writable<Balance>).upnl =
      positions.length > 0 ? Math.round(sumBy(positions, 'unrealizedPnl') * 100) / 100 : 0;

    // total balance from API already takes in account uPNL
    (balance as Writable<Balance>).total = balance.total - balance.upnl;
    return { positions, balance };
  };

  private createPositionMetadataCacheKey = (symbol: string, positionMode: PositionMode) => `${symbol}:${positionMode}`;

  private buildPositionMetadataFromLeverage = (
    symbol: string,
    positionMode: PositionMode,
    response: GET_FUTURES_LEVERAGE_RESPONSE,
  ): WooPositionMetadata[] => {
    const detail = response.data.details[0];
    const leverage = parseInt(detail.leverage);

    if (positionMode === PositionMode.OneWay) {
      return [
        {
          symbol,
          side: PositionSide.Long,
          leverage,
          positionMode,
          marginType: MarginType.Cross,
        },
      ];
    }

    return [
      {
        symbol,
        side: PositionSide.Long,
        leverage,
        positionMode,
        marginType: MarginType.Cross,
      },
      {
        symbol,
        side: PositionSide.Short,
        leverage,
        positionMode,
        marginType: MarginType.Cross,
      },
    ];
  };

  private fetchPositionMetadataForSymbol = async (symbol: string, positionMode: PositionMode) => {
    const cacheKey = this.createPositionMetadataCacheKey(symbol, positionMode);
    const cached = this._positionMetadataCache.get(cacheKey);
    const now = Date.now();

    if (cached && now - cached.updatedAt < POSITION_METADATA_CACHE_TTL_MS) {
      return cached.metadata;
    }

    const inFlight = this._positionMetadataInFlight.get(cacheKey);
    if (inFlight) {
      return inFlight;
    }

    if (cached && now < (this._positionMetadataRateLimitedUntil.get(cacheKey) || 0)) {
      return cached.metadata;
    }

    const request = (async () => {
      try {
        const { data } = await this.xhr.get<GET_FUTURES_LEVERAGE_RESPONSE>(ENDPOINTS.FUTURES_LEVERAGE, {
          params: {
            symbol: reverseSymbol(symbol),
            // woo is fucked and always uses oneway leverage
            // position_mode: positionMode === PositionMode.OneWay ? 'ONE_WAY' : 'HEDGE_MODE',
            position_mode: 'ONE_WAY',
            margin_mode: 'CROSS',
          },
        });

        const metadata = this.buildPositionMetadataFromLeverage(symbol, positionMode, data);
        this._positionMetadataCache.set(cacheKey, { metadata, updatedAt: Date.now() });
        this._positionMetadataRateLimitedUntil.delete(cacheKey);
        return metadata;
      } catch (err: any) {
        if (cached && isWooRateLimitError(err)) {
          this._positionMetadataRateLimitedUntil.set(cacheKey, Date.now() + POSITION_METADATA_RATE_LIMIT_BACKOFF_MS);
          return cached.metadata;
        }

        throw err;
      } finally {
        this._positionMetadataInFlight.delete(cacheKey);
      }
    })();

    this._positionMetadataInFlight.set(cacheKey, request);
    return request;
  };

  private updateCachedPositionMetadataLeverage = (symbol: string, leverage: number) => {
    for (const [cacheKey, cached] of this._positionMetadataCache.entries()) {
      if (!cacheKey.startsWith(`${symbol}:`)) continue;

      this._positionMetadataCache.set(cacheKey, {
        metadata: cached.metadata.map((metadata) => ({ ...metadata, leverage })),
        updatedAt: Date.now(),
      });
      this._positionMetadataRateLimitedUntil.delete(cacheKey);
    }
  };

  private fetchMarketIocTouchPrice = async (symbol: string, side: OrderSide) => {
    if (typeof this.xhr.get !== 'function') {
      return undefined;
    }

    try {
      const { data } = await this.xhr.get<{
        asks?: Array<{ price?: number | string }>;
        bids?: Array<{ price?: number | string }>;
      }>(`${ENDPOINTS.ORDERBOOK}/${reverseSymbol(symbol)}`);
      const [bestLevel] = side === OrderSide.Buy ? data?.asks || [] : data?.bids || [];
      const price = Number(bestLevel?.price);
      return isPositiveFinitePrice(price) ? price : undefined;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return undefined;
    }
  };

  private confirmPositionModeAndRefreshPositions = async (expectedPositionMode: PositionMode) => {
    let lastObservedPositionMode: PositionMode | undefined;

    for (let attempt = 0; attempt < POSITION_MODE_READBACK_ATTEMPTS; attempt++) {
      this._accountPositionMode = null;
      lastObservedPositionMode = await this.getPositionMode();

      if (lastObservedPositionMode === expectedPositionMode) {
        this._positionMetadataCache.clear();
        this._positionMetadataRateLimitedUntil.clear();
        const positions = await this.fetchPositions();
        this.store.update({ positions });
        return;
      }

      if (attempt + 1 < POSITION_MODE_READBACK_ATTEMPTS) {
        await sleep(POSITION_MODE_READBACK_DELAY_MS);
      }
    }

    throw new Error(`WOO position mode readback stayed ${lastObservedPositionMode}; expected ${expectedPositionMode}`);
  };

  fetchPositionMetadata = async (symbols: string[]): Promise<WooPositionMetadata[]> => {
    try {
      const positionMode = await this.getPositionMode();
      return flatten(
        await mapSeries(symbols, async (symbol): Promise<WooPositionMetadata[]> => {
          if (this.isDisposed) return [];
          return this.fetchPositionMetadataForSymbol(symbol, positionMode);
        }),
      );
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.msg || err?.message);
      return [];
    }
  };

  fetchPositions = async (symbols?: string[]) => {
    const openPositions = await this.fetchOpenPositions();
    const positionMode = await this.getPositionMode();
    const targetSymbols = symbols || [
      ...new Set([
        ...openPositions.map((p) => p.symbol),
        ...this.store.orders.filter((o) => o.status === OrderStatus.Open).map((o) => o.symbol),
      ]),
    ];

    if (!targetSymbols.length) return openPositions;

    const openPositionsForSymbols = openPositions.filter((p) => targetSymbols.includes(p.symbol));
    const symbolsNeedingMetadata = targetSymbols.filter((symbol) => {
      const symbolPositions = openPositionsForSymbols.filter((p) => p.symbol === symbol);
      if (!symbolPositions.length) return true;
      if (positionMode !== PositionMode.Hedged) return false;
      const hasLong = symbolPositions.some((p) => p.side === PositionSide.Long);
      const hasShort = symbolPositions.some((p) => p.side === PositionSide.Short);
      return !hasLong || !hasShort;
    });
    const positionMetadatas = await this.fetchPositionMetadata(symbolsNeedingMetadata);
    const openPositionIds = new Set(
      openPositionsForSymbols.map((p) => createPositionId(p.symbol, p.side, p.positionMode || positionMode)),
    );
    const fakePositions = positionMetadatas
      .filter(
        (metadata) => !openPositionIds.has(createPositionId(metadata.symbol, metadata.side, metadata.positionMode)),
      )
      .map((metadata) => {
        return {
          id: createPositionId(metadata.symbol, metadata.side, metadata.positionMode),
          ...metadata,
          entryPrice: 0,
          notional: 0,
          unrealizedPnl: 0,
          contracts: 0,
          liquidationPrice: 0,
        };
      });
    if (symbols) {
      return [...openPositionsForSymbols, ...fakePositions];
    }

    return [...openPositions, ...fakePositions];
  };

  // will not update leverage if called by itself (need to call fetchBalance first)
  fetchOpenPositions = async () => {
    try {
      const { data } = await this.xhr.get<{
        data: { positions: Array<Record<string, any>> };
      }>(ENDPOINTS.POSITIONS);

      if (!(data as any)?.success) {
        throw new Error((data as any).message || `Failed to fetch positions`);
      }

      const positionMode = await this.getPositionMode();
      const positions = data?.data?.positions || [];
      if (!Array.isArray(positions)) throw new TypeError('positions.reduce is not a function');

      const mappedPositions: Position[] = [];
      for (let i = 0; i < positions.length; i++) {
        const p = positions[i];
        const symbol = normalizeSymbol(p.symbol);
        const ticker = this.store.tickersBySymbol[symbol];

        if (!ticker) continue;
        if (!p.holding) continue;

        const entryPrice = v(p, 'averageOpenPrice');
        const holdings = v(p, 'holding');
        const contracts = Math.abs(holdings);
        const side = holdings > 0 ? PositionSide.Long : PositionSide.Short;

        const priceDiff = Math.max(entryPrice, ticker.mark) - Math.min(entryPrice, ticker.mark);

        const isLoss =
          (side === PositionSide.Long && ticker.mark < entryPrice) ||
          (side === PositionSide.Short && ticker.mark > entryPrice);

        const absUPNL = priceDiff * contracts;
        const upnl = Math.round((isLoss ? -absUPNL : absUPNL) * 100) / 100;

        const position: Position = {
          id: createPositionId(symbol, side, positionMode),
          symbol,
          side,
          entryPrice,
          notional: contracts * ticker.mark,
          leverage: p.leverage,
          unrealizedPnl: upnl,
          contracts,
          liquidationPrice: v(p, 'estLiqPrice'),
          // Clear the synthetic flat position's zero when this holding reopens.
          marginSize: undefined,
          positionMode: positionMode,
          marginType: p.marginMode === 'CROSS' ? MarginType.Cross : MarginType.Isolated,
          info: p,
        };

        if (position.marginType === MarginType.Isolated) {
          this.emitAndLogError('Detected isolated margin position. Only cross margin is supported');
        }

        mappedPositions.push(position);
      }
      return mappedPositions;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return this.store.positions;
    }
  };

  fetchOrders = async () => {
    const revision = this._orderRevision;
    const limitOrders = await this.fetchLimitOrders();
    const algoOrders = await this.fetchAlgoOrders();
    const orders = new Map([...limitOrders, ...algoOrders].map((o) => [o.id, this.reconcileOrder(o)]));
    for (const row of this.store.orders) {
      if ((this._orderVersions.get(row.id) || 0) > revision) orders.set(row.id, row);
    }
    return [...orders.values()];
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions = {}): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 500 } = opts;
    const normalizedRequestedSymbol = this.normalizeRequestedSymbol(symbol);
    const normalizedOrderId = orderId ? String(orderId) : undefined;
    const fromSeconds = this.normalizeTimestampSeconds(from);
    const toSeconds = this.normalizeTimestampSeconds(to);
    const maxLimit = Math.min(Math.max(limit, 1), 500);
    const fromMilliseconds = this.normalizeTimestampMilliseconds(from);
    const toMilliseconds = this.normalizeTimestampMilliseconds(to);

    try {
      const rows = await this.fetchTransactionHistoryRows({
        symbol: normalizedRequestedSymbol,
        fromMilliseconds,
        toMilliseconds,
        limit: maxLimit,
      });

      const executions = rows
        .map((row: Record<string, any>) => this.parseTransactionHistoryRow(row))
        .filter((entry) => {
          if (!entry) {
            return false;
          }
          if (normalizedRequestedSymbol && entry.symbol !== normalizedRequestedSymbol) {
            return false;
          }
          if (normalizedOrderId && entry.orderId !== normalizedOrderId) {
            return false;
          }
          if (fromSeconds !== undefined && entry.timestampSeconds < fromSeconds) {
            return false;
          }
          if (toSeconds !== undefined && entry.timestampSeconds > toSeconds) {
            return false;
          }
          return true;
        })
        .map((entry): Execution => {
          return omitUndefined({
            id: entry!.fillId,
            timestamp: entry!.timestampSeconds,
            symbol: entry!.symbol,
            orderId: entry!.orderId,
            price: entry!.price,
            qty: entry!.qty,
            side: entry!.side,
            commission: entry!.commission,
            commissionAsset: entry!.commissionAsset,
            positionSide: entry!.positionSide,
            realizedPnl: entry!.realizedPnl,
            maker: entry!.maker,
          });
        })
        .sort((a, b) => b.timestamp - a.timestamp)
        .slice(0, maxLimit);

      if (normalizedRequestedSymbol && executions.length === 0) {
        const historicalTrades = await this.fetchHistoricalTradesInternal({
          symbol: normalizedRequestedSymbol,
          orderId: normalizedOrderId,
          from,
          to,
          limit: maxLimit,
        });

        if (historicalTrades.length > 0) {
          return historicalTrades
            .map((trade): Execution => {
              return omitUndefined({
                id: trade.id,
                timestamp: Math.floor(trade.time / 1000),
                symbol: trade.symbol,
                orderId: trade.orderId,
                price: trade.price,
                qty: trade.qty,
                side: trade.side,
                commission: trade.commission,
                commissionAsset: trade.commissionAsset,
                positionSide: trade.positionSide,
                realizedPnl: trade.realizedPnl,
                maker: trade.maker,
              });
            })
            .sort((a, b) => b.timestamp - a.timestamp)
            .slice(0, maxLimit);
        }
      }

      return executions;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  /**
   * Internal only — the public fill surface is `fetchHistoricalFills`.
   *
   * Kept because this venue's own `fetchHistoricalFills` consumes it: the two methods
   * read one axis, and here the millisecond-shaped mapper is the one that does
   * the parsing. Private so nothing outside the class can treat it as a second
   * public history axis again.
   */
  private fetchHistoricalTradesInternal = async (opts: HistoricalTradeOptions): Promise<HistoricalTrade[]> => {
    if (this.isPublicDataOnly || !opts.symbol) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 500 } = opts;
    const normalizedRequestedSymbol = this.normalizeRequestedSymbol(symbol);
    if (!normalizedRequestedSymbol) {
      return [];
    }
    const normalizedOrderId = orderId ? String(orderId) : undefined;
    const maxLimit = Math.min(Math.max(limit, 1), 500);
    const fromMilliseconds = this.normalizeTimestampMilliseconds(from);
    const toMilliseconds = this.normalizeTimestampMilliseconds(to);

    try {
      const rows = await this.fetchTransactionHistoryRows({
        symbol: normalizedRequestedSymbol,
        fromMilliseconds,
        toMilliseconds,
        limit: maxLimit,
      });

      return rows
        .map((row: Record<string, any>) => this.parseTransactionHistoryRow(row))
        .filter((entry) => {
          if (!entry) {
            return false;
          }
          if (entry.symbol !== normalizedRequestedSymbol) {
            return false;
          }
          if (normalizedOrderId && entry.orderId !== normalizedOrderId) {
            return false;
          }
          if (fromMilliseconds !== undefined && entry.timestampMilliseconds < fromMilliseconds) {
            return false;
          }
          if (toMilliseconds !== undefined && entry.timestampMilliseconds > toMilliseconds) {
            return false;
          }
          return true;
        })
        .map((entry): HistoricalTrade => {
          return omitUndefined({
            id: entry!.fillId,
            symbol: entry!.symbol,
            orderId: entry!.orderId || `${entry!.symbol}-${entry!.timestampMilliseconds}`,
            price: entry!.price,
            qty: entry!.qty,
            realizedPnl: entry!.realizedPnl,
            positionSide: entry!.positionSide,
            side: entry!.side,
            maker: entry!.maker,
            commission: entry!.commission,
            commissionAsset: entry!.commissionAsset,
            time: entry!.timestampMilliseconds,
          });
        })
        .sort((a, b) => b.time - a.time)
        .slice(0, maxLimit);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const currentCandleStart = getCandleStartTime(opts.interval, Date.now() / 1000);
    const intervalInSeconds = timeframeToSeconds[opts.interval] || Infinity;
    const toInSeconds = (opts.to || 0) / 1000;
    const distanceFromCurrent = Math.abs(currentCandleStart - toInSeconds) / intervalInSeconds;
    let rows;

    // if we can get newest candle within 1000 limit + 500 fetchable candles, we should use KLINE bc that has newest in progress candle
    if (distanceFromCurrent < 100) {
      const { data } = await this.xhr.get<GET_KLINE_RESPONSE['data']>(ENDPOINTS.KLINE, {
        params: {
          symbol: reverseSymbol(opts.symbol),
          type: opts.interval,
          limit: Math.min(opts.limit || 500, 1000),
        },
      });
      rows = data.rows;
    } else {
      const { data } = await this._proxyEnforcedXhr.get<GET_KLINE_RESPONSE>(ENDPOINTS.KLINE_HISTORY, {
        params: {
          symbol: reverseSymbol(opts.symbol),
          type: opts.interval,
          start_time: opts.from,
          end_time: opts.to,
          size: Math.min(opts.limit || 500, 1000),
        },
      });
      rows = data.data.rows;
    }

    const candles: Candle[] = rows.map((r) => {
      return {
        timestamp: r.start_timestamp / 1000,
        open: r.open,
        high: r.high,
        low: r.low,
        close: r.close,
        volume: r.volume,
      };
    });

    return candles.reverse();
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) return this.rejectPreflight(`Market ${order.symbol} not found`);
    if (
      'price' in update &&
      (!(update.price > 0) || !Number.isFinite(update.price) || !(adjust(update.price, market.precision.price) > 0))
    ) {
      return this.rejectPreflight('Invalid WOO amendment price');
    }
    if ('amount' in update) {
      const quantity = adjust(update.amount, market.precision.amount);
      if (
        !Number.isFinite(quantity) ||
        quantity < market.limits.amount.min ||
        quantity > market.limits.amount.max ||
        quantity < order.filled
      ) {
        return this.rejectPreflight('Invalid WOO amendment quantity');
      }
    }
    if (order.type === OrderType.TrailingStopLoss) {
      await this.cancelOrders([order]);

      return this.placeTrailingStopLossOrder({
        symbol: order.symbol,
        side: order.side,
        type: OrderType.TrailingStopLoss,
        price: 'price' in update ? update.price : order.price,
        amount: 'amount' in update ? update.amount : order.amount,
        slTriggerType: order.slTriggerType,
      });
    }

    if (this.isAlgoOrder(order.type)) {
      return this.updateAlgoOrder({ order, update });
    }

    // WOO requires quantity (or amount) on every edit — a price-only body is
    // accepted as EDIT_SENT and then fails asynchronously, cancelling the order.
    const payload: Record<string, any> = { orderId: order.id };
    if ('price' in update) payload.price = `${adjust(update.price, market.precision.price)}`;
    payload.quantity = `${adjust('amount' in update ? update.amount : order.amount, market.precision.amount)}`;

    try {
      await this.xhr.put(`${ENDPOINTS.UPDATE_ORDER}/${order.id}`, payload);
      return [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  updateAlgoOrder = async ({ order, update }: UpdateOrderOpts) => {
    const market = this.store.marketsBySymbol[order.symbol];
    if (!market) return this.rejectPreflight(`Market ${order.symbol} not found`);
    const leaf: Record<string, any> = {};
    if ('price' in update) {
      const price = adjust(update.price, market.precision.price);
      if (!(price > 0) || !Number.isFinite(price)) return this.rejectPreflight('Invalid WOO amendment price');
      leaf.triggerPrice = String(price);
    }
    if ('amount' in update) {
      const amount = adjust(update.amount, market.precision.amount);
      if (
        !Number.isFinite(amount) ||
        amount < market.limits.amount.min ||
        amount > market.limits.amount.max ||
        amount < order.filled
      ) {
        return this.rejectPreflight('Invalid WOO amendment quantity');
      }
      leaf.quantity = String(amount);
    }
    const positional = order.info?.algoType === 'POSITIONAL_TP_SL' || !order.amount;
    const payload = positional ? { childOrders: [{ algoOrderId: order.id, ...leaf }] } : leaf;
    try {
      await this.xhr.put(`${ENDPOINTS.ALGO_ORDER}/${order.parentId || order.id}`, payload);
      return [order.id];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  /**
   * Historical orders, including ones that never filled — see `woo.order-history.ts`.
   *
   * Deliberately NOT routed through `fetchOrdersRecursively`, which hardcodes
   * `status: 'INCOMPLETE'` — that is the OPEN-order path, and reusing it would return
   * exactly the orders this axis exists to look past.
   *
   * `page` genuinely advances here (page 2 returned strictly older rows), unlike WOO's
   * trade route, which returns the oldest rows first.
   */
  fetchHistoricalOrders = async (opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const MAX_PAGES = 50;
    const PAGE_SIZE = 100;
    if (opts?.symbol && !this.store.marketsBySymbol[opts.symbol]) {
      return [];
    }
    // `reverseSymbol`, not `market.id` — every other symbol-scoped WOO call does the same,
    // because `Market.id` is `ETH/USDT:USDT` and the API wants `PERP_ETH_USDT`. Passing the
    // id returns zero rows or a 400 that this method's catch turns into an empty list.
    const requestSymbol = opts?.symbol ? reverseSymbol(opts.symbol) : undefined;

    // NOT `marketsById`. WOO's `Market.id` is `ETH/USDT:USDT` while this endpoint returns
    // `PERP_ETH_USDT`, so an id lookup never matches and every order would fall back to the
    // raw venue string — silently, since the mapper's fallback looks deliberate. blofin can
    // key on the venue id only because it sets `id: instId`; WOO does not.
    const resolveSymbol = (marketId: string) => this.store.marketsBySymbol[normalizeSymbol(marketId)]?.symbol;

    try {
      const orders: HistoricalOrder[] = [];
      const seen = new Set<string>();
      let page = 1;

      for (; page <= MAX_PAGES; page += 1) {
        const { data } = await this.xhr.get<any>(ENDPOINTS.ORDERS, {
          params: omitUndefined({
            symbol: requestSymbol,
            // MILLISECONDS on the request, even though the response reports fractional
            // SECONDS. Sending seconds is rejected with
            // `400 end_t must be greater than or equal to 1000000000000` — which this
            // method's own catch turns into an empty list, so the mismatch reads as an
            // account with no orders rather than as a rejected request.
            start_t: opts?.from,
            end_t: opts?.to,
            size: PAGE_SIZE,
            page,
            // WITHOUT THIS THE PnL FIELDS ARE SIMPLY ABSENT, and absent reads as "this
            // account has no realized PnL" rather than "we did not ask". That confusion is
            // the whole reason woo was recorded UNSUPPORTABLE: the v3 fills route it was
            // judged on returns `realizedPnl: null` on every row, and nobody asked this one.
            // Measured on row 28 — with the flag, each row carries `realized_pnl`,
            // `total_fee`, `total_rebate` and `position_side`.
            realized_pnl: true,
          }),
        });

        if (this.isDisposed) return [];

        if (data?.success === false) {
          this.emitAndLogError(data?.message || data?.meta?.message || 'Failed to fetch order history');
          break;
        }

        const rows: Array<Record<string, any>> = Array.isArray(data?.rows) ? data.rows : [];
        if (rows.length === 0) break;

        for (const row of rows) {
          const order = mapWooHistoricalOrder(row, resolveSymbol);
          if (!order || seen.has(order.id)) continue;
          seen.add(order.id);
          orders.push(order);
        }

        // `records_per_page` rather than the requested `size`, so a clamped or ignored size
        // cannot make the loop think it has read everything. With no `meta` at all, fall
        // back to the page-shortness test rather than stopping after page one.
        const total = Number(data?.meta?.total ?? 0);
        const perPage = Number(data?.meta?.records_per_page ?? PAGE_SIZE);
        if (!total) {
          if (rows.length < PAGE_SIZE) break;
        } else if (total <= perPage * page) {
          break;
        }
      }

      if (page > MAX_PAGES) {
        this.log(
          `fetchHistoricalOrders stopped at the ${MAX_PAGES}-page cap; older orders were not fetched`,
          LogSeverity.Warning,
        );
      }

      const sorted = orders.sort((a, b) => b.createdTime - a.createdTime);
      return opts?.limit && opts.limit > 0 ? sorted.slice(0, opts.limit) : sorted;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  fetchIncomeHistory = async (opts: IncomeHistoryOptions = {}): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) return [];
    const complete = opts.limit === undefined && opts.startTime !== undefined && opts.endTime !== undefined;
    const limit = opts.limit ?? (complete ? 100000 : 500);
    try {
      if (
        !Number.isSafeInteger(limit) ||
        limit < 1 ||
        limit > 100000 ||
        [opts.startTime, opts.endTime].some((time) => time !== undefined && (!Number.isFinite(time) || time < 0)) ||
        (opts.startTime !== undefined && opts.endTime !== undefined && opts.startTime > opts.endTime)
      ) {
        throw new Error('Invalid WOO income range or limit');
      }
      const rows: IncomeRecord[] = [];
      const seen = new Set<string>();
      for (let page = 1; page <= 1000; page += 1) {
        const now = virtualClock.getCurrentTimeMs();
        const at = Math.max(now, this._incomeNextAt);
        this._incomeNextAt = at + 6000;
        if (at > now) await sleep(at - now);
        if (this.isDisposed) throw new Error('WOO exchange disposed');
        const params = buildWooIncomeParams({ ...opts, limit }, page);
        const { data } = await this.xhr.get('/v1/client/transaction_history', { params });
        if (data?.success !== true || !Array.isArray(data.data?.rows)) {
          throw new Error(data?.message || 'Invalid WOO income response');
        }
        let fresh = 0;
        for (const raw of data.data.rows) {
          const row = mapWooIncomeRecord(raw);
          if (!row) throw new Error('Invalid WOO income row');
          const key = wooIncomeIdentity(row);
          if (seen.has(key)) continue;
          seen.add(key);
          fresh += 1;
          if (opts.symbol && row.symbol !== opts.symbol) continue;
          if (opts.incomeType && row.incomeType !== opts.incomeType) continue;
          if (opts.startTime !== undefined && row.timestamp < opts.startTime) continue;
          if (opts.endTime !== undefined && row.timestamp > opts.endTime) continue;
          rows.push(row);
        }
        const meta = data.data.meta;
        if (
          meta !== undefined &&
          (!Number.isSafeInteger(meta.total) ||
            meta.total < 0 ||
            !Number.isSafeInteger(meta.records_per_page) ||
            meta.records_per_page <= 0 ||
            meta.current_page !== page)
        ) {
          throw new Error('Invalid WOO income page metadata');
        }
        if (complete && rows.length > limit) throw new Error('WOO income range exceeded the complete-history budget');
        if (
          (!complete && rows.length >= limit) ||
          data.data.rows.length === 0 ||
          (meta !== undefined && meta.total <= page * meta.records_per_page)
        ) {
          return rows.sort((a, b) => b.timestamp - a.timestamp).slice(0, limit);
        }
        if (!fresh) throw new Error('WOO income pagination did not advance');
      }
      throw new Error('WOO income pagination limit exceeded');
    } catch (error: any) {
      this.emitAndLogError(error?.response?.data?.message || error?.message);
      throw error;
    }
  };

  fetchIncomeHistoryRange = (startTime: number, endTime: number, incomeType?: string) =>
    this.fetchIncomeHistory({ startTime, endTime, incomeType });

  fetchOrdersRecursively = async (endpoint: string, page: number = 1): Promise<Array<Record<string, any>>> => {
    let { data } = await this.xhr.get<FETCH_ORDER_RESPONSE>(endpoint, {
      params: { status: 'INCOMPLETE', page },
    });

    if (this.isDisposed) return [];

    if (!data.success) {
      throw new Error((data as any).message || (data as any).meta?.message || `Failed to fetch orders`);
    }

    // algo orders nest data...
    data = (data as any).data || data;
    if (
      !Array.isArray(data.rows) ||
      !data.meta ||
      data.meta.current_page !== page ||
      !Number.isSafeInteger(data.meta.total) ||
      data.meta.total < 0 ||
      !Number.isSafeInteger(data.meta.records_per_page) ||
      data.meta.records_per_page <= 0 ||
      page > 1000
    ) {
      throw new Error('Invalid WOO order page');
    }
    if (data.rows.length === 0) return [];

    if (data.meta.total > data.meta.records_per_page * page) {
      return [...data.rows, ...(await this.fetchOrdersRecursively(endpoint, page + 1))];
    }

    return data.rows;
  };

  fetchLimitOrders = async () => {
    try {
      const rows = await this.fetchOrdersRecursively(ENDPOINTS.ORDERS);
      if (!Array.isArray(rows)) throw new TypeError('rows.reduce is not a function');

      const orders: Order[] = [];
      for (let i = 0; i < rows.length; i++) {
        const order = this.mapLimitOrder(rows[i]);
        if (order !== null) orders.push(order);
        else if (this.store.marketsBySymbol[normalizeSymbol(rows[i].symbol || '')])
          throw new Error('Invalid WOO regular order');
      }
      return orders;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  mapLimitOrder = (o: Record<string, any>) => {
    const orderId = o.order_id ?? o.orderId ?? o.id;
    if (orderId === undefined || orderId === null) return null;
    const old = this.store.orders.find((row) => row.id === String(orderId));
    const symbol = o.symbol ? normalizeSymbol(o.symbol) : old?.symbol;
    if (!symbol || !this.store.marketsBySymbol[symbol]) return null;
    const quantity = Number(o.quantity ?? o.orderQuantity ?? o.order_quantity ?? old?.amount);
    const executed = Number(o.totalExecutedQuantity ?? o.executed ?? old?.filled ?? 0);
    const rawType = o.type ?? o.orderType ?? o.order_type;
    const type = ['IOC', 'FOK', 'POST_ONLY'].includes(rawType) ? OrderType.Limit : (ORDER_TYPE[rawType] ?? old?.type);
    const rawStatus = o.status;
    const status =
      rawStatus === 'FILLED'
        ? OrderStatus.Closed
        : ['CANCELLED', 'CANCELED', 'REJECTED'].includes(rawStatus)
          ? OrderStatus.Canceled
          : OrderStatus.Open;
    const side = ORDER_SIDE[o.side] ?? old?.side;
    const price = Number(o.price ?? o.orderPrice ?? o.order_price ?? old?.price ?? 0);
    if (
      !type ||
      !side ||
      !Number.isFinite(quantity) ||
      quantity < 0 ||
      !Number.isFinite(executed) ||
      executed < 0 ||
      !Number.isFinite(price)
    )
      return null;
    return this.reconcileOrder({
      id: String(orderId),
      status,
      symbol,
      type,
      side,
      price,
      amount: quantity,
      reduceOnly: o.reduce_only ?? o.reduceOnly ?? old?.reduceOnly ?? false,
      timeInForce:
        rawType === 'POST_ONLY' ? OrderTimeInForce.PostOnly : (old?.timeInForce ?? OrderTimeInForce.GoodTillCancel),
      filled: executed,
      remaining: Math.max(0, subtract(quantity, executed)),
      lastUpdate: virtualClock.getCurrentTimeMs(),
      info: o,
    });
  };

  fetchAlgoOrders = async () => {
    try {
      const rows = await this.fetchOrdersRecursively(ENDPOINTS.ALGO_ORDERS);
      if (!Array.isArray(rows)) throw new TypeError('rows.reduce is not a function');

      const orders: Order[] = [];
      for (let i = 0; i < rows.length; i++) {
        assertWooAlgoSnapshotRow(rows[i], (symbol) => !!this.store.marketsBySymbol[normalizeSymbol(symbol)]);
        const leaves = Array.isArray(rows[i].childOrders) ? rows[i].childOrders : [rows[i]];
        for (const leaf of leaves) {
          const childOrders = this.mapAlgoOrder(
            Array.isArray(rows[i].childOrders) ? { ...rows[i], childOrders: [leaf] } : leaf,
          );
          if (
            !childOrders.length &&
            this.store.marketsBySymbol[normalizeSymbol(leaf.symbol || rows[i].symbol || '')] &&
            ['STOP', 'STOP_LOSS', 'TAKE_PROFIT', 'TRAILING_STOP'].includes(leaf.algoType)
          )
            throw new Error('Invalid WOO algo order');
          orders.push(...childOrders);
        }
      }
      return orders;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  mapAlgoOrder = (o: Record<string, any>): Order[] => {
    const rows = Array.isArray(o.childOrders) ? o.childOrders : [o];
    const result: Order[] = [];
    for (const co of rows) {
      const id = String(co.algoOrderId ?? co.orderId ?? co.id ?? '');
      const old = this.store.orders.find((row) => row.id === id);
      const symbol = normalizeSymbol(co.symbol || o.symbol || old?.symbol || '');
      const ticker = this.store.tickersBySymbol[symbol];
      const side = ORDER_SIDE[co.side ?? o.side] ?? old?.side;
      if (!id || !this.store.marketsBySymbol[symbol] || !side) continue;
      const algoType = co.algoType;
      const source = ORDER_TRIGGER_TYPE[co.triggerPriceType] ?? old?.slTriggerType ?? old?.tpTriggerType;
      const callback = Number(co.callbackValue);
      const extreme = Number(co.extremePrice);
      const triggerPrice = Number(co.triggerPrice);
      const price =
        triggerPrice > 0
          ? triggerPrice
          : Number(
              Number.isFinite(extreme) && Number.isFinite(callback)
                ? extreme + (side === OrderSide.Buy ? callback : -callback)
                : old?.price,
            );
      if (!Number.isFinite(price) || !(price > 0)) continue;
      let type = ORDER_TYPE[algoType] ?? old?.type;
      if (algoType === 'STOP') {
        const reference = source === OrderStopTriggerType.LastPrice ? ticker?.last : ticker?.mark;
        type =
          old?.type ??
          (reference && (side === OrderSide.Sell ? price > reference : price < reference)
            ? OrderType.TakeProfit
            : OrderType.StopLoss);
      }
      if (!type) continue;
      const amount = Number(co.quantity ?? old?.amount ?? 0);
      const filled = Number(co.totalExecutedQuantity ?? co.executed ?? old?.filled ?? 0);
      if (!Number.isFinite(amount) || amount < 0 || !Number.isFinite(filled) || filled < 0) continue;
      const status =
        co.algoStatus === 'FILLED'
          ? OrderStatus.Closed
          : ['CANCELLED', 'CANCELED', 'REJECTED'].includes(co.algoStatus)
            ? OrderStatus.Canceled
            : OrderStatus.Open;
      result.push(
        this.reconcileOrder({
          id,
          parentId: String(
            o.rootAlgoOrderId ||
              co.rootAlgoOrderId ||
              (Array.isArray(o.childOrders) ? o.algoOrderId || o.orderId || o.id : undefined) ||
              old?.parentId ||
              id,
          ),
          status,
          symbol,
          type,
          side,
          price,
          amount,
          filled,
          remaining: Math.max(0, subtract(amount, filled)),
          reduceOnly: co.reduceOnly ?? o.reduceOnly ?? true,
          ...(type === OrderType.TakeProfit ? { tpTriggerType: source } : { slTriggerType: source }),
          lastUpdate: virtualClock.getCurrentTimeMs(),
          info: o,
        }),
      );
    }
    return result;
  };

  cancelOrders = async (orders: Order[]) => {
    // filter out orders with same parent id
    const seenParentIds = new Set<string>();
    orders = orders.filter((o) => {
      if (o.parentId && seenParentIds.has(o.parentId)) return false;
      seenParentIds.add(o.parentId!);
      return true;
    });

    try {
      await forEachSeries(orders, async (order) => {
        const isAlgo = this.isAlgoOrder(order.type);

        if (isAlgo) {
          await this.xhr.delete(`${ENDPOINTS.ALGO_ORDER}/${order.parentId || order.id}`);
        } else {
          await this.xhr.delete(ENDPOINTS.CANCEL_ORDER, {
            data: omitUndefined({
              order_id: order.id,
              symbol: reverseSymbol(order.symbol),
            }),
          });
        }
      });
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  cancelAlgoOrder = async (order: Order, recreateSibling = true) => {
    try {
      const sibling = this.store.orders.find((o) => o.parentId === order.parentId && o.id !== order.id);

      const id = order.parentId || order.id;
      await this.xhr.delete(`${ENDPOINTS.ALGO_ORDER}/${id}`);

      if (sibling && recreateSibling) {
        const priceKey = sibling.type === OrderType.StopLoss ? 'stopLoss' : 'takeProfit';
        const triggerKey = sibling.type === OrderType.StopLoss ? 'slTriggerType' : 'tpTriggerType';

        await this.placePositionalAlgoOrder({
          symbol: sibling.symbol,
          side: sibling.side,
          [priceKey]: sibling.price,
          [triggerKey]: sibling[triggerKey],
        });
      }
    } catch (err: any) {
      if (err?.response?.data?.message?.includes('already canceled')) {
        this.store.updateOrderStatus({ id: order.id }, OrderStatus.Canceled);
      } else {
        this.emitAndLogError(err?.response?.data?.message || err?.message);
        throw err;
      }
    }
  };

  getPositionSideForOrder = async (opts: Pick<PlaceOrderOpts, 'side' | 'reduceOnly'>) => {
    const positionMode = await this.getPositionMode();
    const reduceOnly = opts.reduceOnly;
    let positionSide = undefined;
    if (positionMode === PositionMode.Hedged) {
      if (reduceOnly) {
        if (opts.side === OrderSide.Buy) {
          positionSide = 'SHORT';
        } else {
          positionSide = 'LONG';
        }
      } else {
        if (opts.side === OrderSide.Buy) {
          positionSide = 'LONG';
        } else {
          positionSide = 'SHORT';
        }
      }
    }
    return positionSide;
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(orders);
    if (orders.some((order) => order.type === OrderType.Conditional)) {
      return this.rejectPreflight('WOO opening conditional orders are not implemented by the TS venue');
    }
    return (await mapSeries(orders, (order) => this.placeOrder(order))).flat();
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    const callerClientOrderId = this.resolveCallerClientOrderId(opts);
    if (opts.type === OrderType.Conditional)
      return this.rejectPreflight('WOO opening conditional orders are not implemented by the TS venue');
    if (this.isAlgoOrder(opts.type)) {
      return this.placeAlgoOrder(opts);
    }

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market not found: ${opts.symbol}`);
    }

    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);

    const price = opts.price ? adjust(opts.price, pPrice) : null;
    let type = inverseObj(ORDER_TYPE)[opts.type];
    let orderPrice = opts.type === OrderType.Limit ? price : undefined;
    const ticker = this.store.tickersBySymbol[opts.symbol];

    if (opts.timeInForce === OrderTimeInForce.PostOnly && opts.type === OrderType.Limit) {
      const limitPrice = price ?? undefined;
      if (isPositiveFinitePrice(limitPrice)) {
        const fairCross = isPostOnlyCrossingFairPrice(opts.side, limitPrice, ticker);
        if (fairCross?.wouldCross) {
          throw new Error(`WOO post-only ${opts.side} order would cross current fair price ${fairCross.fairPrice}`);
        }

        const touchPrice = await this.fetchMarketIocTouchPrice(opts.symbol, opts.side);
        if (isPositiveFinitePrice(touchPrice) && isPostOnlyCrossingPrice(opts.side, limitPrice, touchPrice)) {
          throw new Error(
            `WOO post-only ${opts.side} order would cross current ${
              opts.side === OrderSide.Buy ? 'ask' : 'bid'
            } ${touchPrice}`,
          );
        }
      }
      type = 'POST_ONLY';
    } else if (opts.type === OrderType.Market) {
      const slippagePct =
        typeof opts.maxSlippagePct === 'number' && Number.isFinite(opts.maxSlippagePct)
          ? opts.maxSlippagePct
          : DEFAULT_MARKET_IOC_SLIPPAGE_PCT;
      const priceRangePct = getWooMarketPriceRangePct(market);
      const effectiveSlippagePct = Math.min(slippagePct, priceRangePct);
      const touchPrice = await this.fetchMarketIocTouchPrice(opts.symbol, opts.side);
      const crossingPrice = chooseMarketIocOrderPrice(
        ticker,
        opts.side,
        effectiveSlippagePct,
        priceRangePct,
        touchPrice,
      );

      if (!crossingPrice || !Number.isFinite(crossingPrice) || crossingPrice <= 0) {
        throw new Error(`Ticker ${opts.symbol} missing valid price for WOO market IOC order`);
      }

      orderPrice = adjust(crossingPrice, pPrice);
      type = 'IOC';
    }

    const req = omitUndefined({
      symbol: reverseSymbol(opts.symbol),
      order_type: type,
      order_price: orderPrice,
      reduce_only: opts.reduceOnly,
      side: inverseObj(ORDER_SIDE)[opts.side],
      position_side: await this.getPositionSideForOrder(opts),
      client_order_id: callerClientOrderId === undefined ? undefined : Number(callerClientOrderId),
    });

    const payloads = splitWooQuantity(amount, market).map((quantity) => ({ ...req, order_quantity: quantity }));
    this.assertSingleNativeClientOrderId(callerClientOrderId, payloads.length);

    try {
      const orderIds = await mapSeries(payloads, async (payload) => {
        const { data } = await this.xhr.post<PLACE_ORDER_RESPONSE>(ENDPOINTS.PLACE_ORDER, payload);
        return String(data.order_id);
      });

      if (opts.stopLoss || opts.takeProfit) {
        const algoIds = await this.placePositionalAlgoOrder({
          ...opts,
          side: opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy,
        });
        orderIds.push(...algoIds);
      }

      return orderIds;
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  placeAlgoOrder = async (opts: PlaceOrderOpts) => {
    if (!opts.price) {
      this.emitAndLogError('Price is required for algo orders');
      return [];
    }

    if (opts.type === OrderType.TrailingStopLoss) {
      return this.placeTrailingStopLossOrder(opts);
    }

    if (opts.amount) {
      return this.placeStopMarket(opts);
    }

    const params = {
      ...opts,
      symbol: opts.symbol,
      side: opts.side === OrderSide.Buy ? OrderSide.Buy : OrderSide.Sell,
      stopLoss: 0,
      takeProfit: 0,
    };

    const sibling = this.store.orders.find(
      (o) =>
        o.symbol === opts.symbol &&
        o.side === opts.side &&
        o.type === (opts.type === OrderType.StopLoss ? OrderType.TakeProfit : OrderType.StopLoss) &&
        o.status === OrderStatus.Open,
    );

    if (opts.type === OrderType.StopLoss) {
      params.stopLoss = opts.price;
    }

    if (opts.type === OrderType.TakeProfit) {
      params.takeProfit = opts.price;
    }

    if (sibling) {
      if (sibling.type === OrderType.StopLoss) {
        params.stopLoss = sibling.price;
        params.slTriggerType = sibling.slTriggerType;
      }
      if (sibling.type === OrderType.TakeProfit) {
        params.takeProfit = sibling.price;
        params.tpTriggerType = sibling.tpTriggerType;
      }
      await this.cancelAlgoOrder(sibling, false);
    }

    return await this.placePositionalAlgoOrder(params);
  };

  placeStopMarket = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    const ticker = this.store.tickersBySymbol[opts.symbol];

    if (!market) throw new Error(`Market ${opts.symbol} not found`);
    if (!ticker) throw new Error(`Ticker ${opts.symbol} not found`);

    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;

    const amount = adjust(opts.amount, pAmount);

    const price = opts.price ? adjust(opts.price, pPrice) : null;

    const rawTriggerType =
      (opts.type === OrderType.StopLoss || opts.type === OrderType.TrailingStopLoss
        ? opts.slTriggerType
        : opts.tpTriggerType) ||
      opts.triggerType ||
      OrderStopTriggerType.MarkPrice;

    const triggerType = inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType];

    const req = omitUndefined({
      symbol: reverseSymbol(opts.symbol),
      type: 'MARKET',
      algoType: 'STOP',
      side: inverseObj(ORDER_SIDE)[opts.side],
      triggerPrice: String(price),
      triggerPriceType: triggerType,
      reduceOnly: true,
      positionSide: await this.getPositionSideForOrder({
        ...opts,
        reduceOnly: true,
      }),
    });

    const payloads = splitWooQuantity(amount, market).map((quantity) => ({ ...req, quantity: String(quantity) }));

    try {
      const orderIds = await mapSeries(payloads, async (payload) => {
        const {
          data: { data },
        } = await this.xhr.post<PLACE_ALGO_ORDER_RESPONSE>(ENDPOINTS.ALGO_ORDER, payload);

        return data.rows.map((row) => String(row.orderId));
      });
      return orderIds.flat();
    } catch (err: any) {
      this.emitAndLogError(err, err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  placeTrailingStopLossOrder = async (opts: PlaceOrderOpts) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    const ticker = this.store.tickersBySymbol[opts.symbol];

    const pSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;

    const position = this.store.positions.find((p) => p.symbol === opts.symbol && p.side === pSide);

    if (!market) throw new Error(`Market ${opts.symbol} not found`);
    if (!ticker) throw new Error(`Ticker ${opts.symbol} not found`);

    if (!position) {
      throw new Error(`Position ${opts.symbol} and side ${pSide} not found`);
    }

    const priceDistance = adjust(
      Math.max(ticker.last, opts.price!) - Math.min(ticker.last, opts.price!),
      market.precision.price,
    );

    if (!(priceDistance > 0) || !Number.isFinite(priceDistance) || !(position.contracts > 0)) {
      return this.rejectPreflight('WOO trailing callback distance and quantity must be positive');
    }
    const rawTriggerType =
      opts.slTriggerType || opts.tpTriggerType || opts.triggerType || OrderStopTriggerType.MarkPrice;
    const triggerType = inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType];

    const payload = omitUndefined({
      symbol: reverseSymbol(opts.symbol),
      type: 'MARKET',
      algoType: 'TRAILING_STOP',
      callbackValue: `${priceDistance}`,
      side: inverseObj(ORDER_SIDE)[opts.side],
      reduceOnly: true,
      quantity: `${position.contracts}`,
      triggerPriceType: triggerType,
      positionSide: await this.getPositionSideForOrder({
        ...opts,
        reduceOnly: true,
      }),
    });

    try {
      const {
        data: { data },
      } = await this.xhr.post<{
        data: { rows: Array<Record<string, any>> };
      }>(ENDPOINTS.ALGO_ORDER, payload);

      return data?.rows?.map((r) => String(r.orderId)) || [];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  placePositionalAlgoOrder = async (
    opts: Pick<
      PlaceOrderOpts,
      'side' | 'stopLoss' | 'symbol' | 'takeProfit' | 'tpTriggerType' | 'slTriggerType' | 'triggerType'
    >,
  ) => {
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }
    const pPrice = market.precision.price;
    const otherProps = omitUndefined({
      reduceOnly: true,
      positionSide: await this.getPositionSideForOrder({
        ...opts,
        reduceOnly: true,
      }),
    });

    const req: Record<string, any> = {
      ...otherProps,
      symbol: reverseSymbol(opts.symbol),
      algoType: 'POSITIONAL_TP_SL',
      childOrders: [],
    };

    if (opts.stopLoss) {
      const rawTriggerType = opts.slTriggerType || OrderStopTriggerType.MarkPrice;
      const triggerType = inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType];

      req.childOrders.push({
        ...otherProps,
        algoType: 'STOP_LOSS',
        type: 'CLOSE_POSITION',
        triggerPriceType: triggerType,
        side: opts.side === OrderSide.Sell ? 'SELL' : 'BUY',
        triggerPrice: `${adjust(opts.stopLoss, pPrice)}`,
      });
    }

    if (opts.takeProfit) {
      const rawTriggerType = opts.tpTriggerType || opts.triggerType || OrderStopTriggerType.MarkPrice;
      const triggerType = inverseObj(ORDER_TRIGGER_TYPE)[rawTriggerType];

      req.childOrders.push({
        ...otherProps,
        algoType: 'TAKE_PROFIT',
        type: 'CLOSE_POSITION',
        triggerPriceType: triggerType,
        side: opts.side === OrderSide.Sell ? 'SELL' : 'BUY',
        triggerPrice: `${adjust(opts.takeProfit, pPrice)}`,
      });
    }

    try {
      const {
        data: { data },
      } = await this.xhr.post<{
        data: { rows: Array<Record<string, any>> };
      }>(ENDPOINTS.ALGO_ORDER, req);

      return data?.rows?.map((r) => String(r.orderId)) || [];
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  cancelAllOrders = async () => {
    const errors: unknown[] = [];
    for (const endpoint of [ENDPOINTS.CANCEL_ORDERS, ENDPOINTS.CANCEL_ALGO_ORDERS]) {
      try {
        await this.xhr.delete(endpoint);
      } catch (error) {
        errors.push(error);
      }
    }
    if (errors.length) {
      this.emitAndLogError(String(errors[0]));
      throw errors[0];
    }
  };

  cancelSymbolOrders = async (symbol: string) => {
    try {
      await this.xhr.delete(`/v3/merge/orders/pending/${reverseSymbol(symbol)}`);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    // const positionMode = await this.getPositionMode();
    try {
      //   if (positionMode === PositionMode.OneWay) {
      await this.xhr.post(ENDPOINTS.FUTURES_LEVERAGE, {
        symbol: reverseSymbol(symbol),
        margin_mode: 'CROSS',
        position_side: 'BOTH',
        leverage: inputLeverage,
      });
      this.updateCachedPositionMetadataLeverage(symbol, inputLeverage);
      // } else {
      //   // const sides = side ? [side === PositionSide.Long ? 'LONG' : 'SHORT'] : ['LONG', 'SHORT'];
      //   // WOO seems to actually ignore this and always sets both, so to avoid regression lets just set both explicitly
      //   const sides = ['LONG', 'SHORT'];
      //   await mapSeries(sides, async (side) => {
      //     await this.xhr.post(ENDPOINTS.FUTURES_LEVERAGE, {
      //       symbol: reverseSymbol(symbol),
      //       margin_mode: 'CROSS',
      //       position_side: side,
      //       leverage: inputLeverage,
      //     });
      //   });
      // }
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      throw err;
    }
  };

  changePositionMode = async (hedged: boolean) => {
    const expectedPositionMode = hedged ? PositionMode.Hedged : PositionMode.OneWay;
    try {
      // ONE_WAY | HEDGE_MODE
      await this.xhr.post(ENDPOINTS.POSITION_MODE, { position_mode: hedged ? 'HEDGE_MODE' : 'ONE_WAY' });
      await this.confirmPositionModeAndRefreshPositions(expectedPositionMode);
    } catch (e: any) {
      if (isWooPositionModeAlreadySetError(e)) {
        await this.confirmPositionModeAndRefreshPositions(expectedPositionMode);
        return;
      }
      // {
      //   "success": false,
      //   "code": -1005,
      //   "error_code": 111000,
      //   "message": "Position mode cannot be altered while there are pending orders."
      // }
      this.emitAndLogError(e, e?.response?.data?.message || e?.message);
      throw e;
    }
  };

  private isAlgoOrder = (orderType: OrderType) => {
    return (
      orderType === OrderType.StopLoss || orderType === OrderType.TakeProfit || orderType === OrderType.TrailingStopLoss
    );
  };
}
