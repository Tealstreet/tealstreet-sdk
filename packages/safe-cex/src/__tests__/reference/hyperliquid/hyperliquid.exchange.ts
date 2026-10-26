import type { Axios } from 'axios';
import type { Store } from '../../../store/store.interface';
import type { ClientOrderLookup, ReplacementOrderIds } from '../../../exchanges/clientOrderId';
import type { HyperliquidPlacementRecord } from '../../../exchanges/hyperliquid/hyperliquid.cloid';
import type { BuilderFeeCheckResult, BuilderInfo } from '../../../exchanges/hyperliquid/hyperliquid.types';

import rateLimit from 'axios-rate-limit';
import dayjs, { ManipulateType } from 'dayjs';
import uniqBy from 'lodash/uniqBy';
import { mapSeries } from 'p-iteration';

import { numberToString, parsePrecision } from '@tealstreet/ccxt-compat/src/functions/ccxt_exchange';
import {
  DECIMAL_PLACES,
  decimalToPrecision,
  NO_PADDING,
  precisionFromString,
  ROUND,
  SIGNIFICANT_DIGITS,
} from '@tealstreet/ccxt-compat/src/functions/number';
import { safeString } from '@tealstreet/ccxt-compat/src/functions/type';
import {
  Candle,
  ClosedPnlEntry,
  CoinBalance,
  ExchangeName,
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
  Order,
  OrderBook,
  OrderHistoryOptions,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PlaceOrderOpts,
  Position,
  PositionMode,
  PositionSide,
  Ticker,
  UpdateOrderOpts,
  Writable,
} from '../../../types';
import { createPositionId } from '../../../utils/createPositionId';
import { createWalletFromPrivateKey } from '../../../utils/ethereum/createWalletFromPrivateKey';
import { validateWalletAndPrivateKey } from '../../../utils/ethereum/validateWalletAndPrivateKey';
import { v } from '../../../utils/get-key';
import { loop } from '../../../utils/loop';
import { adjust } from '../../../utils/safe-math';
import { virtualClock } from '../../../utils/virtualClock';
import { BaseExchange } from '../../../exchanges/base';
import { orderCarriesClientOrderId, withReplacementClientOrderId } from '../../../exchanges/clientOrderId';
import { createExchangeAPI, createInfoAPI } from './hyperliquid.api';
import { checkBuilderFeeApproved } from '../../../exchanges/hyperliquid/hyperliquid.builderFee';
import {
  HYPERLIQUID_CLOID_PATTERN,
  hyperliquidClientOrderIdRule,
  mintHyperliquidCloid,
  placementRecordFromWire,
  readHyperliquidCloid,
} from '../../../exchanges/hyperliquid/hyperliquid.cloid';
import {
  PERP_MAX_PRICE_DECIMALS,
  SPOT_ASSET_INDEX_OFFSET,
  SPOT_MAX_PRICE_DECIMALS,
  resolveHyperliquidBuilder,
} from '../../../exchanges/hyperliquid/hyperliquid.constants';
import { getCrossCache, hasSeenFill, markFillSeen } from './hyperliquid.cross-cache';
import {
  deriveHyperliquidClosedPositions,
  mapHyperliquidFillToIncomeRecords,
  mapHyperliquidFundingToIncomeRecord,
  paginateHyperliquidByTime,
} from './hyperliquid.income';
import {
  fetchCoreMetaAndAssetCtxsWithFallback,
  fetchHip3MetaAndAssetCtxsWithRetry,
} from './hyperliquid.market-startup-helpers';
import { collapseHyperliquidOrderHistory, mapHyperliquidHistoricalOrder } from './hyperliquid.order-history';
import { inferHyperliquidOrderTypeAndPrice, parseHyperliquidOrderTimeInForce } from '../../../exchanges/hyperliquid/hyperliquid.order-inference';
import {
  hyperliquidOrderResponseHasDeferredOrderStatus,
  HyperliquidUnreadableOrderResponseError,
  isOnlyHyperliquidTerminalCancelError,
  parseHyperliquidOrderResponse,
} from './hyperliquid.order-response';
import {
  orderRequestToOrderWire,
  orderWiresToNormalTPSLAction,
  orderWiresToOrderAction,
  orderWiresToPositionalTPSLAction,
  signL1Action,
} from './hyperliquid.signing';
import {
  AssetCtx,
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  HyperliquidUserAbstractionMode,
  INFO_TYPES,
  Meta,
  OrderWire,
  PerpDex,
  PerpDexs,
  ScheduleCancelAction,
  SpotAssetCtx,
  SpotClearinghouseState,
  SpotMeta,
  SpotToken,
} from '../../../exchanges/hyperliquid/hyperliquid.types';
import { calculatePricePrecision, symbolToCoin } from './hyperliquid.utils';
import { HyperliquidPrivateWebSocket } from './hyperliquid.ws-private';
import { HyperliquidPublicWebSocket } from './hyperliquid.ws-public';

// Export fill deduplication functions for use by WebSocket handlers
export { hasSeenFill, markFillSeen };

const parseFiniteNumber = (value: unknown): number => {
  const parsed = typeof value === 'number' ? value : parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

const maskHyperliquidAddress = (address: unknown): string | undefined => {
  if (typeof address !== 'string' || !address) return undefined;
  return address.length > 14 ? `${address.slice(0, 8)}...${address.slice(-6)}` : address;
};

const parseHyperliquidExchangePayload = (requestData: unknown): any | undefined => {
  if (typeof requestData === 'string') {
    try {
      return JSON.parse(requestData);
    } catch {
      return undefined;
    }
  }
  return requestData && typeof requestData === 'object' && !Array.isArray(requestData) ? requestData : undefined;
};

const calculatePriceChangePercentage = (price: number, prevDayPx: number): number => {
  if (!Number.isFinite(price) || !Number.isFinite(prevDayPx) || prevDayPx <= 0) return 0;
  return ((price - prevDayPx) / prevDayPx) * 100;
};

const parseHyperliquidVolumeFields = (
  sourceTicker: AssetCtx | SpotAssetCtx,
  price: number,
): { volume: number; quoteVolume: number } => {
  const quoteVolume = parseFiniteNumber(v(sourceTicker, 'dayNtlVlm'));
  const dayBaseVolume = parseFiniteNumber(v(sourceTicker, 'dayBaseVlm'));
  const volume = dayBaseVolume > 0 ? dayBaseVolume : price > 0 ? quoteVolume / price : 0;

  return { volume, quoteVolume };
};

const parseHyperliquidOpenInterestNotional = (sourceTicker: AssetCtx, price: number): number => {
  const openInterestBase = parseFiniteNumber(v(sourceTicker, 'openInterest'));
  return price > 0 ? openInterestBase * price : 0;
};

const isFinitePositiveNumber = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value) && value > 0;

export class HyperliquidExchange extends BaseExchange {
  /** Journal records: this venue has no reported closed axis, so round trips fold from fills. */
  fetchHistoricalPositions = async (opts: { account: string; from?: number; to?: number }) =>
    this.foldClosedPositionRecords(opts, { symbolScoped: true });

  xhr: Axios; // For /exchange endpoint (trading operations)
  infoXhr: Axios; // For /info endpoint (read-only operations)

  // Every placement carries a cloid (`hyperliquid.cloid.ts`); a caller's goes on the main wire verbatim.
  protected readonly clientOrderIdRule = hyperliquidClientOrderIdRule;
  unlimitedXHR: Axios;

  public publicWebsocket: HyperliquidPublicWebSocket;
  public privateWebsocket: HyperliquidPrivateWebSocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private lastNonce: number = 0;
  private static readonly BUILDER_FEE_APPROVAL_TIMEOUT_MS = 8_000;
  private static readonly HIP3_DEX_DISCOVERY_TIMEOUT_MS = 6_000;
  /**
   * A FIXED budget here is a time bomb. `hyperliquidRestRateLimiter` paces by weight
   * at 50ms/unit and `metaAndAssetCtxs` is weight 20, so every dex reserves ~1s of a
   * strictly serial timeline — 10 live dexes need ~10s against the old flat 8s, and
   * the list grows as builders deploy. Derived from the count, floored so small
   * deployments are no slower and capped so a pathological list cannot stall startup.
   */
  private static readonly HIP3_MARKETS_LOAD_TIMEOUT_MS = 8_000;
  private static readonly HIP3_PER_DEX_LOAD_BUDGET_MS = 2_000;
  /**
   * Hard ceiling, and the number is not arbitrary: `ExchangeWorker.start()` races the
   * WHOLE of `start()` against 30s, so HIP-3 must never be able to eat that budget —
   * core perps, spot, fees, builder fee and the sockets all come out of the same 30s.
   * Past this many dexes the load goes partial, which is now graceful rather than an
   * outage; an unbounded budget would instead turn it into a startup failure.
   */
  private static readonly HIP3_MARKETS_LOAD_TIMEOUT_CAP_MS = 15_000;
  private static readonly SPOT_MARKETS_LOAD_TIMEOUT_MS = 6_000;
  private static readonly USER_ROLE_RESOLVE_TIMEOUT_MS = 6_000;
  private static readonly USER_ABSTRACTION_RESOLVE_TIMEOUT_MS = 6_000;
  private static readonly SPOT_COLLATERAL_REFRESH_INTERVAL_MS = 10_000;
  private static readonly UNIFIED_MARGIN_CURRENCY = 'USDC';

  private userAbstractionMode: HyperliquidUserAbstractionMode = 'standard';
  private spotCollateralRefreshInFlight: Promise<void> | null = null;
  private lastSpotCollateralRefreshAt = 0;
  private hasSpotCollateralSnapshot = false;
  private cachedSpotCollateralState: SpotClearinghouseState = { balances: [] };
  private latestAuthoritativeUnifiedBalanceSnapshot: { total: number; used: number; free: number } | null = null;
  private hasAuthoritativeUnifiedSpotWsSnapshot = false;
  private latestCorePerpsAccountValue = 0;
  private latestCorePerpsUsedMargin = 0;
  private latestCorePerpsUpnl = 0;

  private generateNonce(): number {
    const now = Date.now();
    // Ensure nonce is always increasing and unique
    this.lastNonce = Math.max(now, this.lastNonce + 1);
    return this.lastNonce;
  }

  private withStartupTimeout = async <T>(
    operation: Promise<T>,
    timeoutMs: number,
    operationName: string,
    fallback: T,
  ): Promise<T> => {
    let timeoutId: ReturnType<typeof setTimeout> | null = null;
    const timeoutPromise = new Promise<T>((resolve) => {
      timeoutId = setTimeout(() => {
        this.log(
          `${operationName} timed out after ${Math.round(timeoutMs / 1000)}s; continuing with fallback`,
          LogSeverity.Warning,
        );
        resolve(fallback);
      }, timeoutMs);
    });

    try {
      return await Promise.race([operation, timeoutPromise]);
    } finally {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
    }
  };

  private getOrderBasePrice = (market: Market, purpose: string): number => {
    const ticker = this.store.tickersBySymbol[market.symbol];
    const candidates = market.isSpot
      ? [ticker?.last, ticker?.midPrice, ticker?.mark, ticker?.index]
      : [ticker?.mark, ticker?.last, ticker?.midPrice, ticker?.index];
    const price = candidates.find(isFinitePositiveNumber);

    if (!price) {
      throw new Error(`Cannot place Hyperliquid ${purpose} for ${market.symbol}: missing valid ticker price data`);
    }

    return price;
  };

  private shouldUsePositionTpsl = (opts: PlaceOrderOpts, market: Market, orderSize: number): boolean => {
    if (opts.amount === 0) return true;
    if (!opts.reduceOnly || !isFinitePositiveNumber(orderSize)) return false;

    const targetSide = opts.side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    const position = this.store.positions.find(
      (p) => p.symbol === market.symbol && p.side === targetSide && p.contracts > 0,
    );
    if (!position) return false;

    return orderSize >= position.contracts;
  };

  /**
   * Builds a lowercase market lookup map for O(1) case-insensitive market lookups.
   * This prevents O(n) linear searches when dealing with HIP-3 markets that may have case mismatches.
   */
  private buildMarketsLowerCaseMap(markets: Market[]): void {
    this.marketsLowerCaseMap.clear();
    for (const market of markets) {
      this.marketsLowerCaseMap.set(market.id.toLowerCase(), market);
    }
    this.log(
      `Built case-insensitive market lookup map with ${this.marketsLowerCaseMap.size} markets`,
      LogSeverity.Debug,
    );
  }

  public activeAssetData: Map<
    string,
    {
      user: string;
      coint: string;
      leverage: {
        type: 'cross' | 'isolated';
        value: number;
      };
      maxTradeSzs: [number, number];
      availableToTrade: [string, string];
    }
  > = new Map();

  // List of HIP-3 DEX names for WebSocket subscriptions
  public hip3DexNames: string[] = [];

  // O(1) case-insensitive market lookup map for HIP-3 markets
  // Built when markets are loaded and used as fallback for case-mismatched coin lookups
  public marketsLowerCaseMap = new Map<string, Market>();

  // Periodic refresh of HIP-3 orders to ensure order types stay accurate
  // WS orderUpdates doesn't include orderType for HIP-3, so we re-fetch from REST when needed
  private _lastOrderRefreshTime = 0;
  // Minimum time between HIP-3 order refreshes (when triggered by unknown WS orders)
  // Keep this short since we skip showing unknown HIP-3 orders until REST confirms their type
  private readonly HIP3_ORDER_REFRESH_INTERVAL_MS = 1000; // 1 second minimum between refreshes
  // Flag set when WS receives a HIP-3 order that we couldn't find in the store (new order from another device)
  // This triggers a refresh on next tick to get correct order types
  public _hip3OrdersNeedRefresh = false;

  // NOTE: Session-only caches for HIP-3 orders now use cross-context cache module
  // This ensures all HyperliquidExchange instances share the same cache, even across
  // different JS contexts (web workers, iframes) within the same browser tab.

  // Add to pre-placement cache before REST call (delegates to cross-context cache)
  public addToPrePlacementCache(
    coin: string,
    side: OrderSide,
    type: OrderType,
    price: number,
    amount: number,
    cloid?: string,
  ): void {
    getCrossCache().addPrePlacement(coin, side, type, price, amount, cloid);
  }

  // Consume the pre-placement entry of an order the WS handler identified by its cloid record.
  public forgetPrePlacement(coin: string, side: OrderSide, cloid: string): void {
    getCrossCache().removePrePlacementByCloid(coin, side, cloid);
  }

  // ============================================================================
  // PLACEMENT RECORDS (cloid -> what we signed)
  // The PRIMARY way the WS handler identifies our own orders: written before the
  // REST call, read on the first `orderUpdates` frame (which echoes the cloid).
  // The pre-placement / pending caches and the reduce-only deferral are the
  // fallback for frames with no cloid or no record (orders from other clients,
  // or a context the record never reached — the hosted exchange-server).
  // ============================================================================
  public recordPlacement(wires: OrderWire[], symbol: string): void {
    const placedAt = Date.now();
    const cache = getCrossCache();
    for (const wire of wires) {
      if (wire.c) cache.addPlacementRecord(wire.c, placementRecordFromWire(wire, symbol, placedAt));
    }
  }

  public getPlacementRecord(cloid: string): HyperliquidPlacementRecord | undefined {
    return getCrossCache().getPlacementRecord(cloid);
  }

  public forgetPlacementRecord(cloid: string): void {
    getCrossCache().removePlacementRecord(cloid);
  }

  // Match incoming order to pre-placement cache (delegates to cross-context cache)
  // wsLimitPx and wsAmount are used to disambiguate when multiple orders exist for the same coin+side
  public matchPrePlacement(
    coin: string,
    side: OrderSide,
    wsLimitPx?: number,
    wsAmount?: number,
  ): { type: OrderType; price: number } | undefined {
    return getCrossCache().matchPrePlacement(coin, side, wsLimitPx, wsAmount);
  }

  // Strict trigger-only match for core-perp stop entries (see cross-cache)
  public matchPrePlacementTrigger(coin: string, side: OrderSide, wsAmount: number): { type: OrderType; price: number } | undefined {
    return getCrossCache().matchPrePlacementTrigger(coin, side, wsAmount);
  }

  // Get pending HIP-3 order from cross-context cache
  public getPendingHip3Order(orderId: string): { type: OrderType; price: number } | undefined {
    return getCrossCache().getPendingOrder(orderId);
  }

  // Add to pending HIP-3 orders cross-context cache
  public addPendingHip3Order(orderId: string, type: OrderType, price: number): void {
    getCrossCache().addPendingOrder(orderId, type, price);
  }

  // Remove from pending HIP-3 orders cross-context cache
  public removePendingHip3Order(orderId: string): void {
    getCrossCache().removePendingOrder(orderId);
  }

  // ============================================================================
  // TIME-IN-FORCE PRESERVATION
  // `orderUpdates` carries no `tif` for ANY market (core perps included — the
  // field is absent from the WS frame, not just from HIP-3 ones), and `tick()`
  // only re-fetches orders when HIP-3 dexes are configured. So for core perps
  // the placement-time value is the only lasting source of truth.
  // ============================================================================
  private _placedLimitOrderTif = new Map<string, OrderTimeInForce>();
  private readonly PLACED_TIF_CACHE_LIMIT = 500;

  public rememberLimitOrderTimeInForce(orderId: string, timeInForce: OrderTimeInForce): void {
    this._placedLimitOrderTif.delete(orderId);
    this._placedLimitOrderTif.set(orderId, timeInForce);
    while (this._placedLimitOrderTif.size > this.PLACED_TIF_CACHE_LIMIT) {
      const oldest = this._placedLimitOrderTif.keys().next();
      if (oldest.done) break;
      this._placedLimitOrderTif.delete(oldest.value);
    }
  }

  public getPlacedLimitOrderTimeInForce(orderId: string): OrderTimeInForce | undefined {
    return this._placedLimitOrderTif.get(orderId);
  }

  public forgetLimitOrderTimeInForce(orderId: string): void {
    this._placedLimitOrderTif.delete(orderId);
  }

  // Cache for fake positions - invalidated when markets change
  private _fakePositionsCache: Position[] | null = null;

  // Builder fee approval status: null = unknown, true = approved, false = not approved
  private _builderFeeApproved: boolean | null = null;
  private _builderFeeLastCheck: BuilderFeeCheckResult | null = null;

  // ============================================================================
  // CANCELLATION SUPPRESSION
  // Tracks recently cancelled orders to prevent stale WS snapshots from
  // re-adding them. REST backfill is the source of truth for reconciliation.
  // ============================================================================
  private _cancelledOrders = new Map<string, number>(); // orderId → cancel timestamp
  private readonly CANCEL_SUPPRESSION_MAX_AGE_MS = 120_000; // Auto-expire after 2 minutes
  private readonly CANCEL_SUPPRESSION_TRUST_WINDOW_MS = 30_000; // Trust cancel for 30s before allowing REST override

  /**
   * Check if an order is suppressed due to recent cancellation.
   * Also auto-cleans expired entries.
   */
  public isOrderCancelled(orderId: string): boolean {
    const cancelTime = this._cancelledOrders.get(orderId);
    if (cancelTime === undefined) return false;

    // Auto-expire stale entries
    if (Date.now() - cancelTime > this.CANCEL_SUPPRESSION_MAX_AGE_MS) {
      this._cancelledOrders.delete(orderId);
      return false;
    }

    return true;
  }

  /**
   * Mark an order as recently cancelled. Stale WS/REST open snapshots will be
   * suppressed until REST backfill confirms the cancellation or the suppression expires.
   */
  public suppressCancelledOrder(orderId: string): void {
    this._cancelledOrders.set(orderId, Date.now());
  }

  /**
   * Remove cancellation suppression (e.g., REST confirmed it's still open).
   */
  private unsuppressCancelledOrder(orderId: string): void {
    this._cancelledOrders.delete(orderId);
  }

  // Debug instance ID to track if multiple instances exist
  public readonly _instanceId = Math.random().toString(36).substr(2, 9);

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.xhr = rateLimit(createExchangeAPI(opts, this), { maxRPS: 3 });
    this.infoXhr = createInfoAPI(opts, this);
    this.unlimitedXHR = createExchangeAPI(opts, this);

    this.publicWebsocket = new HyperliquidPublicWebSocket(this);
    this.privateWebsocket = new HyperliquidPrivateWebSocket(this);
  }

  private get hasPrivateSessionIdentity(): boolean {
    return Boolean(this.options.vault || this.options.applicationId);
  }

  /**
   * Hyperliquid private account streams are keyed by vault/applicationId (wallet),
   * not by key/secret presence. Keep base public-only behavior only when explicitly
   * requested, or when no wallet identity is available.
   */
  get isPublicDataOnly(): boolean {
    if (this.options.publicDataOnly === true) return true;
    return !this.hasPrivateSessionIdentity;
  }

  public shouldUseSpotCollateralBalance = (): boolean =>
    this.userAbstractionMode === 'unifiedAccount' || this.userAbstractionMode === 'portfolioMargin';

  public shouldRequestPortfolioMarginSpotState = (): boolean => this.userAbstractionMode === 'portfolioMargin';

  /**
   * Whether to fetch `spotClearinghouseState` at all.
   *
   * Unified/portfolio-margin accounts need it because spot IS their collateral.
   * Standard-abstraction accounts need it too once they opt into spot markets —
   * otherwise the merged connection lists spot pairs the user can trade while
   * `balance.coins` stays empty and the margin panel's Spot tab reads zero.
   * The separate spot exchange this supersedes fetched it unconditionally.
   */
  public shouldTrackSpotHoldings = (): boolean =>
    this.shouldUseSpotCollateralBalance() || Boolean(this.options.enableSpotMarkets);

  private resolveUserAbstractionMode = async (): Promise<void> => {
    const user = this.options.vault || this.options.applicationId;
    if (!user) return;

    const requestBody = {
      type: INFO_TYPES.USER_ABSTRACTION,
      user,
    };
    const requestConfig = {
      headers: { 'Content-Type': 'application/json' },
    };
    const fallbackMode: HyperliquidUserAbstractionMode = 'standard';

    const fetchMode = async (): Promise<HyperliquidUserAbstractionMode> => {
      try {
        const { data } = await this.infoXhr.post(ENDPOINTS.INFO, requestBody, requestConfig);
        const mode = `${data || ''}` as HyperliquidUserAbstractionMode;
        if (
          mode === 'unifiedAccount' ||
          mode === 'portfolioMargin' ||
          mode === 'disabled' ||
          mode === 'default' ||
          mode === 'dexAbstraction'
        ) {
          return mode;
        }
        this.log(`userAbstraction returned unexpected mode=${mode}; defaulting to standard`, LogSeverity.Warning);
        return fallbackMode;
      } catch (error) {
        this.log(`userAbstraction resolution failed: ${error}`, LogSeverity.Warning);
        return fallbackMode;
      }
    };

    this.userAbstractionMode = await this.withStartupTimeout(
      fetchMode(),
      HyperliquidExchange.USER_ABSTRACTION_RESOLVE_TIMEOUT_MS,
      'userAbstraction resolution',
      fallbackMode,
    );

    this.log(
      `Detected Hyperliquid account abstraction mode: ${this.userAbstractionMode}`,
      this.shouldUseSpotCollateralBalance() ? LogSeverity.Info : LogSeverity.Debug,
    );
  };

  private getSpotCollateralMarkPrice = (coin: string): number => {
    const normalizedCoin = `${coin || ''}`.trim();
    if (!normalizedCoin) return 0;

    const upperCoin = normalizedCoin.toUpperCase();
    if (upperCoin === 'USDC' || upperCoin === 'USDT' || upperCoin === 'USD' || upperCoin === 'USDH') {
      return 1;
    }

    // Prefer the real spot pair. Before spot markets were loaded into this
    // store, the only reachable price was the coin's *perp* ticker, so any
    // spot-only token (no perp listing) priced at 0 and silently contributed
    // 0 notional to a unified-account user's balance. Perp remains the
    // fallback for tokens with no spot pair.
    // USDC first, then the other ~$1 stables Hyperliquid quotes spot pairs in
    // (a live check found USDH/USDT0/USDE pairs), then the coin's perp.
    const candidates = [
      this.store.tickersBySymbol[`${normalizedCoin}/USDC`],
      this.store.tickersBySymbol[`${upperCoin}/USDC`],
      this.store.tickersBySymbol[`${upperCoin}/USDH`],
      this.store.tickersBySymbol[`${upperCoin}/USDT0`],
      this.store.tickersBySymbol[`${upperCoin}/USDE`],
      this.store.tickersById[normalizedCoin],
      this.store.tickersById[upperCoin],
      this.store.tickersBySymbol[`${upperCoin}-PERP`],
    ];

    // Field preference stays `mark -> last -> index`, matching the behaviour
    // before spot markets existed here. Only the SOURCE ladder above is new
    // (prefer the real spot pair over the coin's perp). Re-ordering the fields
    // would quietly re-value collateral for unified/portfolio-margin accounts
    // that never opted into spot, since that balance path is not gated by
    // `enableSpotMarkets`.
    for (const ticker of candidates) {
      if (!ticker) continue;
      const price = Number(ticker.mark ?? ticker.last ?? ticker.index);
      if (Number.isFinite(price) && price > 0) return price;
    }

    return 0;
  };

  private buildUnifiedSpotCollateralSnapshot = (
    balances: SpotClearinghouseState['balances'],
  ): {
    total: number;
    spotHoldNotional: number;
    spotCurrencies: { [asset: string]: { free: number; used: number; total: number; upnl: number } };
    coins: { [coin: string]: CoinBalance };
  } => {
    const spotCurrencies: { [asset: string]: { free: number; used: number; total: number; upnl: number } } = {};
    const coins: { [coin: string]: CoinBalance } = {};

    let total = 0;
    let spotHoldNotional = 0;

    for (const balance of balances) {
      const coin = `${balance?.coin || ''}`.trim();
      if (!coin) continue;

      const totalAmount = Number(balance?.total);
      const holdAmount = Number(balance?.hold);
      if (!Number.isFinite(totalAmount) || !Number.isFinite(holdAmount)) continue;

      const freeAmount = Math.max(totalAmount - holdAmount, 0);
      const markPrice = this.getSpotCollateralMarkPrice(coin);
      const totalNotional = totalAmount * markPrice;
      const holdNotional = holdAmount * markPrice;

      spotCurrencies[coin] = {
        total: totalAmount,
        used: holdAmount,
        free: freeAmount,
        upnl: 0,
      };
      coins[coin] = {
        base: coin,
        amount: totalAmount,
        notional: totalNotional,
      };

      total += totalNotional;
      spotHoldNotional += holdNotional;
    }

    return {
      total,
      spotHoldNotional,
      spotCurrencies,
      coins,
    };
  };

  private getAuthoritativeUnifiedUsdcAvailability = (state: SpotClearinghouseState): number | null => {
    const tokenToAvailableAfterMaintenance = state.tokenToAvailableAfterMaintenance;
    if (!Array.isArray(tokenToAvailableAfterMaintenance)) return null;

    for (const [token, available] of tokenToAvailableAfterMaintenance) {
      if (token !== 0) continue;
      const amount = Number(available);
      return Number.isFinite(amount) ? amount : null;
    }

    return null;
  };

  private applyBalanceFromCurrentMode = (): void => {
    if (this.shouldUseSpotCollateralBalance() && this.hasSpotCollateralSnapshot) {
      const spotSnapshot = this.buildUnifiedSpotCollateralSnapshot(this.cachedSpotCollateralState.balances);
      const upnl = this.latestCorePerpsUpnl;
      // An authoritative spot snapshot is internally consistent at one HyperCore
      // block. Keep its wallet total/used/free together until the next authoritative
      // spot state; ticker-derived PnL may update independently between those frames.
      const authoritativeSnapshot = this.latestAuthoritativeUnifiedBalanceSnapshot;
      const total = authoritativeSnapshot?.total ?? spotSnapshot.total;
      const used = authoritativeSnapshot?.used ?? this.latestCorePerpsUsedMargin + spotSnapshot.spotHoldNotional;
      const free = authoritativeSnapshot?.free ?? spotSnapshot.total + upnl - used;
      const currencies = {
        ...spotSnapshot.spotCurrencies,
        [HyperliquidExchange.UNIFIED_MARGIN_CURRENCY]: {
          total,
          used,
          free,
          upnl,
        },
      };

      this.store.update({
        balance: {
          total,
          used,
          free,
          upnl,
          currencies: currencies as any,
          coins: spotSnapshot.coins,
          info: {
            userAbstractionMode: this.userAbstractionMode,
            source: 'spotClearinghouseState+clearinghouseState',
            spotCollateralCurrencies: spotSnapshot.spotCurrencies,
          },
        },
      });
      return;
    }

    const accountValue = this.latestCorePerpsAccountValue;
    const used = this.latestCorePerpsUsedMargin;
    const upnl = this.latestCorePerpsUpnl;

    // Standard abstraction: spot is a SEPARATE wallet from perp collateral, so
    // total/used/free stay perp-only exactly as before. But when the account
    // opted into spot we still surface the holdings, or the margin panel's
    // Spot tab renders zeros for balances the user demonstrably has.
    const spotSnapshot =
      !this.shouldUseSpotCollateralBalance() && this.hasSpotCollateralSnapshot
        ? this.buildUnifiedSpotCollateralSnapshot(this.cachedSpotCollateralState.balances)
        : null;

    this.store.update({
      balance: {
        total: accountValue - upnl,
        used,
        free: accountValue - used,
        upnl,
        ...(spotSnapshot ? { coins: spotSnapshot.coins } : {}),
        info: {
          userAbstractionMode: this.userAbstractionMode,
          source: spotSnapshot ? 'clearinghouseState+spotHoldings' : 'clearinghouseState',
          ...(spotSnapshot ? { spotCollateralCurrencies: spotSnapshot.spotCurrencies } : {}),
        },
      },
    });
  };

  public applyCorePerpsBalanceSnapshot = (snapshot: {
    accountValue: number;
    totalMarginUsed: number;
    upnl: number;
  }): void => {
    const accountValue = Number(snapshot.accountValue);
    const totalMarginUsed = Number(snapshot.totalMarginUsed);
    const upnl = Number(snapshot.upnl);
    if (!Number.isFinite(accountValue) || !Number.isFinite(totalMarginUsed) || !Number.isFinite(upnl)) {
      return;
    }

    this.latestCorePerpsAccountValue = accountValue;
    this.latestCorePerpsUsedMargin = totalMarginUsed;
    this.latestCorePerpsUpnl = upnl;
    this.applyBalanceFromCurrentMode();
  };

  public handleSpotClearinghouseStateUpdate = (
    state: SpotClearinghouseState,
    source: 'rest' | 'websocket' | 'legacy' = 'legacy',
  ): void => {
    // Spot-opted-in standard accounts also ingest this — they use it for
    // holdings only, not collateral (see `applyBalanceFromCurrentMode`).
    if (!this.shouldTrackSpotHoldings()) {
      return;
    }

    if (!Array.isArray(state?.balances)) {
      return;
    }

    // REST is a startup/reconnect fallback. Once the live spot stream has
    // produced an authoritative snapshot, an older in-flight REST response must
    // not overwrite it. A reconnect delivers a fresh spotState snapshot itself.
    if (source === 'rest' && this.hasAuthoritativeUnifiedSpotWsSnapshot) {
      return;
    }

    this.hasSpotCollateralSnapshot = true;
    this.cachedSpotCollateralState = {
      balances: state.balances.filter((entry) => typeof entry?.coin === 'string' && entry.coin.length > 0),
      tokenToAvailableAfterMaintenance: state.tokenToAvailableAfterMaintenance,
    };
    const authoritativeAvailable = this.getAuthoritativeUnifiedUsdcAvailability(state);
    if (authoritativeAvailable !== null) {
      if (source === 'websocket') {
        this.hasAuthoritativeUnifiedSpotWsSnapshot = true;
      }
      const spotSnapshot = this.buildUnifiedSpotCollateralSnapshot(this.cachedSpotCollateralState.balances);
      // Unified spot totals already include live perp PnL. Snapshot the wallet
      // principal and used amount alongside authoritative availability so a later
      // field-less REST response or ticker-only PnL update cannot mix block states.
      this.latestAuthoritativeUnifiedBalanceSnapshot = {
        total: spotSnapshot.total - this.latestCorePerpsUpnl,
        used: spotSnapshot.total - authoritativeAvailable,
        free: authoritativeAvailable,
      };
    }
    this.applyBalanceFromCurrentMode();
  };

  public requestUnifiedSpotCollateralRefresh = (force = false): void => {
    void this.refreshUnifiedSpotCollateralBalance(force);
  };

  private refreshUnifiedSpotCollateralBalance = async (force = false): Promise<void> => {
    if (!this.shouldTrackSpotHoldings()) {
      return;
    }

    const user = this.options.vault || this.options.applicationId;
    if (!user) {
      return;
    }

    const now = Date.now();
    if (!force && now - this.lastSpotCollateralRefreshAt < HyperliquidExchange.SPOT_COLLATERAL_REFRESH_INTERVAL_MS) {
      return;
    }

    if (this.spotCollateralRefreshInFlight) {
      return this.spotCollateralRefreshInFlight;
    }

    this.lastSpotCollateralRefreshAt = now;
    this.spotCollateralRefreshInFlight = (async () => {
      try {
        const { data } = await this.infoXhr.post<SpotClearinghouseState>(
          ENDPOINTS.INFO,
          {
            type: INFO_TYPES.SPOT_CLEARINGHOUSE_STATE,
            user,
          },
          {
            headers: {
              'Content-Type': 'application/json',
            },
          },
        );

        if (this.isDisposed) return;
        this.handleSpotClearinghouseStateUpdate(data || { balances: [] }, 'rest');
      } catch (error) {
        this.log(`Unified spot collateral refresh failed: ${error}`, LogSeverity.Warning);
      } finally {
        this.spotCollateralRefreshInFlight = null;
      }
    })();

    return this.spotCollateralRefreshInFlight;
  };

  validateAccount = async () => {
    try {
      if (!validateWalletAndPrivateKey(this.options.secret, this.options.key)) {
        throw new Error('Invalid private key & wallet address pair');
      }

      return { error: '' };
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };
    }
  };

  /**
   * Fetch and cache builder fee approval status from Hyperliquid API.
   * Only checks if we have an applicationId (main wallet address for wallet-based connections).
   */
  private fetchBuilderFeeApproval = async (): Promise<void> => {
    // Only check for wallet-based connections that have applicationId
    if (!this.options.applicationId) {
      this._builderFeeApproved = false;
      this._builderFeeLastCheck = { approved: false, error: 'missing applicationId' };
      return;
    }

    try {
      const builder = resolveHyperliquidBuilder(this.options);
      const result = await this.withStartupTimeout(
        checkBuilderFeeApproved(
          this.options.applicationId,
          builder.address,
          this.options.testnet ?? false,
          builder.feeTenthsBps,
        ),
        HyperliquidExchange.BUILDER_FEE_APPROVAL_TIMEOUT_MS,
        'Builder fee approval check',
        { approved: false },
      );
      this._builderFeeApproved = result.approved;
      this._builderFeeLastCheck = result;
      this.log(
        `Builder fee approval status: ${result.approved ? 'approved' : 'not approved'}`,
        result.approved ? LogSeverity.Info : LogSeverity.Debug,
      );
    } catch (err) {
      this.log(`Failed to check builder fee approval: ${err}`, LogSeverity.Warning);
      this._builderFeeApproved = null; // Keep as unknown on error
      this._builderFeeLastCheck = {
        approved: false,
        error: err instanceof Error ? err.message : String(err),
      };
    }
  };

  /**
   * Extract a usable balance summary from clearinghouseState payload.
   * Hyperliquid responses can include either marginSummary or crossMarginSummary
   * depending on source/version.
   */
  private extractClearinghouseBalanceSummary = (
    data: any,
  ): { accountValue: number; totalMarginUsed: number; sourceField: 'marginSummary' | 'crossMarginSummary' } | null => {
    const sourceField = data?.marginSummary ? 'marginSummary' : data?.crossMarginSummary ? 'crossMarginSummary' : null;
    if (!sourceField) {
      return null;
    }

    const summary = data[sourceField];
    const accountValue = Number(summary?.accountValue);
    const totalMarginUsed = Number(summary?.totalMarginUsed);

    if (!Number.isFinite(accountValue) || !Number.isFinite(totalMarginUsed)) {
      return null;
    }

    return {
      accountValue,
      totalMarginUsed,
      sourceField,
    };
  };

  /**
   * Resolve a missing applicationId/vault from `options.key` via the `userRole` info endpoint.
   * Hyperliquid info queries (clearinghouseState, orders, positions) are keyed off the master
   * wallet, so without applicationId we can't load any private state. For accounts saved before
   * the agent-wallet flow persisted applicationId, infer it from the wallet address.
   *
   * - role 'user'        → the key IS a master wallet, applicationId = key
   * - role 'agent'       → key is an agent, master is data.user
   * - role 'subAccount'  → key is a subaccount, master is data.master
   * - role 'vault'       → key is a vault wallet, route via options.vault instead
   * - role 'missing'     → unknown wallet on Hyperliquid, leave unresolved
   */
  private resolveApplicationIdFromKey = async (): Promise<void> => {
    if (this.options.applicationId || this.options.vault) return;
    if (!this.options.key) return;

    const requestBody = {
      type: INFO_TYPES.USER_ROLE,
      user: this.options.key,
    };
    const requestConfig = {
      headers: { 'Content-Type': 'application/json' },
    };

    const fetchRole = async () => {
      try {
        const { data } = await this.infoXhr.post(ENDPOINTS.INFO, requestBody, requestConfig);
        return data;
      } catch (error) {
        this.log(`userRole resolution failed: ${error}`, LogSeverity.Warning);
        return null;
      }
    };

    const data = await this.withStartupTimeout(
      fetchRole(),
      HyperliquidExchange.USER_ROLE_RESOLVE_TIMEOUT_MS,
      'userRole resolution',
      null as any,
    );

    if (!data) return;

    // ExchangeOptions is Readonly; intentional in-place patch so downstream reads see the resolved value.
    const opts = this.options as { applicationId?: string; vault?: string; key?: string };
    const role = data?.role;
    switch (role) {
      case 'user':
        opts.applicationId = this.options.key;
        this.log(`Resolved applicationId from userRole=user (key is master wallet)`, LogSeverity.Info);
        return;
      case 'agent': {
        const master = data?.data?.user;
        if (master) {
          opts.applicationId = master;
          this.log(`Resolved applicationId from userRole=agent: master=${master}`, LogSeverity.Info);
        } else {
          this.emitAndLogError(
            `Hyperliquid recognises this wallet as an agent but did not return its master address. Please re-add the account.`,
          );
        }
        return;
      }
      case 'subAccount': {
        const master = data?.data?.master;
        if (master) {
          opts.applicationId = master;
          this.log(`Resolved applicationId from userRole=subAccount: master=${master}`, LogSeverity.Info);
        } else {
          this.emitAndLogError(
            `Hyperliquid recognises this wallet as a subaccount but did not return its master address. Please re-add the account.`,
          );
        }
        return;
      }
      case 'vault':
        opts.vault = this.options.key;
        this.log(`Resolved vault from userRole=vault (key is vault wallet)`, LogSeverity.Info);
        return;
      case 'missing':
        this.emitAndLogError(
          `Hyperliquid does not recognise wallet ${this.options.key}. The wallet has no deposits, agent approvals, or subaccount linkage on this network. Please re-add the account using the correct wallet address (and main wallet if this is an agent key).`,
        );
        return;
      default:
        this.log(`userRole returned unexpected role=${role}; leaving applicationId unresolved`, LogSeverity.Warning);
        return;
    }
  };

  /**
   * Fetch a complete core-perp account snapshot from REST. It seeds both
   * balance and positions at startup and reconciles missed state on reconnect.
   */
  private fetchCorePerpsState = async (): Promise<any | null> => {
    const user = this.options.vault || this.options.applicationId;
    if (!user) {
      this.log('Skipping core-perp account-state fetch: no applicationId/vault configured', LogSeverity.Warning);
      return null;
    }

    const requestBody = {
      type: INFO_TYPES.PERPS_CLEARINGHOUSE_STATE,
      user,
    };
    const requestConfig = {
      headers: {
        'Content-Type': 'application/json',
      },
    };
    const logBalanceSummary = (data: any): void => {
      const summary = this.extractClearinghouseBalanceSummary(data);
      if (!summary) {
        this.log(
          `No usable balance summary. marginSummary=${Boolean(data?.marginSummary)} crossMarginSummary=${Boolean(data?.crossMarginSummary)}`,
          LogSeverity.Warning,
        );
        return;
      }

      const used = summary.totalMarginUsed;
      const free = summary.accountValue - used;

      this.log(
        `Core-perp account state fetched (${summary.sourceField}): total=${summary.accountValue.toFixed(2)}, used=${used.toFixed(2)}, free=${free.toFixed(2)}`,
        LogSeverity.Info,
      );
    };

    try {
      this.log('Fetching core-perp account state via REST API', LogSeverity.Debug);

      const { data } = await this.infoXhr.post(ENDPOINTS.INFO, requestBody, requestConfig);
      logBalanceSummary(data);
      return data;
    } catch (error) {
      this.log(`Core-perp account-state fetch failed: ${error}`, LogSeverity.Warning);
      return null;
    }
  };

  /**
   * Get builder info for orders if the user has approved builder fee.
   * If approval status is unknown, fetches it first.
   */
  private getBuilderInfo = async (): Promise<BuilderInfo | undefined> => {
    // Only applicable for wallet-based connections
    if (!this.options.applicationId) {
      return undefined;
    }

    // If status is unknown, fetch it
    if (this._builderFeeApproved === null) {
      await this.fetchBuilderFeeApproval();
    }

    // Return builder info only if approved. Address and rate come from ONE
    // resolve — pairing an address with another tier's rate is an order the
    // venue rejects, because the user's approval is per (builder, max rate).
    if (this._builderFeeApproved) {
      const builder = resolveHyperliquidBuilder(this.options);
      return {
        b: builder.address.toLowerCase(),
        f: builder.feeTenthsBps,
      };
    }

    return undefined;
  };

  public getOrderErrorTelemetry = (requestData: unknown, message: string): Record<string, unknown> | undefined => {
    if (!message.toLowerCase().includes('builder fee')) return undefined;

    const payload = parseHyperliquidExchangePayload(requestData);
    const action = payload?.action && typeof payload.action === 'object' ? payload.action : undefined;
    const builder =
      action?.builder && typeof action.builder === 'object' ? (action.builder as Partial<BuilderInfo>) : undefined;
    const lastCheck = this._builderFeeLastCheck;
    const maxFeeTenths = lastCheck?.maxFeeRateTenthsBps;
    const configuredBuilder = resolveHyperliquidBuilder(this.options);
    const feeSufficient =
      typeof maxFeeTenths === 'number' ? (maxFeeTenths >= configuredBuilder.feeTenthsBps ? 'yes' : 'no') : 'unknown';
    const builderCheckStatus =
      this._builderFeeApproved === true ? 'approved' : this._builderFeeApproved === false ? 'not_approved' : 'unknown';

    return {
      sentryTags: {
        hyperliquid_error_family: 'builder_fee',
        hyperliquid_builder_attached: builder ? 'yes' : 'no',
        hyperliquid_builder_check: builderCheckStatus,
        hyperliquid_builder_fee_sufficient: feeSufficient,
        hyperliquid_has_application_id: this.options.applicationId ? 'yes' : 'no',
        hyperliquid_has_vault: this.options.vault ? 'yes' : 'no',
        hyperliquid_network: this.options.testnet ? 'testnet' : 'mainnet',
      },
      sentryExtra: {
        hyperliquidBuilderFee: {
          actionType: typeof action?.type === 'string' ? action.type : undefined,
          grouping: typeof action?.grouping === 'string' ? action.grouping : undefined,
          orderCount: Array.isArray(action?.orders) ? action.orders.length : undefined,
          builderPayloadPresent: Boolean(builder),
          attachedBuilderAddress: maskHyperliquidAddress(builder?.b),
          attachedBuilderFeeTenthsBps: builder?.f,
          expectedBuilderAddress: maskHyperliquidAddress(configuredBuilder.address),
          requiredBuilderFeeTenthsBps: configuredBuilder.feeTenthsBps,
          checkedMainWallet: maskHyperliquidAddress(this.options.applicationId),
          signerWallet: maskHyperliquidAddress(this.options.key),
          vaultAddress: maskHyperliquidAddress(this.options.vault),
          lastCheckApproved: lastCheck?.approved,
          lastCheckMaxFeeRate: lastCheck?.maxFeeRate,
          lastCheckMaxFeeTenthsBps: maxFeeTenths,
          lastCheckError: lastCheck?.error,
          publicDataOnly: this.options.publicDataOnly,
        },
      },
    };
  };

  /**
   * Hyperliquid reads are unauthenticated — `/info` takes the wallet address as
   * a parameter — so a saved account whose private key no longer matches its API
   * wallet loads looking perfectly healthy and only fails when the user tries to
   * trade. `validateAccount` catches this, but it only runs on the add-key form,
   * never for an account restored from storage. Warn once at startup instead of
   * letting the first rejected order be the notification.
   *
   * Deliberately non-fatal: public data and read-only use still work, and the
   * pair can legitimately be unset for wallet-based connections.
   */
  private warnOnMismatchedApiWallet = (explicitPublicDataOnly: boolean) => {
    if (explicitPublicDataOnly) return;
    const { applicationId, key, secret, vault } = this.options;
    const wallet = key || vault || applicationId;
    if (!wallet || !secret) return;
    // A malformed secret makes ethers throw rather than return false. That is
    // still invalid credentials, and startup must never fail on the diagnostic.
    try {
      if (validateWalletAndPrivateKey(secret, wallet)) return;
    } catch {
      // fall through to the warning
    }

    this.emitAndLogError(
      `Hyperliquid credentials are invalid: the saved private key does not match API wallet ${wallet}. ` +
        `Orders will be rejected until the account is re-added.`,
    );
  };

  start = async () => {
    const useExternalPublicData = await this.initExternalPublicData();
    const explicitPublicDataOnly = this.options.publicDataOnly === true;

    // Older saved accounts may be missing applicationId; infer it from the key via userRole
    // before deciding whether to start private streams.
    if (!explicitPublicDataOnly && !this.hasPrivateSessionIdentity && this.options.key) {
      await this.resolveApplicationIdFromKey();
      if (this.isDisposed) return;
    }

    this.warnOnMismatchedApiWallet(explicitPublicDataOnly);

    const shouldStartPrivateData = !explicitPublicDataOnly && this.hasPrivateSessionIdentity;
    const shouldMarkPrivateLoaded = explicitPublicDataOnly || shouldStartPrivateData;

    if (!shouldStartPrivateData && !explicitPublicDataOnly) {
      this.log(`Skipping private startup: no applicationId/vault configured`, LogSeverity.Warning);
    }

    let markets: Market[] = [];
    let tickers: Ticker[] = [];

    const userAbstractionModePromise = shouldStartPrivateData
      ? this.resolveUserAbstractionMode().catch((error) => {
          this.log(`userAbstraction resolution failed: ${error}`, LogSeverity.Warning);
        })
      : null;

    // Keep private startup tasks non-blocking so public market data can hydrate immediately
    // even when account-level REST calls are delayed by route congestion/retries.
    const initialCorePerpsStatePromise = shouldStartPrivateData
      ? this.fetchCorePerpsState().catch((error) => {
          this.log(`Initial core-perp account-state fetch failed: ${error}`, LogSeverity.Warning);
          return null;
        })
      : null;

    if (shouldStartPrivateData) {
      this.fetchBuilderFeeApproval().catch((error) => {
        this.log(`Builder fee approval check failed: ${error}`, LogSeverity.Warning);
      });
    }

    if (!useExternalPublicData) {
      // Fetch public bootstrap data first so UI can become responsive quickly.
      const marketsAndTickers = await this.fetchMarketsAndTickers();
      if (this.isDisposed) return;

      markets = marketsAndTickers[0];
      tickers = marketsAndTickers[1];

      // Start websocket streams early - they can establish connections while we process data
      this.publicWebsocket.connectAndSubscribe();
    } else {
      markets = this.store.markets;
      tickers = this.store.tickers;
    }

    // Build case-insensitive lookup map for O(1) performance in both direct and external-public-data modes.
    this.buildMarketsLowerCaseMap(markets);

    // Generate positions directly from fetched markets (no store dependency)
    const positions = this.generateFakePositionsFromMarkets(markets);

    // Update store with all initial data in a single call
    // Use default zero balance at startup (REST/WS private streams patch real values asynchronously).
    const storeUpdate: any = {
      positions,
      orders: [],
      balance: {
        used: 0,
        free: 0,
        total: 0,
        upnl: 0,
      },
      loaded: {
        ...this.store.loaded,
        orders: shouldMarkPrivateLoaded,
        balance: shouldMarkPrivateLoaded,
        positions: shouldMarkPrivateLoaded,
      },
    };

    if (!useExternalPublicData) {
      storeUpdate.markets = markets;
      storeUpdate.tickers = tickers;
      storeUpdate.loaded.markets = true;
      storeUpdate.loaded.tickers = true;
    }

    this.store.update(storeUpdate);

    if (shouldStartPrivateData) {
      await userAbstractionModePromise;
      if (this.isDisposed) return;

      this.privateWebsocket.connectAndSubscribe();
      if (this.shouldTrackSpotHoldings()) {
        this.requestUnifiedSpotCollateralRefresh(true);
      }
    }

    // The REST response is a complete core-perp snapshot, so it also covers
    // existing positions if the first private WebSocket frame is delayed.
    if (initialCorePerpsStatePromise) {
      initialCorePerpsStatePromise.then((fetchedState) => {
        if (this.isDisposed || !fetchedState) return;
        this.privateWebsocket.applyCoreClearinghouseState(fetchedState, true);
      });
    }

    this.log(`Ready to watch public data on Hyperliquid`);

    if (this.isPublicDataOnly) return;

    // Fetch initial orders (including HIP-3) after markets are loaded
    // This ensures orders are available before WebSocket starts streaming updates
    this._lastOrderRefreshTime = Date.now(); // Prevent immediate re-fetch in tick
    this.refreshOrders().catch((error) => {
      this.log(`Failed to fetch initial orders: ${error}`, LogSeverity.Warning);
    });

    // Fetch HIP-3 positions asynchronously (fire and forget)
    // This ensures users see their existing HIP-3 positions on startup
    // Prevent immediate duplicate periodic sweep on the first tick.
    this._lastPositionRefreshTime = Date.now();
    this.fetchAllHip3Positions().catch((error) => {
      this.log(`Failed to fetch initial HIP-3 positions: ${error}`, LogSeverity.Warning);
    });

    await this.tick();
    if (this.isDisposed) return;
  };

  fetchAndSetPrivateSnapshot = async (_tries = 1): Promise<void> => {
    if (this.isPublicDataOnly || this.isDisposed) return;

    // `fetchOrders()` already handles its own API failures and in-flight
    // coalescing, so keep the reconnect snapshot parallel and bounded rather
    // than inheriting the generic order-only retry loop.
    const [orders, coreState] = await Promise.all([this.fetchOrders(), this.fetchCorePerpsState()]);
    if (this.isDisposed) return;

    this.store.update({
      orders,
      loaded: { ...this.store.loaded, orders: true },
    });
    if (coreState) this.privateWebsocket.applyCoreClearinghouseState(coreState, true);
  };

  // Periodic HIP-3 position refresh interval (120 seconds) as a safety net
  // The primary path for position updates is now WS clearinghouseState with dex parameter.
  // This REST refresh is only a safety net for edge cases where WS data is missed.
  private readonly HIP3_POSITION_REFRESH_INTERVAL_MS = 120_000;
  private _lastPositionRefreshTime = 0;
  private readonly HIP3_POSITION_REFRESH_DEBOUNCE_MS = 350;
  private _hip3PositionRefreshDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  private _pendingHip3PositionRefreshDexes = new Set<string>();

  // Debounced order refresh to prevent multiple rapid calls from position closures
  private _orderRefreshPending = false;
  private _orderRefreshDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  // Collapse overlapping order REST refreshes into a single in-flight request.
  private _fetchOrdersInFlight: Promise<Order[]> | null = null;

  /**
   * Schedule an order refresh with debounce (500ms).
   * Multiple calls within the debounce window are coalesced into one.
   */
  public scheduleOrderRefresh = (): void => {
    if (this._orderRefreshPending) return; // Already scheduled

    this._orderRefreshPending = true;
    if (this._orderRefreshDebounceTimer) {
      clearTimeout(this._orderRefreshDebounceTimer);
    }

    this._orderRefreshDebounceTimer = setTimeout(() => {
      this._orderRefreshPending = false;
      this._orderRefreshDebounceTimer = null;

      // Only refresh if enough time has passed since last refresh
      const now = Date.now();
      if (now - this._lastOrderRefreshTime > this.HIP3_ORDER_REFRESH_INTERVAL_MS) {
        this._lastOrderRefreshTime = now;
        this.refreshOrders().catch((error) => {
          this.log(`Debounced order refresh failed: ${error}`, LogSeverity.Warning);
        });
      }
    }, 500);
  };

  tick = async () => {
    if (!this.isDisposed) {
      if (this.isPublicDataOnly) return;

      // Only refresh HIP-3 orders when:
      // 1. We have HIP-3 DEXes configured
      // 2. A new order was detected via WS that we couldn't find in store (cross-device scenario)
      // 3. Enough time has passed since last refresh (rate limit protection)
      const now = Date.now();
      if (
        this._hip3OrdersNeedRefresh &&
        this.hip3DexNames.length > 0 &&
        now - this._lastOrderRefreshTime > this.HIP3_ORDER_REFRESH_INTERVAL_MS
      ) {
        this._hip3OrdersNeedRefresh = false;
        this._lastOrderRefreshTime = now;
        this.log(`Refreshing HIP-3 orders (new order detected via WS)`, LogSeverity.Debug);
        this.refreshOrders().catch((error) => {
          this.log(`HIP-3 order refresh failed: ${error}`, LogSeverity.Warning);
        });
      }

      // Periodic HIP-3 position refresh as a safety net
      // This ensures positions stay in sync even if fills are missed or deduplicated
      if (
        this.hip3DexNames.length > 0 &&
        now - this._lastPositionRefreshTime > this.HIP3_POSITION_REFRESH_INTERVAL_MS
      ) {
        this._lastPositionRefreshTime = now;
        this.log(`Periodic HIP-3 position refresh`, LogSeverity.Debug);
        this.fetchAllHip3Positions().catch((error) => {
          this.log(`Periodic HIP-3 position refresh failed: ${error}`, LogSeverity.Warning);
        });
      }

      if (!this.isDisposed) {
        loop(() => this.tick(), this.getTickInterval());
      }
    }
  };

  dispose = () => {
    this._fetchOrdersInFlight = null;
    this._placedLimitOrderTif.clear();

    // Clear any pending debounced order refresh
    if (this._orderRefreshDebounceTimer) {
      clearTimeout(this._orderRefreshDebounceTimer);
      this._orderRefreshDebounceTimer = null;
    }

    // Clear any pending debounced HIP-3 position refresh
    if (this._hip3PositionRefreshDebounceTimer) {
      clearTimeout(this._hip3PositionRefreshDebounceTimer);
      this._hip3PositionRefreshDebounceTimer = null;
    }
    this._pendingHip3PositionRefreshDexes.clear();

    // Clear any active retry chain timer
    if (this._retryChainTimer) {
      clearTimeout(this._retryChainTimer);
      this._retryChainTimer = null;
    }
    this.endRetryChain();

    // Dispose WebSockets safely
    this.publicWebsocket?.dispose();
    this.privateWebsocket?.dispose();

    // Call parent dispose
    super.dispose();
  };

  private fetchFee = async (): Promise<{ takerFee: number; makerFee: number } | null> => {
    const user = this.options.vault || this.options.applicationId;
    if (this.isPublicDataOnly || !user) {
      return { takerFee: 0.00045, makerFee: 0.00015 };
    }
    try {
      const { data } = await this.infoXhr.post(
        ENDPOINTS.INFO,
        {
          type: 'userFees',
          user,
        },
        {
          headers: {
            'Content-Type': 'application/json',
          },
        },
      );

      if (!data) {
        return {
          takerFee: 0.00045,
          makerFee: 0.00015,
        };
      }

      const crossRate = parseFloat(data.userCrossRate || data.crossRate || '0.00045');
      const addRate = parseFloat(data.userAddRate || data.addRate || '0.00015');

      return {
        takerFee: crossRate,
        makerFee: addRate,
      };
    } catch (error) {
      this.log(`Failed to fetch user fees, using defaults: ${error}`, LogSeverity.Warning);
      return {
        takerFee: 0.00045,
        makerFee: 0.00015,
      };
    }
  };

  private fetchMarketsHelper = (
    meta: Meta,
    assets: AssetCtx[],
    fees: { takerFee: number; makerFee: number } | null,
  ): { markets: Market[]; tickers: Ticker[] } => {
    const markets: Market[] = [];
    const tickers: Ticker[] = [];

    for (let i = 0; i < meta.universe.length; i++) {
      const sourceMarket = meta.universe[i];
      const sourceTicker = assets[i];

      const baseAsset = v(sourceMarket, 'name');
      const quoteAsset = 'USDC';

      const amountPrecisionStr = safeString(sourceMarket, 'szDecimals');
      const amountPrecision = parseInt(amountPrecisionStr as string);

      const price = parseFloat(v(sourceTicker, 'midPx'));
      // deprecated ticker skip
      if (!sourceTicker || !sourceTicker['midPx']) {
        continue;
      }
      try {
        const pricePrecision = calculatePricePrecision(price, amountPrecision, 6);
        const pricePrecisionStr = numberToString(pricePrecision);

        const id = baseAsset;
        const symbol = `${sourceMarket.name.toUpperCase()}-PERP`;

        markets.push({
          id,
          symbol,
          base: baseAsset,
          quote: quoteAsset,
          active: true,
          precision: {
            amount: +(parsePrecision(amountPrecisionStr) || 0),
            price: +(parsePrecision(pricePrecisionStr) || 0),
          },
          limits: {
            amount: {
              min: +(parsePrecision(amountPrecisionStr) || 0),
              max: Number.MAX_SAFE_INTEGER,
            },
            leverage: {
              min: 1,
              max: v(sourceMarket, 'maxLeverage') || 1,
            },
          },
          takerFee: fees?.takerFee,
          makerFee: fees?.makerFee,
          info: {
            hyperliquidId: i,
            ...sourceMarket,
          },
        });

        // Create ticker in the same loop (price is already parsed as midPx above)
        const mark = parseFiniteNumber(v(sourceTicker, 'markPx'));
        const prevDayPx = parseFiniteNumber(v(sourceTicker, 'prevDayPx'));
        const notionalPrice = mark > 0 ? mark : price;
        const { volume, quoteVolume } = parseHyperliquidVolumeFields(sourceTicker, price);

        tickers.push({
          id,
          symbol,
          bid: 0,
          ask: 0,
          last: price,
          mark,
          index: parseFiniteNumber(v(sourceTicker, 'oraclePx')),
          midPrice: price,
          percentage: calculatePriceChangePercentage(notionalPrice, prevDayPx),
          fundingRate: parseFiniteNumber(v(sourceTicker, 'funding')),
          fundingTime: 0,
          volume,
          quoteVolume,
          openInterest: parseHyperliquidOpenInterestNotional(sourceTicker, notionalPrice),
          info: sourceTicker,
        });
      } catch (e) {
        this.log(`Error in fetchMarketsHelper: ${e}`, LogSeverity.Error);
      }
    }

    return { markets, tickers };
  };

  // HIP-3 DEXes to fetch on testnet (reduces API calls from 60+ to just these)
  private static readonly TESTNET_HIP3_WHITELIST = ['vntls', 'xyz', 'VOLMEX', 'flx'];

  // Fetch available HIP-3 DEXes (builder-deployed perpetuals)
  // Returns DEXes with their original position in the perpDexs array (needed for asset index offset)
  private fetchHip3Dexs = async (): Promise<Array<PerpDex & { dexPosition: number }>> => {
    try {
      const { data } = await this.xhr.post<PerpDexs>(
        ENDPOINTS.INFO,
        { type: INFO_TYPES.PERP_DEXS },
        { headers: { 'Content-Type': 'application/json' } },
      );
      // First element is null (core HL perps), map with original index then filter
      let dexsWithPosition = data
        .map((dex, index) => (dex ? { ...dex, dexPosition: index } : null))
        .filter((dex): dex is PerpDex & { dexPosition: number } => dex !== null);

      // On testnet, only fetch whitelisted DEXes to reduce API calls
      if (this.options.testnet) {
        dexsWithPosition = dexsWithPosition.filter((dex) =>
          HyperliquidExchange.TESTNET_HIP3_WHITELIST.includes(dex.name),
        );
      }

      return dexsWithPosition;
    } catch (error) {
      this.log(`Failed to fetch HIP-3 DEXes: ${error}`, LogSeverity.Warning);
      return [];
    }
  };

  // Fetch markets for a specific HIP-3 DEX
  // dexPosition is the 1-indexed position in the perpDexs array (used for asset index offset)
  private fetchHip3Markets = async (
    dexName: string,
    dexPosition: number,
    fees: { takerFee: number; makerFee: number } | null,
    // `null` means THIS DEX FAILED, which an empty market list cannot express: a dex
    // that answered with nothing and a dex whose fetch died both used to return `[]`,
    // so a broken dex counted as successfully loaded and contributed silence.
  ): Promise<{ markets: Market[]; tickers: Ticker[] } | null> => {
    const parsedResponse = await fetchHip3MetaAndAssetCtxsWithRetry({
      xhr: this.xhr,
      dexName,
      log: (message, severity = LogSeverity.Info) => this.log(message, severity),
    });
    if (!parsedResponse) {
      return null;
    }

    try {
      const [meta, assets] = parsedResponse;
      const markets: Market[] = [];
      const tickers: Ticker[] = [];

      // HIP-3 asset indices use offset to avoid collision with core perps
      // Formula from CCXT: 110000 + (dexPosition - 1) * 10000
      const assetIndexOffset = 110000 + (dexPosition - 1) * 10000;

      for (let i = 0; i < meta.universe.length; i++) {
        const sourceMarket = meta.universe[i];
        const sourceTicker = assets[i];

        // Skip delisted or inactive markets
        if (sourceMarket.isDelisted || !sourceTicker || !sourceTicker['midPx']) {
          continue;
        }

        // HIP-3 market names from API may or may not have dex prefix
        const rawName = v(sourceMarket, 'name');
        const baseAsset = rawName.includes(':') ? rawName.split(':')[1] : rawName;

        const amountPrecisionStr = safeString(sourceMarket, 'szDecimals');
        const amountPrecision = parseInt(amountPrecisionStr as string);
        const price = parseFloat(v(sourceTicker, 'midPx'));

        try {
          const pricePrecision = calculatePricePrecision(price, amountPrecision, 6);
          const pricePrecisionStr = numberToString(pricePrecision);

          // ID must include dex prefix (e.g., "vntls:vKRAKEN") to match order coin names
          const id = rawName.includes(':') ? rawName : `${dexName}:${rawName}`;
          // Symbol format: {DEX}|{BASE}-PERP (e.g., "VNTLS|VKRAKEN-PERP") - use pipe to avoid TradingView colon conflicts
          const symbol = `${dexName.toUpperCase()}|${baseAsset.toUpperCase()}-PERP`;

          markets.push({
            id,
            symbol,
            base: baseAsset,
            quote: 'USDC',
            active: true,
            precision: {
              amount: +(parsePrecision(amountPrecisionStr) || 0),
              price: +(parsePrecision(pricePrecisionStr) || 0),
            },
            limits: {
              amount: {
                min: +(parsePrecision(amountPrecisionStr) || 0),
                max: Number.MAX_SAFE_INTEGER,
              },
              leverage: {
                min: 1,
                max: v(sourceMarket, 'maxLeverage') || 1,
              },
            },
            takerFee: fees?.takerFee,
            makerFee: fees?.makerFee,
            info: {
              hyperliquidId: i + assetIndexOffset,
              hip3Dex: dexName,
              ...sourceMarket,
            },
          });

          const mark = parseFiniteNumber(v(sourceTicker, 'markPx'));
          const prevDayPx = parseFiniteNumber(v(sourceTicker, 'prevDayPx'));
          const notionalPrice = mark > 0 ? mark : price;
          const { volume, quoteVolume } = parseHyperliquidVolumeFields(sourceTicker, price);

          tickers.push({
            id,
            symbol,
            bid: 0,
            ask: 0,
            last: price,
            mark,
            index: parseFiniteNumber(v(sourceTicker, 'oraclePx')),
            midPrice: price,
            percentage: calculatePriceChangePercentage(notionalPrice, prevDayPx),
            fundingRate: parseFiniteNumber(v(sourceTicker, 'funding')),
            fundingTime: 0,
            volume,
            quoteVolume,
            openInterest: parseHyperliquidOpenInterestNotional(sourceTicker, notionalPrice),
            info: sourceTicker,
          });
        } catch (e) {
          this.log(`Error processing HIP-3 market ${rawName}: ${e}`, LogSeverity.Error);
        }
      }

      return { markets, tickers };
    } catch (error) {
      this.log(`Failed to fetch HIP-3 markets for ${dexName}: ${error}`, LogSeverity.Warning);
      // `null`, not an empty list, for the same reason as the fetch failure above: a
      // throw while building markets is a FAILED dex, and reporting it as loaded-with-
      // nothing is indistinguishable from a dex whose universe is legitimately empty.
      return null;
    }
  };

  private fetchMarketsAndTickers = async (): Promise<[Market[], Ticker[]]> => {
    const fetchCoreMetaAndAssetCtxs = async (): Promise<[Meta, AssetCtx[]]> => {
      return await fetchCoreMetaAndAssetCtxsWithFallback({
        infoXhr: this.infoXhr,
        xhr: this.xhr,
        log: (message, severity = LogSeverity.Info) => this.log(message, severity),
      });
    };

    // Fetch core startup data in parallel.
    // HIP-3 discovery is best-effort and bounded by a startup timeout to avoid blocking core perps.
    // Spot rides along in the same parallel batch rather than running after the
    // HIP-3 loop — sequencing it there would add its whole timeout budget on
    // top of HIP-3's, lengthening worst-case startup for every account.
    const [coreMetaAndAssets, fees, hip3Dexs, spot] = await Promise.all([
      fetchCoreMetaAndAssetCtxs(),
      this.fetchFee(),
      this.withStartupTimeout(
        this.fetchHip3Dexs(),
        HyperliquidExchange.HIP3_DEX_DISCOVERY_TIMEOUT_MS,
        'HIP-3 DEX discovery',
        [],
      ),
      this.withStartupTimeout<{ markets: Market[]; tickers: Ticker[] }>(
        this.fetchSpotMarketsAndTickers(),
        HyperliquidExchange.SPOT_MARKETS_LOAD_TIMEOUT_MS,
        'spot markets loading',
        { markets: [], tickers: [] },
      ),
    ]);

    const [meta, assets] = coreMetaAndAssets;
    const { markets, tickers } = this.fetchMarketsHelper(meta, assets, fees);

    // Fetch HIP-3 markets with a strict startup budget.
    // Avoid spawning a large parallel burst that keeps the shared rate-limited queue busy
    // long after timeout fallback has already chosen to continue with core perps only.
    if (hip3Dexs.length > 0) {
      const hip3BudgetMs = Math.min(
        HyperliquidExchange.HIP3_MARKETS_LOAD_TIMEOUT_CAP_MS,
        Math.max(
          HyperliquidExchange.HIP3_MARKETS_LOAD_TIMEOUT_MS,
          hip3Dexs.length * HyperliquidExchange.HIP3_PER_DEX_LOAD_BUDGET_MS,
        ),
      );
      const loadedDexNames: string[] = [];
      const failedDexNames: string[] = [];
      const hip3Results: Array<{ markets: Market[]; tickers: Ticker[] }> = [];
      const hip3Deadline = Date.now() + hip3BudgetMs;
      let hip3TimedOut = false;

      for (const dex of hip3Dexs) {
        const remainingBudgetMs = hip3Deadline - Date.now();
        if (remainingBudgetMs <= 0) {
          hip3TimedOut = true;
          break;
        }

        const result = await this.withStartupTimeout<{ markets: Market[]; tickers: Ticker[] } | null | 'timed-out'>(
          this.fetchHip3Markets(dex.name, dex.dexPosition, fees),
          remainingBudgetMs,
          `HIP-3 markets loading (${dex.name})`,
          'timed-out',
        );

        if (result === 'timed-out') {
          hip3TimedOut = true;
          break;
        }

        // One dex's own failure is not the others' problem — keep going.
        if (result === null) {
          failedDexNames.push(dex.name);
          continue;
        }

        hip3Results.push(result);
        loadedDexNames.push(dex.name);
      }

      // KEEP whatever loaded. Discarding complete dexes because a LATER one ran out of
      // budget is what turned a slow load into no HIP-3 markets at all: with 10 dexes
      // against the old flat 8s, seven finished and all seven were thrown away.
      // `hip3DexNames` must list exactly those, since it drives the HIP-3 WS
      // subscriptions and the order/position refresh.
      this.hip3DexNames = loadedDexNames;

      for (const result of hip3Results) {
        markets.push(...result.markets);
        tickers.push(...result.tickers);
      }

      if (loadedDexNames.length > 0) {
        this.log(
          `Loaded ${loadedDexNames.length}/${hip3Dexs.length} HIP-3 DEXes (${loadedDexNames.join(', ')}); ${markets.length} total markets`,
          LogSeverity.Info,
        );
      }

      if (failedDexNames.length > 0) {
        this.log(`HIP-3 DEXes that failed to load: ${failedDexNames.join(', ')}`, LogSeverity.Warning);
      }

      if (hip3TimedOut) {
        const unreached = hip3Dexs
          .map((d) => d.name)
          .filter((name) => !loadedDexNames.includes(name) && !failedDexNames.includes(name));
        this.log(
          `HIP-3 market loading ran out of its ${Math.round(hip3BudgetMs / 1000)}s budget; not loaded: ${unreached.join(', ')}`,
          LogSeverity.Warning,
        );
      }
    }

    // Spot is best-effort and never blocks perps: `fetchSpotMarketsAndTickers`
    // swallows its own errors and the timeout above falls back to empty.
    if (spot.markets.length > 0) {
      markets.push(...spot.markets);
      tickers.push(...spot.tickers);
      this.log(`Loaded ${spot.markets.length} Hyperliquid spot markets`, LogSeverity.Info);
    }

    return [markets, tickers];
  };

  /**
   * Display name for a spot token, matching what Hyperliquid's own UI shows.
   *
   * HyperCore has no native BTC/ETH/SOL — those are bridged by Unit and carry
   * a `U` prefix on-chain (`UBTC`, `UETH`). The API only ever returns the
   * prefixed name; Hyperliquid's frontend strips it for display, which is why
   * their UI reads `ETH/USDC` while `spotMeta` says `UETH`. We mirror that so
   * users see the same market names in both apps.
   *
   * Two guards make this safe rather than a blind prefix strip:
   *
   * - Only tokens whose `fullName` is `Unit <asset>` are considered, so a
   *   token legitimately named `UNI`/`USDC` is never mangled.
   * - The stripped name is rejected if it collides with a real, distinct token.
   *   `UPUMP` (Unit Pump Fun), `UMON` and `UANSEM` each have an unrelated
   *   natively-issued token already using the bare name, so those keep their
   *   prefix. Aliasing them would present two different assets under one name.
   *
   * Display only — `symbol`, `base` and `id` keep the venue's real names, so
   * routing, the order wire, and `marketsBySymbol` are untouched.
   */
  private resolveSpotDisplayName = (token: SpotToken, tokenNames: Set<string>): string => {
    const isUnitBridged = typeof token.fullName === 'string' && token.fullName.startsWith('Unit ');
    if (!isUnitBridged || !/^U[A-Z0-9]/.test(token.name)) return token.name;

    const stripped = token.name.slice(1);
    return stripped && !tokenNames.has(stripped) ? stripped : token.name;
  };

  /**
   * Load Hyperliquid spot pairs into the same store as perps.
   *
   * Safe to merge because the namespaces are disjoint by construction:
   * spot symbols are `TOKEN/{QUOTE}` (perps are `BASE-PERP`, HIP-3 are
   * `DEX|ASSET-PERP`) and spot asset indices are offset by
   * `SPOT_ASSET_INDEX_OFFSET`. Markets carry `isSpot: true`, which is what
   * every downstream consumer (order forms, margin widget, position
   * synthesis) branches on — do not reintroduce a class-level spot flag.
   */
  private fetchSpotMarketsAndTickers = async (): Promise<{ markets: Market[]; tickers: Ticker[] }> => {
    const markets: Market[] = [];
    const tickers: Ticker[] = [];

    // Opt-in per account. Default-off keeps a perp trader's market list and
    // request volume byte-identical to before the merge — spot costs an extra
    // startup REST call and roughly doubles the market count, and most users
    // never touch it. The toggle lives on the account form.
    if (!this.options.enableSpotMarkets) {
      return { markets, tickers };
    }

    try {
      const { data } = await this.infoXhr.post<[SpotMeta, SpotAssetCtx[]]>(
        ENDPOINTS.INFO,
        { type: INFO_TYPES.SPOT_META_AND_ASSET_CTXS },
        { headers: { 'Content-Type': 'application/json' } },
      );

      const [meta, assets] = data ?? [];
      if (!meta?.universe || !Array.isArray(assets)) {
        this.log('Malformed spotMetaAndAssetCtxs response; skipping spot markets', LogSeverity.Warning);
        return { markets, tickers };
      }

      const tokenByIndex: Record<number, SpotToken> = {};
      for (let i = 0; i < meta.tokens.length; i++) {
        tokenByIndex[meta.tokens[i].index] = meta.tokens[i];
      }

      // Join asset contexts by `coin`, NOT positionally. Unlike the perp
      // `metaAndAssetCtxs` (where `assets[i]` lines up with `universe[i]`),
      // the spot response returns more contexts than universe entries
      // (316 universe vs 707 ctxs as of 2026-07-27) and they are not in
      // universe order — a positional join mismatched 245 of 316 pairs in a
      // live check, i.e. it would silently price most spot markets off the
      // wrong asset. Build the index once rather than a linear `find` per pair.
      const assetCtxByCoin: Record<string, SpotAssetCtx> = {};
      for (let i = 0; i < assets.length; i++) {
        const coin = assets[i]?.coin;
        if (coin) assetCtxByCoin[coin] = assets[i];
      }

      const tokenNames = new Set<string>();
      for (let i = 0; i < meta.tokens.length; i++) tokenNames.add(meta.tokens[i].name);

      for (let i = 0; i < meta.universe.length; i++) {
        const sourceMarket = meta.universe[i];
        const token = tokenByIndex[sourceMarket.tokens?.[0]];
        // `tokens` is [base, quote]. The quote is NOT always USDC — a live
        // check found USDH (11), USDT0 (5) and USDE (1) alongside USDC (299).
        // Hardcoding USDC produced FIVE duplicate symbols (`HYPE/USDC` three
        // times, from the real HYPE/USDC plus HYPE/USDH and HYPE/USDT0), and
        // `marketsBySymbol` is symbol-keyed, so the collisions silently
        // dropped markets. Always read the real quote token.
        const quoteToken = tokenByIndex[sourceMarket.tokens?.[1]];
        if (!token || !quoteToken) continue;

        const sourceTicker = assetCtxByCoin[sourceMarket.name];
        // `midPx` is null for delisted/dead pairs — skip rather than
        // publishing a market that can never be priced.
        if (!sourceTicker?.midPx) continue;

        const quoteAsset = quoteToken.name;
        // szDecimals lives on the token, not the universe entry.
        const amountPrecisionStr = safeString(token, 'szDecimals');
        const amountPrecision = parseInt(amountPrecisionStr as string);

        const midPx = parseFloat(v(sourceTicker, 'midPx'));
        const pricePrecision = calculatePricePrecision(midPx, amountPrecision, SPOT_MAX_PRICE_DECIMALS);
        const pricePrecisionStr = numberToString(pricePrecision);

        // `sourceMarket.name` is the API coin handle (`@107`, `PURR/USDC`);
        // the display symbol is built from the token name so it reads as a
        // normal pair. Both stay disjoint from the perp namespace.
        const id = sourceMarket.name;
        const symbol = `${token.name}/${quoteAsset}`;
        // Hyperliquid's UI shows `ETH/USDC` for the `UETH` token. Mirror that
        // on `base` + `prettySymbol` — the two fields every list surface reads
        // — while `symbol` stays the venue-true `UETH/USDC`.
        //
        // `symbol` is deliberately NOT aliased: it is the routing key behind
        // saved favourites, layouts, TradingView symbols and the candle cache,
        // and the alias is data-dependent (it backs off if Hyperliquid ever
        // lists a real `ETH` token). Aliasing it would make every saved
        // reference break the day that happens.
        const displayBase = this.resolveSpotDisplayName(token, tokenNames);
        const displaySymbol = `${displayBase}/${quoteAsset}`;
        const mark = parseFiniteNumber(v(sourceTicker, 'markPx')) || midPx;
        const prevDayPx = parseFiniteNumber(v(sourceTicker, 'prevDayPx'));
        const { volume, quoteVolume } = parseHyperliquidVolumeFields(sourceTicker, midPx);

        markets.push({
          id,
          symbol,
          ...(displaySymbol !== symbol ? { prettySymbol: displaySymbol } : {}),
          base: displayBase,
          quote: quoteAsset,
          isSpot: true,
          active: true,
          precision: {
            amount: +(parsePrecision(amountPrecisionStr) || 0),
            price: +(parsePrecision(pricePrecisionStr) || 0),
          },
          limits: {
            amount: {
              min: +(parsePrecision(amountPrecisionStr) || 0),
              max: Number.MAX_SAFE_INTEGER,
            },
            leverage: { min: 1, max: 1 },
          },
          info: {
            hyperliquidId: sourceMarket.index + SPOT_ASSET_INDEX_OFFSET,
            isSpot: true,
            // The venue's real token identity, preserved because `base` may
            // carry the de-prefixed display alias (`UBTC` -> `BTC`). The UI
            // surfaces these so a user can see that a bridged asset is a claim
            // on Unit's custody, not the native coin.
            hlTokenName: token.name,
            hlTokenFullName: token.fullName ?? undefined,
            ...sourceMarket,
          },
        });

        tickers.push({
          id,
          symbol,
          bid: 0,
          ask: 0,
          last: midPx,
          mark,
          // Spot has no oracle/funding/OI. Zero on the fields below means "not
          // published by the venue", per the Ticker contract — consumers must
          // not read it as a real zero.
          index: 0,
          midPrice: midPx,
          percentage: calculatePriceChangePercentage(mark, prevDayPx),
          fundingRate: 0,
          fundingTime: 0,
          volume,
          quoteVolume,
          openInterest: 0,
          info: sourceTicker,
        });
      }
    } catch (err: any) {
      // Never fail startup over spot — perps must still come up.
      this.log(`Failed to load Hyperliquid spot markets: ${err?.message ?? err}`, LogSeverity.Warning);
      return { markets: [], tickers: [] };
    }

    return { markets, tickers };
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    const interval = opts.interval;
    const limit = Math.min(opts.limit || 500, 1500);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const end = opts.to ? dayjs(opts.to) : dayjs();
    const start =
      !opts.limit && opts.from ? dayjs(opts.from) : end.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const { data } = await this.xhr.post<
      { T: number; t: number; o: string; h: string; l: string; c: string; v: string }[]
    >(
      ENDPOINTS.INFO,
      {
        type: 'candleSnapshot',
        req: {
          // Use market.id for the coin parameter (API requires specific case format)
          // Fallback: strip -PERP suffix and convert to API format
          // - HIP-3: "VNTLS|VKRAKEN-PERP" -> "vntls:vKRAKEN" (lowercase dex, preserve asset case)
          // - Regular: "BTC-PERP" -> "btc" (all lowercase)
          coin: (() => {
            const marketId = this.store.markets?.find?.((market) => market.symbol === opts.symbol)?.id;
            if (marketId) return marketId;

            const symbolWithoutPerp = opts.symbol.replace(/-PERP$/i, '');
            if (symbolWithoutPerp.includes('|')) {
              // HIP-3 market: lowercase dex, preserve asset case
              const [dex, asset] = symbolWithoutPerp.split('|');
              return `${dex.toLowerCase()}:${asset}`;
            }
            // Regular market: all lowercase
            return symbolWithoutPerp.toLowerCase();
          })(),
          interval,
          startTime: start.valueOf(),
          endTime: end.valueOf(),
          // ...(opts.limit
          //   ? {
          //       limit,
          //     }
          //   : {}),
        },
      },
      {
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );

    // Handle different response formats - sometimes the API returns wrapped data
    const candleData = Array.isArray(data) ? data : (data as any)?.data || (data as any)?.response || [];

    if (!Array.isArray(candleData)) {
      this.log(`Unexpected fetchOHLCV response format: ${JSON.stringify(data)}`, LogSeverity.Error);
      return [];
    }

    const candles: Candle[] = candleData.map(({ t, o, h, l, c, v }) => {
      return {
        timestamp: t / 1000,
        open: parseFloat(o),
        high: parseFloat(h),
        low: parseFloat(l),
        close: parseFloat(c),
        volume: parseFloat(v),
      };
    });

    return candles;
  };

  // Default symbol config shared across HIP-3 positions and any core
  // position whose `activeAssetData` hasn't arrived yet. Hoisted as a
  // shared frozen object so we don't allocate a fresh
  // `{ marginType, positionMode, leverage }` on every position
  // mapping. Callers always spread this into a new object
  // (`{ ...config, ... }`) — they don't mutate it — so sharing the
  // reference is safe. `Object.freeze` makes that contract explicit.
  private static readonly DEFAULT_SYMBOL_CONFIG = Object.freeze({
    marginType: MarginType.Cross,
    positionMode: PositionMode.OneWay,
    leverage: 20,
  });

  _getSymbolConfigurationFromAssetData(coin: string) {
    if (this.activeAssetData.has(coin)) {
      const activeAssetData = this.activeAssetData.get(coin);
      return {
        marginType: activeAssetData?.leverage.type === 'isolated' ? MarginType.Isolated : MarginType.Cross,
        // I didn't find any hedged coins tbh
        positionMode: PositionMode.OneWay,
        leverage: activeAssetData?.leverage.value || 20,
      };
    }
    // Sensible defaults when activeAssetData not yet received from WebSocket.
    // Prevents "Leverage not loaded!" errors during the race condition between
    // position creation and WebSocket subscription response.
    return HyperliquidExchange.DEFAULT_SYMBOL_CONFIG;
  }

  /**
   * Keep balance.upnl/free synchronized with live ticker-based position PnL.
   * Core balance totals/margin-used still come from clearinghouseState; this method
   * only refreshes derived fields between balance snapshots.
   */
  public syncBalanceFromLivePositions = (updatedSymbols?: Set<string>): void => {
    if (this.isPublicDataOnly) return;

    const balance = this.store.balance;
    if (!balance) return;

    const walletBalance = Number(balance.total);
    const usedMargin = Number(balance.used);
    if (!Number.isFinite(walletBalance) || !Number.isFinite(usedMargin)) return;

    const positions = this.store.positions;
    let hasOpenPositions = false;
    let hasUpdatedOpenPosition = updatedSymbols === undefined;

    for (const position of positions) {
      const contracts = Number(position.contracts);
      if (!Number.isFinite(contracts) || contracts === 0) continue;
      hasOpenPositions = true;
      if (updatedSymbols && updatedSymbols.has(position.symbol)) {
        hasUpdatedOpenPosition = true;
      }
    }

    if (updatedSymbols && !hasUpdatedOpenPosition) return;
    if (!hasOpenPositions && Number(balance.upnl) === 0) return;

    let nextUpnl = 0;
    for (const position of positions) {
      const contracts = Number(position.contracts);
      if (!Number.isFinite(contracts) || contracts === 0) continue;

      const absContracts = Math.abs(contracts);
      const ticker = this.store.tickersBySymbol[position.symbol];
      const lastPrice = Number(ticker?.last);
      const entryPrice = Number(position.entryPrice);

      if (Number.isFinite(lastPrice) && lastPrice > 0 && Number.isFinite(entryPrice) && entryPrice > 0) {
        nextUpnl +=
          (position.side === PositionSide.Long ? lastPrice - entryPrice : entryPrice - lastPrice) * absContracts;
      } else {
        const fallbackUpnl = Number(position.unrealizedPnl);
        if (Number.isFinite(fallbackUpnl)) {
          nextUpnl += fallbackUpnl;
        }
      }
    }

    const nextFree = walletBalance + nextUpnl - usedMargin;
    const prevUpnl = Number(balance.upnl);
    const prevFree = Number(balance.free);
    const epsilon = 1e-8;

    if (
      Number.isFinite(prevUpnl) &&
      Number.isFinite(prevFree) &&
      Math.abs(prevUpnl - nextUpnl) < epsilon &&
      Math.abs(prevFree - nextFree) < epsilon
    ) {
      return;
    }

    if (this.shouldUseSpotCollateralBalance() && this.hasSpotCollateralSnapshot) {
      this.latestCorePerpsUpnl = nextUpnl;
      this.applyBalanceFromCurrentMode();
      return;
    }

    this.store.update({
      balance: {
        ...balance,
        upnl: nextUpnl,
        free: nextFree,
      },
    });
  };

  /**
   * Validates Hyperliquid API response and extracts order IDs
   * @throws Error if response is invalid or contains errors
   */
  /**
   * POST an order action to `/exchange`, reconciling from REST on any failure.
   *
   * Once the POST is on the wire the placement is UNKNOWN, not failed — the
   * order may be resting on the venue right now. A tunnel timeout, an egress
   * 502 and an unreadable body are indistinguishable from here, and all three
   * reject before the response parser ever runs, so the refresh has to hang off
   * the request rather than off the parse. Without it the user gets a "failed
   * to place" toast for a stop that landed and places it a second time.
   */
  private async postOrderAction(payload: unknown): Promise<any> {
    try {
      const { data } = await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
        headers: {
          'Content-Type': 'application/json',
        },
      });
      return data;
    } catch (error) {
      this.scheduleOrderRefresh();
      throw error;
    }
  }

  private validateResponseAndExtractOrderIds(data: any): string[] {
    try {
      return parseHyperliquidOrderResponse(data).orderIds;
    } catch (error) {
      // Same reasoning as postOrderAction: an unreadable body is a failure we
      // cannot attribute, so reconcile rather than assume nothing landed. The
      // throw still stands — never an accepted empty-id result. A venue
      // rejection is a plain Error and stays out of this branch.
      if (error instanceof HyperliquidUnreadableOrderResponseError) this.scheduleOrderRefresh();
      throw error;
    }
  }

  private responseHasDeferredOrderStatus(data: any): boolean {
    return hyperliquidOrderResponseHasDeferredOrderStatus(data);
  }

  private generateFakePositionsFromMarkets = (markets: Market[]) => {
    // Spot markets are deliberately excluded. Spot exposure is a coin balance
    // (`Balance.coins`), not a Position — synthesizing zero-size spot rows
    // here would pollute the positions table and, worse, feed
    // `DisplayedMarginPostprocessor`, whose long/short exposure sums are
    // defined over derivatives notional only.
    const fakePositions = markets.map((market) => {
      if (market.isSpot) return [];
      const marketConfig = this._getSymbolConfigurationFromAssetData(market.id);
      return marketConfig.positionMode === PositionMode.OneWay
        ? [
            {
              id: createPositionId(market.symbol, PositionSide.Long, PositionMode.OneWay),
              symbol: market.symbol,
              side: PositionSide.Long,
              entryPrice: 0,
              notional: 0,
              unrealizedPnl: 0,
              contracts: 0,
              liquidationPrice: 0,
              ...marketConfig,
            },
          ]
        : [
            {
              id: createPositionId(market.symbol, PositionSide.Long, PositionMode.Hedged),
              symbol: market.symbol,
              side: PositionSide.Long,
              entryPrice: 0,
              notional: 0,
              unrealizedPnl: 0,
              contracts: 0,
              liquidationPrice: 0,
              ...marketConfig,
            },
            {
              id: createPositionId(market.symbol, PositionSide.Short, PositionMode.Hedged),
              symbol: market.symbol,
              side: PositionSide.Short,
              entryPrice: 0,
              notional: 0,
              unrealizedPnl: 0,
              contracts: 0,
              liquidationPrice: 0,
              ...marketConfig,
            },
          ];
    });
    return fakePositions.flat();
  };

  getFakePositions = () => {
    // Return cached fake positions, regenerate only if cache is invalid
    if (!this._fakePositionsCache) {
      this._fakePositionsCache = this.generateFakePositionsFromMarkets(this.store.markets);
    }
    return this._fakePositionsCache;
  };

  invalidateFakePositionsCache = () => {
    this._fakePositionsCache = null;
  };

  fetchPositions = async (symbols?: string[]) => {
    if (this.store.positions.length > 0) {
      return [...this.store.positions];
    }
    const positions = this.getFakePositions();
    if (!symbols || symbols.length === 0) {
      return positions;
    }
    return positions.filter((position) => symbols.includes(position.symbol));
  };

  /**
   * Fetch HIP-3 positions for a specific DEX and update the store.
   * Called when a fill is received for a HIP-3 market since clearinghouseState
   * channels do not include HIP-3 positions.
   */
  fetchHip3Positions = async (dexName: string): Promise<void> => {
    const user = this.options.vault || this.options.applicationId;

    try {
      const { data } = await this.xhr.post<{
        assetPositions: Array<{
          position: {
            coin: string;
            szi: string;
            entryPx: string;
            positionValue: string;
            unrealizedPnl: string;
            liquidationPx: string | null;
            marginUsed?: string;
            leverage: { type: string; value: number };
          };
          type: string;
        }>;
        marginSummary?: {
          accountValue: number;
          totalMarginUsed: number;
        };
      }>(
        ENDPOINTS.INFO,
        { type: INFO_TYPES.PERPS_CLEARINGHOUSE_STATE, user, dex: dexName },
        { headers: { 'Content-Type': 'application/json' } },
      );

      const marketsById = this.store.marketsById;
      const newPositions: Position[] = [];

      // Handle empty positions response - still need to clear existing HIP-3 positions for this dex
      if (!data?.assetPositions || data.assetPositions.length === 0) {
        // Check if we HAD positions for this dex before (meaning position was just closed)
        const hadPositionsForDex = this.store.positions.some((p) => {
          const market = this.store.marketsBySymbol[p.symbol];
          return market?.info?.hip3Dex === dexName;
        });

        // Filter out existing HIP-3 positions for this dex
        const currentPositions = this.store.positions.filter((p) => {
          const market = this.store.marketsBySymbol[p.symbol];
          return market?.info?.hip3Dex !== dexName;
        });

        const allPositions = uniqBy([...currentPositions, ...this.getFakePositions()], 'id');
        this.store.addOrUpdatePositionsFull(allPositions);
        this.log(`Cleared HIP-3 positions for dex: ${dexName} (empty response)`, LogSeverity.Info);

        // If we had positions before and now have none, the position was closed
        // Hyperliquid auto-cancels TP/SL orders when position closes, so refresh orders
        // Use debounced refresh to prevent multiple rapid calls when checking many DEXes
        if (hadPositionsForDex) {
          this.scheduleOrderRefresh();
        }

        return;
      }

      for (const assetPosition of data.assetPositions) {
        const pos = assetPosition.position;
        const szi = parseFloat(pos.szi);

        this.log(`Processing HIP-3 position: coin=${pos.coin}, szi=${szi}, entryPx=${pos.entryPx}`, LogSeverity.Debug);

        // Skip zero positions
        if (szi === 0) {
          this.log(`Skipping zero position for ${pos.coin}`, LogSeverity.Debug);
          continue;
        }

        // Try exact match first, then case-insensitive (O(1) map lookup)
        let market: Market | undefined = marketsById[pos.coin];
        if (!market) {
          // Try case-insensitive lookup for HIP-3 markets via O(1) map
          market = this.marketsLowerCaseMap.get(pos.coin.toLowerCase());
          if (market) {
            this.log(
              `Found market via case-insensitive lookup: pos.coin="${pos.coin}" matched market.id="${market.id}"`,
              LogSeverity.Info,
            );
          }
        }
        if (!market) {
          this.log(
            `HIP-3 position for unknown market: ${pos.coin}. Available HIP-3 markets: ${Object.keys(marketsById)
              .filter((k) => k.includes(':'))
              .join(', ')}`,
            LogSeverity.Warning,
          );
          continue;
        }

        const side = szi > 0 ? PositionSide.Long : PositionSide.Short;
        const position: Position = {
          // Spread defaults first, then explicit API values override them
          ...this._getSymbolConfigurationFromAssetData(market.id),
          id: createPositionId(market.symbol, side, PositionMode.OneWay),
          symbol: market.symbol,
          side,
          entryPrice: parseFloat(pos.entryPx),
          notional: parseFloat(pos.positionValue),
          leverage: pos.leverage.value,
          unrealizedPnl: parseFloat(pos.unrealizedPnl),
          contracts: Math.abs(szi),
          liquidationPrice: pos.liquidationPx ? parseFloat(pos.liquidationPx) : 0,
          marginSize: pos.marginUsed ? parseFloat(pos.marginUsed) : undefined,
          positionMode: PositionMode.OneWay,
          marginType: pos.leverage.type === 'cross' ? MarginType.Cross : MarginType.Isolated,
        };

        this.log(
          `Created position object: id=${position.id}, symbol=${position.symbol}, side=${position.side}, contracts=${position.contracts}`,
          LogSeverity.Info,
        );

        newPositions.push(position);
      }

      // Check if we HAD positions for this dex before (for order refresh trigger)
      const hadPositionsForDex = this.store.positions.some((p) => {
        const market = this.store.marketsBySymbol[p.symbol];
        return market?.info?.hip3Dex === dexName;
      });

      // ALWAYS filter out old HIP-3 positions for this dex, even if no new positions
      // This ensures closed positions (size = 0) are properly removed from the store
      const currentPositions = this.store.positions.filter((p) => {
        const market = this.store.marketsBySymbol[p.symbol];
        return market?.info?.hip3Dex !== dexName;
      });

      this.log(
        `Updating store: ${currentPositions.length} existing positions + ${newPositions.length} new HIP-3 positions for ${dexName}`,
        LogSeverity.Info,
      );

      // Deduplicate before updating store - real positions take precedence over fake ones
      // Use uniqBy to keep first occurrence (real positions come before fakes)
      const allPositions = uniqBy([...currentPositions, ...newPositions, ...this.getFakePositions()], 'id');
      this.store.addOrUpdatePositionsFull(allPositions);

      if (newPositions.length > 0) {
        this.log(`Successfully updated ${newPositions.length} HIP-3 positions for dex: ${dexName}`, LogSeverity.Info);
      } else {
        this.log(`Cleared HIP-3 positions for dex: ${dexName} (all positions closed)`, LogSeverity.Info);

        // If we had positions before and now have none (all zero-sized), position was closed
        // Hyperliquid auto-cancels TP/SL orders when position closes, so refresh orders
        // Use debounced refresh to prevent multiple rapid calls when checking many DEXes
        if (hadPositionsForDex) {
          this.scheduleOrderRefresh();
        }
      }
    } catch (error) {
      this.log(`Failed to fetch HIP-3 positions for ${dexName}: ${error}`, LogSeverity.Warning);
    }
  };

  /**
   * Fetch HIP-3 positions from all DEXes asynchronously.
   * Uses sequential fetching to avoid rate limiting with many DEXes.
   * Errors are logged but don't block other DEX fetches.
   */
  fetchAllHip3Positions = async (): Promise<void> => {
    if (this.hip3DexNames.length === 0) return;

    // Fetch sequentially to avoid rate limiting - each fetch is fast
    // and we don't want to hammer the API with parallel requests
    for (const dexName of this.hip3DexNames) {
      if (this.isDisposed) return;
      try {
        await this.fetchHip3Positions(dexName);
      } catch (_error) {
        // Already logged in fetchHip3Positions, continue with other DEXes
      }
    }
  };

  /**
   * Refresh HIP-3 positions for specific DEXes (fire and forget).
   * Called after fills to update positions without blocking message processing.
   * Also resets the periodic refresh timer so we don't double-refresh.
   */
  refreshHip3PositionsAsync = (dexNames: Set<string>): void => {
    if (dexNames.size === 0) return;

    // Mark that a WS-triggered refresh happened so periodic tick skips redundant fetch
    this._lastPositionRefreshTime = Date.now();

    // Fire and forget - don't await, just log errors
    Promise.all(
      Array.from(dexNames).map((dexName) =>
        this.fetchHip3Positions(dexName).catch((error) => {
          this.log(`Failed to refresh HIP-3 positions for ${dexName}: ${error}`, LogSeverity.Warning);
        }),
      ),
    ).catch(() => {
      // Ignore - individual errors already logged
    });
  };

  /**
   * Debounced HIP-3 position refresh trigger.
   * Used by fill handlers as a REST safety net when dex clearinghouseState
   * updates are delayed or missed.
   */
  public scheduleHip3PositionRefresh = (dexNames: Set<string>): void => {
    if (dexNames.size === 0 || this.isDisposed) return;

    for (const dexName of dexNames) {
      if (dexName) {
        this._pendingHip3PositionRefreshDexes.add(dexName);
      }
    }

    if (this._hip3PositionRefreshDebounceTimer) {
      clearTimeout(this._hip3PositionRefreshDebounceTimer);
    }

    this._hip3PositionRefreshDebounceTimer = setTimeout(() => {
      this._hip3PositionRefreshDebounceTimer = null;
      if (this.isDisposed || this._pendingHip3PositionRefreshDexes.size === 0) return;

      const pendingDexes = new Set(this._pendingHip3PositionRefreshDexes);
      this._pendingHip3PositionRefreshDexes.clear();
      this.refreshHip3PositionsAsync(pendingDexes);
    }, this.HIP3_POSITION_REFRESH_DEBOUNCE_MS);
  };

  private fetchOrdersInternal = async (): Promise<Order[]> => {
    const user = this.options.vault || this.options.applicationId;
    const orders: Order[] = [];
    const observedOpenOrderIds = new Set<string>();
    const recordObservedOpenOrderIds = (openOrders: unknown): void => {
      if (!Array.isArray(openOrders)) return;
      for (const openOrder of openOrders) {
        if (!openOrder || typeof openOrder !== 'object') continue;
        const oid = (openOrder as { oid?: unknown }).oid;
        if (oid !== undefined && oid !== null) {
          observedOpenOrderIds.add(`${oid}`);
        }
      }
    };

    try {
      // Fetch core perp orders (without dex param)
      const { data: coreOrders } = await this.xhr.post(
        ENDPOINTS.INFO,
        { type: INFO_TYPES.FRONTEND_OPEN_ORDERS, user },
        { headers: { 'Content-Type': 'application/json' } },
      );

      if (Array.isArray(coreOrders)) {
        recordObservedOpenOrderIds(coreOrders);
        orders.push(...this.parseOpenOrders(coreOrders));
      }

      // Fetch HIP-3 orders from each dex — track which dexes succeeded
      const fetchedDexes = new Set<string>();
      for (const dexName of this.hip3DexNames) {
        try {
          this.log(`Fetching orders from HIP-3 dex: ${dexName}`, LogSeverity.Debug);
          const { data: dexOrders } = await this.xhr.post(
            ENDPOINTS.INFO,
            { type: INFO_TYPES.FRONTEND_OPEN_ORDERS, user, dex: dexName },
            { headers: { 'Content-Type': 'application/json' } },
          );

          this.log(
            `HIP-3 dex ${dexName} returned ${Array.isArray(dexOrders) ? dexOrders.length : 0} orders`,
            LogSeverity.Debug,
          );

          fetchedDexes.add(dexName.toLowerCase());
          if (Array.isArray(dexOrders)) {
            recordObservedOpenOrderIds(dexOrders);
          }
          if (Array.isArray(dexOrders) && dexOrders.length > 0) {
            const parsedOrders = this.parseOpenOrders(dexOrders);
            this.log(`Successfully parsed ${parsedOrders.length} orders from HIP-3 dex: ${dexName}`, LogSeverity.Info);
            orders.push(...parsedOrders);
          }
        } catch (error) {
          this.log(`Failed to fetch orders from HIP-3 dex ${dexName}: ${error}`, LogSeverity.Warning);
        }
      }

      // Surgical update: only update individual orders that changed to prevent UI flickering
      // This preserves object references for unchanged orders, preventing unnecessary re-renders
      //
      // IMPORTANT: Re-read currentOrders from the store AFTER all REST calls complete.
      // During the awaited REST calls above, WS handlers (clearinghouseState, orderUpdates) may have
      // replaced the orders array via addOrUpdateOrdersFull. If we captured currentOrders
      // before the REST calls, we'd hold a reference to the OLD array and the removal step
      // would incorrectly remove orders that WS handlers had already added to the NEW array.
      // A degraded/retry-heavy refresh can see the same oid more than once
      // across core and dex `frontendOpenOrders` reads. Reconcile one row per
      // oid; the loop below snapshots `currentOrdersById` before applying
      // changes, so duplicate incoming rows would otherwise both look "new"
      // and could be appended twice.
      const uniqueOrders = uniqBy(orders, (order) => order.id);
      const fetchedOrderIds = new Set(uniqueOrders.map((o) => o.id));
      const currentOrders = [...this.store.orders]; // Snapshot the CURRENT array (after WS updates)
      const currentOrdersById = new Map(currentOrders.map((o) => [o.id, o]));
      const now = Date.now();

      // 1. Add new orders and update changed orders (one at a time, no array replacement)
      for (const newOrder of uniqueOrders) {
        // Reconcile with cancellation suppression:
        // If REST returns an order we thought was cancelled, decide whether to trust REST or the cancel
        const cancelTime = this._cancelledOrders.get(newOrder.id);
        if (cancelTime !== undefined) {
          if (now - cancelTime < this.CANCEL_SUPPRESSION_TRUST_WINDOW_MS) {
            // Cancel was recent — suppress this order (API lag, hasn't propagated yet)
            continue;
          } else {
            // Cancel was a while ago but REST still shows it as open — cancel may have failed.
            // Un-suppress and let the order through so the user sees it.
            this.unsuppressCancelledOrder(newOrder.id);
            this.log(
              `Cancel suppression expired for order ${newOrder.id} — REST still shows it as open, un-suppressing`,
              LogSeverity.Warning,
            );
          }
        }

        const existing = currentOrdersById.get(newOrder.id);
        if (!existing) {
          // New order - add it
          this.store.addOrder(newOrder);
        } else if (
          existing.status !== newOrder.status ||
          existing.price !== newOrder.price ||
          existing.amount !== newOrder.amount ||
          existing.filled !== newOrder.filled ||
          existing.type !== newOrder.type ||
          existing.timeInForce !== newOrder.timeInForce ||
          existing.clientOrderId !== newOrder.clientOrderId
        ) {
          // Order changed - update only this order
          this.store.updateOrder({ id: newOrder.id }, newOrder);
        }
        // Unchanged orders are left alone (preserves object reference)
      }

      // 2. Remove orders that are no longer in the fetched list
      // But preserve:
      //   - Optimistic orders (Creating/Updating) that haven't appeared in REST yet
      //   - HIP-3 orders from dexes whose REST fetch failed (don't remove what we can't verify)
      for (const currentOrder of currentOrders) {
        if (!fetchedOrderIds.has(currentOrder.id)) {
          const isOptimistic =
            currentOrder.status === OrderStatus.Creating || currentOrder.status === OrderStatus.Updating;
          if (isOptimistic) continue;

          // Don't remove HIP-3 orders if their dex fetch failed — we can't confirm they're gone
          const market = this.store.marketsBySymbol[currentOrder.symbol];
          const orderDex = market?.info?.hip3Dex?.toLowerCase();
          if (orderDex && !fetchedDexes.has(orderDex)) continue;

          this.store.removeOrder({ id: currentOrder.id });
        }
      }

      // 3. Clean up confirmed cancellations — orders not in REST are truly gone
      for (const [cancelledId] of this._cancelledOrders) {
        if (!observedOpenOrderIds.has(cancelledId)) {
          this._cancelledOrders.delete(cancelledId);
        }
      }

      return uniqueOrders;
    } catch (error) {
      this.log(`Failed to fetch orders: ${error}`, LogSeverity.Error);
      return [];
    }
  };

  fetchOrders = async (_symbols?: string[]): Promise<Order[]> => {
    if (this._fetchOrdersInFlight) {
      return this._fetchOrdersInFlight;
    }

    const request = this.fetchOrdersInternal();
    this._fetchOrdersInFlight = request;

    try {
      return await request;
    } finally {
      if (this._fetchOrdersInFlight === request) {
        this._fetchOrdersInFlight = null;
      }
    }
  };

  // Hyperliquid is the narrow legacy exception to safe-cex's pure fetcher
  // convention: fetchOrdersInternal performs surgical, DEX-aware reconciliation
  // while it builds the result. Fire-and-forget callers must use this explicitly
  // side-effecting name instead of discarding fetchOrders() directly.
  private refreshOrders = (): Promise<void> => this.fetchOrders().then(() => undefined);

  // Single coalesced retry chain for HIP-3 order refresh.
  // Multiple rapid order placements merge into one chain instead of spawning parallel chains.
  private _pendingRetryOrderIds = new Set<string>();
  private _retryChainActive = false;
  private _retryChainTimer: ReturnType<typeof setTimeout> | null = null;

  /**
   * Refreshes HIP-3 orders with retry logic.
   * Hyperliquid's API may not immediately return newly placed orders,
   * so we retry with increasing delays until orders appear.
   *
   * Multiple calls are coalesced: new order IDs are merged into the
   * active retry chain rather than spawning parallel chains.
   */
  private refreshHip3OrdersWithRetry = (expectedOrderIds: string[]): void => {
    // Merge new IDs into the shared pending set
    for (const id of expectedOrderIds) {
      this._pendingRetryOrderIds.add(id);
    }

    // If a retry chain is already running, the new IDs will be picked up on its next iteration
    if (this._retryChainActive) return;

    this._retryChainActive = true;
    this.runRetryChainStep(1);
  };

  private runRetryChainStep = (attempt: number): void => {
    const maxAttempts = 5;
    // Progressive delays: 300ms, 700ms, 1500ms, 3000ms, 5000ms
    const delays = [300, 700, 1500, 3000, 5000];
    const delay = delays[attempt - 1] || delays[delays.length - 1];

    this._retryChainTimer = setTimeout(async () => {
      this._retryChainTimer = null;

      if (this.isDisposed) {
        this.endRetryChain();
        return;
      }

      try {
        const orders = await this.fetchOrders();
        const foundIds = new Set(orders.map((o) => o.id));

        // Remove found IDs from pending set
        for (const id of foundIds) {
          this._pendingRetryOrderIds.delete(id);
        }

        if (this._pendingRetryOrderIds.size === 0 || attempt >= maxAttempts) {
          if (this._pendingRetryOrderIds.size > 0 && attempt >= maxAttempts) {
            this.log(
              `HIP-3 order refresh: ${this._pendingRetryOrderIds.size} expected orders not found after ${maxAttempts} attempts`,
              LogSeverity.Warning,
            );
          }
          this.endRetryChain();
          return;
        }

        // Continue retry chain with next attempt
        this.runRetryChainStep(attempt + 1);
      } catch (error) {
        this.log(`HIP-3 order refresh failed (attempt ${attempt}): ${error}`, LogSeverity.Warning);
        if (attempt < maxAttempts) {
          this.runRetryChainStep(attempt + 1);
        } else {
          this.endRetryChain();
        }
      }
    }, delay);
  };

  private endRetryChain = (): void => {
    this._retryChainActive = false;
    this._pendingRetryOrderIds.clear();
  };

  private parseOpenOrders = (openOrders: any[]): Order[] => {
    return openOrders
      .filter((openOrder) => {
        const oid = `${openOrder.oid}`;
        // Filter out dead orders (from updateOrder cancel+replace) — stringify to match Set<string>
        if (this._deadOrders.has(oid)) return false;
        // Filter out recently cancelled orders (suppressed until REST confirms)
        if (this.isOrderCancelled(oid)) return false;
        return true;
      })
      .map((openOrder) => this.mapOpenOrderRow(openOrder, OrderStatus.Open))
      .filter((order): order is Order => order !== null);
  };

  // One `frontendOpenOrders` row (also the `order` of an `orderStatus` answer).
  private mapOpenOrderRow = (openOrder: any, status: OrderStatus): Order | null => {
    // Try exact match first, then case-insensitive for HIP-3 markets (O(1) map lookup)
    let market: Market | undefined = this.store.marketsById[openOrder.coin];

    if (!market && openOrder.coin.includes(':')) {
      // Try case-insensitive lookup for HIP-3 markets via O(1) map
      market = this.marketsLowerCaseMap.get(openOrder.coin.toLowerCase());
    }
    if (!market) {
      return null;
    }

    // Infer order type from frontendOpenOrders response
    // REST API (frontendOpenOrders) provides orderType, triggerPx, isTrigger for both core perps and HIP-3
    const inferred = inferHyperliquidOrderTypeAndPrice(openOrder);
    // Live rows carry `cloid` (null when the order has none) — checked 2026-10-11.
    const clientOrderId = readHyperliquidCloid(openOrder.cloid);

    return {
      id: `${openOrder.oid}`,
      status,
      symbol: market.symbol,
      type: inferred.type,
      side: openOrder.side === 'B' ? OrderSide.Buy : OrderSide.Sell,
      price: inferred.price,
      amount: +openOrder.origSz,
      filled: +openOrder.origSz - +openOrder.sz,
      remaining: +openOrder.sz,
      reduceOnly: Boolean(openOrder.reduceOnly),
      lastUpdate: Date.now(),
      timeInForce: parseHyperliquidOrderTimeInForce(openOrder.tif),
      ...(clientOrderId ? { clientOrderId } : {}),
      info: openOrder,
    };
  };

  /**
   * Looks the order up by cloid with `orderStatus`, which also answers for
   * filled and cancelled orders — and, unlike `fetchOrders` here, never writes
   * the store. A miss is still never definitive: Hyperliquid accepts a nonce up
   * to two days old, so a send still in flight can land after the lookup.
   */
  findOrderByClientOrderId = async (symbol: string, clientOrderId: string): Promise<ClientOrderLookup> => {
    const notFound: ClientOrderLookup = { found: false, definitive: false };
    const inStore = this.store.orders.find(
      (order) => order.symbol === symbol && orderCarriesClientOrderId(order, clientOrderId),
    );
    if (inStore) return { found: true, order: inStore };

    const user = this.options.vault || this.options.applicationId;
    if (!user || !HYPERLIQUID_CLOID_PATTERN.test(clientOrderId)) return notFound;

    const { data } = await this.infoXhr.post(
      ENDPOINTS.INFO,
      { type: INFO_TYPES.ORDER_STATUS, user, oid: clientOrderId },
      { headers: { 'Content-Type': 'application/json' } },
    );
    if (data?.status !== 'order' || !data.order?.order) return notFound;

    const venueStatus = data.order.status;
    const status =
      venueStatus === 'open' || venueStatus === 'triggered'
        ? OrderStatus.Open
        : venueStatus === 'filled'
          ? OrderStatus.Closed
          : OrderStatus.Canceled;
    const order = this.mapOpenOrderRow(data.order.order, status);
    return order && order.symbol === symbol ? { found: true, order } : notFound;
  };

  /**
   * Account ledger from fills + funding.
   *
   * Hyperliquid emits no separate fee row — the fee rides on the fill next to that fill's
   * `closedPnl` — so one fill becomes up to two records. See `hyperliquid.income.ts`.
   *
   * Three deliberate departures from `fetchHistoricalFills` in this same file:
   *
   * - **Uses `infoXhr`, not `xhr`.** Every other private read here goes through the
   *   unlimited info client; `fetchHistoricalFills` is the odd one out.
   * - **Uses `vault || applicationId`.** `fetchHistoricalFills` reads `applicationId` alone, so
   *   on a vault account it returns the MASTER WALLET's data. Every other private read in
   *   this file uses the pair.
   * - **No 30-day floor and no 7-day chunking.** Those constants in `fetchHistoricalFills` are
   *   ours, bybit-shaped, with no upstream citation. Hyperliquid is address-queryable with
   *   no retention ceiling — that is the entire reason this venue matters for a journal —
   *   so the range is walked by row-cap exhaustion instead, and warns if it stops early.
   *
   * Known gap, stated rather than hidden: deposits, withdrawals and liquidations live in
   * `userNonFundingLedgerUpdates`, which is not read here, so the `transfer` and
   * `liquidation` categories are always empty on hyperliquid. Until that is added the
   * full equity identity cannot close on this venue — it is what distinguishes a losing
   * trade from a withdrawal.
   */
  fetchIncomeHistory = async (opts?: IncomeHistoryOptions): Promise<IncomeRecord[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const user = this.options.vault || this.options.applicationId;
    if (!user) {
      this.log('Skipping fetchIncomeHistory: no vault/applicationId', LogSeverity.Warning);
      return [];
    }

    const endTime = opts?.endTime ?? virtualClock.getCurrentTimeMs();
    const startTime = opts?.startTime ?? endTime - 30 * 24 * 60 * 60 * 1000;
    // Via the helper: a direct marketsById lookup misses every HIP-3 coin on casing.
    const resolveSymbol = (coin: string) => this.resolveCoinToSymbol(coin);

    const onCapHit = (type: string, pages: number) =>
      this.log(
        `fetchIncomeHistory stopped at the ${pages}-page cap for ${type}; older rows in this window were not fetched`,
        LogSeverity.Warning,
      );

    const post = async (type: string, body: Record<string, any> = {}) => {
      const { data } = await this.infoXhr.post<any>(
        ENDPOINTS.INFO,
        { type, user, ...body },
        { headers: { 'Content-Type': 'application/json' } },
      );
      return Array.isArray(data) ? data : [];
    };

    try {
      const records: IncomeRecord[] = [];

      if (!opts?.incomeType || opts.incomeType !== 'FUNDING_FEE') {
        const fills = await paginateHyperliquidByTime(
          post,
          INFO_TYPES.USER_FILLS_BY_TIME,
          startTime,
          endTime,
          (row) => String(row.tid ?? `${row.time}:${row.coin}:${row.px}:${row.sz}`),
          onCapHit,
        );
        for (const fill of fills) {
          records.push(...mapHyperliquidFillToIncomeRecords(fill, resolveSymbol));
        }
      }

      if (!opts?.incomeType || opts.incomeType === 'FUNDING_FEE') {
        // Paged, not a single request: funding is row-capped like fills, and a 30-day
        // window on a multi-coin account exceeds that in days. Truncating here would be
        // INVISIBLE to the ledger audit, because the audit reads the same truncated
        // response — both sides shrink together and the residual stays 0.
        const funding = await paginateHyperliquidByTime(
          post,
          INFO_TYPES.USER_FUNDING,
          startTime,
          endTime,
          (row) => {
            const delta = row.delta ?? row;
            return `${row.hash ?? ''}:${delta.coin ?? ''}:${row.time ?? ''}`;
          },
          onCapHit,
        );
        for (const row of funding) {
          const record = mapHyperliquidFundingToIncomeRecord(row, resolveSymbol);
          if (record) records.push(record);
        }
      }

      const filtered = opts?.symbol ? records.filter((record) => record.symbol === opts.symbol) : records;
      return filtered.sort((a, b) => b.timestamp - a.timestamp);
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  /**
   * Closed positions folded from this account's own fills.
   *
   * Deliberately NOT named `fetchClosedPnl`: hyperliquid has no closed-position endpoint,
   * so this is a reshaping of the fills rather than a second observation of them. Under
   * the venue method's name it would look identical to bybit's exchange-reported rows and
   * the cross-axis oracle would "verify" the ledger against data folded from the same
   * source. `resolveClosedPositions` composes the two and stamps provenance.
   */
  /**
   * TRUE here: this folds `userFillsByTime`, which is exactly what `fetchIncomeHistory`
   * reads. So it cannot cross-check the ledger — they would agree by construction. Binance
   * folds a different endpoint than its ledger reads and therefore declares false.
   */
  closedPositionsShareLedgerSource = true;

  /**
   * Coin -> safe-cex symbol, honouring HIP-3's casing.
   *
   * HIP-3 market ids are built from the meta's own casing (`vntls:vKRAKEN`), so a direct
   * `marketsById[coin]` lookup MISSES for every HIP-3 asset — which is why `parseOpenOrders`
   * and the HIP-3 position path both fall back to `marketsLowerCaseMap`. A history mapper
   * that skips the fallback emits the raw `dex:ASSET` string as the symbol, which looks
   * like a deliberate fallback rather than a miss and can never match a caller's symbol
   * filter.
   */
  private resolveCoinToSymbol = (coin: string): string | undefined => {
    const direct = this.store.marketsById[coin];
    if (direct) return direct.symbol;
    return this.marketsLowerCaseMap.get(coin.toLowerCase())?.symbol;
  };

  /**
   * Historical orders, including ones that never filled — see
   * `hyperliquid.order-history.ts`. Measured live: 1020 orders, only 315 of them filled.
   *
   * Two properties of `historicalOrders` shape this method and are not negotiable:
   *
   * - **the server-side time window is ignored**, so `from`/`to` are applied here. Passing
   *   `startTime`/`endTime` returned a byte-identical response, which would otherwise read
   *   as a working filter;
   * - **it returns one row per status transition**, 2000 rows for 1020 orders on a live
   *   account, so the rows are collapsed to the latest status per `oid` before mapping.
   *
   * The 2000-row ceiling is a retention limit, not a page size — there is no cursor to
   * walk past it, so a window older than the snapshot returns nothing rather than paging.
   */
  fetchHistoricalOrders = async (opts?: OrderHistoryOptions): Promise<HistoricalOrder[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const user = this.options.vault || this.options.applicationId;
    if (!user) {
      this.log('Skipping fetchHistoricalOrders: no vault/applicationId', LogSeverity.Warning);
      return [];
    }

    const SNAPSHOT_CAP = 2000;
    const resolveSymbol = (coin: string) => this.resolveCoinToSymbol(coin);

    try {
      const { data } = await this.infoXhr.post<any>(
        ENDPOINTS.INFO,
        { type: INFO_TYPES.HISTORICAL_ORDERS, user },
        { headers: { 'Content-Type': 'application/json' } },
      );

      // A rejected `/info` comes back 200 with a STRING body, so a bare isArray guard turns
      // it into "this account has no orders" with nothing logged.
      if (!Array.isArray(data)) {
        this.log(`fetchHistoricalOrders got a non-array historicalOrders response`, LogSeverity.Warning);
        return [];
      }

      const rows: Array<Record<string, any>> = data;
      if (rows.length >= SNAPSHOT_CAP) {
        this.log(
          `fetchHistoricalOrders received the full ${SNAPSHOT_CAP}-row historicalOrders snapshot; older orders are not retrievable`,
          LogSeverity.Warning,
        );
      }

      const orders = collapseHyperliquidOrderHistory(rows)
        .map((row) => mapHyperliquidHistoricalOrder(row, resolveSymbol))
        .filter((order): order is HistoricalOrder => order !== null)
        .filter((order) => {
          if (opts?.symbol && order.symbol !== opts.symbol) return false;
          if (opts?.from && order.createdTime < opts.from) return false;
          if (opts?.to && order.createdTime > opts.to) return false;
          return true;
        })
        .sort((a, b) => b.createdTime - a.createdTime);

      return opts?.limit && opts.limit > 0 ? orders.slice(0, opts.limit) : orders;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  deriveClosedPositions = async (opts: HistoricalTradeOptions): Promise<ClosedPnlEntry[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const user = this.options.vault || this.options.applicationId;
    if (!user) {
      return [];
    }

    const to = opts.to ?? virtualClock.getCurrentTimeMs();
    const from = opts.from ?? to - 30 * 24 * 60 * 60 * 1000;
    // Via the helper: a direct marketsById lookup misses every HIP-3 coin on casing.
    const resolveSymbol = (coin: string) => this.resolveCoinToSymbol(coin);

    const post = async (type: string, body: Record<string, any> = {}) => {
      const { data } = await this.infoXhr.post<any>(
        ENDPOINTS.INFO,
        { type, user, ...body },
        { headers: { 'Content-Type': 'application/json' } },
      );
      return Array.isArray(data) ? data : [];
    };

    try {
      const fills = await paginateHyperliquidByTime(post, INFO_TYPES.USER_FILLS_BY_TIME, from, to, (row) =>
        String(row.tid ?? `${row.time}:${row.coin}:${row.px}:${row.sz}`),
      );
      const entries = deriveHyperliquidClosedPositions(fills, resolveSymbol, opts.accountName || '');
      return opts.symbol ? entries.filter((entry) => entry.symbol === opts.symbol) : entries;
    } catch (err: any) {
      this.emitAndLogError(err?.response?.data?.message || err?.message);
      return [];
    }
  };

  fetchIncomeHistoryRange = async (
    startTime: number,
    endTime: number,
    incomeType?: string,
  ): Promise<IncomeRecord[]> => {
    // No chunking: hyperliquid bounds a response by row count, not by window width.
    return await this.fetchIncomeHistory({ startTime, endTime, incomeType });
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 50 } = opts;

    if (!symbol) {
      // Hyperliquid requires symbol for execution queries
      return [];
    }

    const normalizeToMilliseconds = (value?: number) => {
      if (value === undefined || value === null || !Number.isFinite(value)) {
        return undefined;
      }
      return value > 1_000_000_000_000 ? Math.floor(value) : Math.floor(value * 1000);
    };
    const requestedFromMilliseconds = normalizeToMilliseconds(from);
    const requestedToMilliseconds = normalizeToMilliseconds(to) ?? virtualClock.getCurrentTimeMs();
    const maxLimit = Math.min(Math.max(limit, 1), 500);
    const EXECUTION_QUERY_MAX_RANGE_MS = 7 * 24 * 60 * 60 * 1000 - 1;
    const DEFAULT_EXECUTIONS_LOOKBACK_MS = 30 * 24 * 60 * 60 * 1000;
    const minimumAllowedFromMilliseconds = requestedToMilliseconds - DEFAULT_EXECUTIONS_LOOKBACK_MS;
    const effectiveFromMilliseconds = Math.max(
      requestedFromMilliseconds ?? minimumAllowedFromMilliseconds,
      minimumAllowedFromMilliseconds,
    );

    if (requestedFromMilliseconds !== undefined && requestedFromMilliseconds < minimumAllowedFromMilliseconds) {
      this.log(
        `fetchHistoricalFills capped to last 30 days for ${symbol} (requestedFrom=${requestedFromMilliseconds}, cappedFrom=${effectiveFromMilliseconds})`,
        LogSeverity.Warning,
      );
    }

    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[symbol];
    if (!market) {
      this.log(`Market ${symbol} not found`, LogSeverity.Warning);
      return [];
    }

    if (effectiveFromMilliseconds > requestedToMilliseconds) {
      return [];
    }

    const queryWindows: Array<{ startTime: number; endTime: number }> = [];
    let windowEnd = requestedToMilliseconds;
    while (windowEnd >= effectiveFromMilliseconds) {
      const windowStart = Math.max(effectiveFromMilliseconds, windowEnd - EXECUTION_QUERY_MAX_RANGE_MS);
      queryWindows.push({ startTime: windowStart, endTime: windowEnd });
      if (windowStart <= effectiveFromMilliseconds) {
        break;
      }
      windowEnd = windowStart - 1;
    }

    const user = this.options.applicationId;
    if (!user) {
      this.log('Skipping fetchHistoricalFills: applicationId missing', LogSeverity.Warning);
      return [];
    }

    const coinCandidates = new Set(
      [market.info?.name, market.id, symbolToCoin(symbol)].map((value) => `${value || ''}`.trim()).filter(Boolean),
    );

    const parseSide = (value: unknown): OrderSide | undefined => {
      const normalizedSide = `${value || ''}`.trim().toUpperCase();
      if (normalizedSide === 'B' || normalizedSide === 'BUY') {
        return OrderSide.Buy;
      }
      if (normalizedSide === 'A' || normalizedSide === 'S' || normalizedSide === 'SELL') {
        return OrderSide.Sell;
      }
      return undefined;
    };

    try {
      const deduped = new Map<string, Execution>();

      for (const queryWindow of queryWindows) {
        const { data } = await this.xhr.post(
          ENDPOINTS.INFO,
          {
            type: INFO_TYPES.USER_FILLS_BY_TIME,
            user,
            startTime: queryWindow.startTime,
            endTime: queryWindow.endTime,
          },
          {
            headers: {
              'Content-Type': 'application/json',
            },
          },
        );

        if (!Array.isArray(data) || data.length === 0) {
          continue;
        }

        for (const fill of data) {
          const fillCoin = `${fill?.coin || ''}`.trim();
          if (!coinCandidates.has(fillCoin)) {
            continue;
          }

          const executionOrderId = String(fill?.oid || fill?.orderId || '');
          if (orderId && executionOrderId && executionOrderId !== String(orderId)) {
            continue;
          }

          const timestampMilliseconds = Number(fill?.time);
          if (!Number.isFinite(timestampMilliseconds) || timestampMilliseconds <= 0) {
            continue;
          }
          if (
            (requestedFromMilliseconds !== undefined && timestampMilliseconds < requestedFromMilliseconds) ||
            timestampMilliseconds > requestedToMilliseconds
          ) {
            continue;
          }

          const price = Number.parseFloat(String(fill?.px || 0));
          const qty = Number.parseFloat(String(fill?.sz || 0));
          if (!Number.isFinite(price) || !Number.isFinite(qty) || price <= 0 || qty <= 0) {
            continue;
          }

          const side = parseSide(fill?.side);
          if (!side) {
            continue;
          }

          const feeRaw = Number.parseFloat(String(fill?.fee || 0));
          // `userFills`/`userFillsByTime` carry `builderFee` per fill. Absent on
          // fills placed without a builder code, which is why it is not defaulted.
          const builderFeeRaw = Number.parseFloat(String((fill as { builderFee?: string })?.builderFee ?? ''));
          const closedPnlRaw = Number.parseFloat(String(fill?.closedPnl ?? ''));
          const execution: Execution = {
            id: fill?.tid !== undefined && fill?.tid !== null ? `${fill.tid}` : undefined,
            timestamp: Math.floor(timestampMilliseconds / 1000),
            symbol,
            orderId: executionOrderId || undefined,
            price,
            qty,
            side,
            commission: Number.isFinite(feeRaw) ? Math.abs(feeRaw) : undefined,
            commissionAsset: fill?.feeToken || 'USDC',
            // positionSide is deliberately NOT derived from `dir`. Hyperliquid is
            // one-way only (metadata `positionModes: [OneWay]`), so `dir`
            // ("Open Long" / "Close Long") describes the fill, not a hedge leg —
            // mapping it would split one continuous position into long/short
            // buckets that a one-way consumer never seeds.
            realizedPnl: Number.isFinite(closedPnlRaw) ? closedPnlRaw : undefined,
            // `crossed` means the fill crossed the spread, i.e. took liquidity.
            maker: typeof fill?.crossed === 'boolean' ? !fill.crossed : undefined,
            // What the venue says WE earned, as opposed to `commission` above,
            // which is what the user paid. Hyperliquid is the only venue in the
            // integration that reports this, and it is what lets the computed
            // builder cut be checked per fill rather than per day.
            venueReportedRevenue: Number.isFinite(builderFeeRaw) ? Math.abs(builderFeeRaw) : undefined,
            venueReportedRevenueAsset: Number.isFinite(builderFeeRaw) ? fill?.feeToken || 'USDC' : undefined,
          };

          const key = `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;
          deduped.set(key, execution);
        }

        // Windows are fetched newest -> oldest, so once we have enough rows we can stop.
        if (deduped.size >= maxLimit) {
          break;
        }
      }

      return [...deduped.values()].sort((a, b) => b.timestamp - a.timestamp).slice(0, maxLimit);
    } catch (error: any) {
      this.log(`Error fetching executions: ${error.message}`, LogSeverity.Error);
      return [];
    }
  };

  /**
   * Fetch fills for specific order IDs from REST API.
   * Used by chaser to verify order completion before placing new orders.
   * @param orderIds Array of order IDs to fetch fills for
   * @returns Map of order ID to total filled amount
   */
  fetchFillsForOrderIds = async (orderIds: string[]): Promise<Map<string, number>> => {
    const fillsByOrderId = new Map<string, number>();

    if (orderIds.length === 0) {
      return fillsByOrderId;
    }

    try {
      const { data } = await this.xhr.post(
        ENDPOINTS.INFO,
        {
          type: INFO_TYPES.USER_FILLS,
          user: this.options.key,
          aggregateByTime: true,
        },
        {
          headers: {
            'Content-Type': 'application/json',
          },
        },
      );

      if (!data || !Array.isArray(data)) {
        throw new Error('Hyperliquid fill reconciliation returned an invalid response');
      }

      // Filter fills for the requested order IDs and sum up amounts
      for (const fill of data) {
        const oid = String(fill.oid);
        if (orderIds.includes(oid)) {
          const amount = parseFloat(fill.sz || '0');
          fillsByOrderId.set(oid, (fillsByOrderId.get(oid) || 0) + amount);
        }
      }

      return fillsByOrderId;
    } catch (error: any) {
      this.log(`Error fetching fills for order IDs: ${error.message}`, LogSeverity.Warning);
      throw error;
    }
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    return this.publicWebsocket.listenOHLCV(opts, callback);
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);
    // Spot is always 1x. `updateLeverage` on a spot asset index is not a
    // meaningful action — silently no-op rather than signing and posting a
    // request the venue has no use for. (The retired standalone spot class
    // did send it, copied verbatim from the perp path.)
    if (market.isSpot) return;
    const coin = market.id;
    const activeAssetData = this.activeAssetData.get(coin);
    const timestamp = virtualClock.getCurrentTimeMs();
    const action = {
      type: 'updateLeverage',
      asset: market.info.hyperliquidId,
      isCross: activeAssetData?.leverage.type === 'cross',
      leverage: +inputLeverage,
    };
    const signature = await this.signActionOrDefer(action, timestamp);
    await this.xhr.post(
      ENDPOINTS.EXCHANGE,
      {
        action,
        nonce: timestamp,
        ...(signature ? { signature } : {}),
        ...(this.options.vault ? { vaultAddress: this.options.vault } : {}),
      },
      {
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );

    // Invalidate fake positions cache since leverage changed
    this.invalidateFakePositionsCache();
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[symbol!];
    if (!market) throw new Error(`Market ${symbol} not found`);
    // Spot has no margin mode — see the `setLeverage` note above.
    if (market.isSpot) return;
    const coin = market.id;
    const activeAssetData = this.activeAssetData.get(coin);
    const timestamp = virtualClock.getCurrentTimeMs();
    const action = {
      type: 'updateLeverage',
      asset: market.info.hyperliquidId,
      isCross: !isolated,
      leverage: activeAssetData?.leverage.value,
    };
    const signature = await this.signActionOrDefer(action, timestamp);
    await this.xhr.post(
      ENDPOINTS.EXCHANGE,
      {
        action,
        nonce: timestamp,
        ...(signature ? { signature } : {}),
        ...(this.options.vault ? { vaultAddress: this.options.vault } : {}),
      },
      {
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );

    // Invalidate fake positions cache since margin type changed
    this.invalidateFakePositionsCache();
  };

  _hasFullSizedPositionOppositeSide = (symbol: string, side: OrderSide, amount: number) => {
    const targetSide = side === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
    return this.store.positions.some(
      (p) => p.symbol === symbol && p.side === targetSide && p.contracts !== 0 && p.contracts >= amount,
    );
  };

  // `cloid` goes on the entry wire; each TP/SL leg gets its own, so every wire is identifiable.
  createLimitOrderWires = (opts: PlaceOrderOpts, cloid: string = mintHyperliquidCloid()) => {
    if (opts.type !== OrderType.Limit) {
      throw new Error('Not supported');
    }

    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    // Limit with TP & SL
    //   {
    //     "action": {
    //         "grouping": "normalTpsl",
    //         "orders": [
    //             {
    //                 "a": 98,
    //                 "b": true,
    //                 "p": "3.2",
    //                 "r": false,
    //                 "s": "21",
    //                 "t": {
    //                     "limit": {
    //                         "tif": "Gtc"
    //                     }
    //                 }
    //             },
    //             {
    //                 "a": 98,
    //                 "b": false,
    //                 "p": "1.84",
    //                 "r": true,
    //                 "s": "21",
    //                 "t": {
    //                     "trigger": {
    //                         "isMarket": true,
    //                         "tpsl": "sl",
    //                         "triggerPx": "2"
    //                     }
    //                 }
    //             },
    //             {
    //                 "a": 98,
    //                 "b": false,
    //                 "p": "3.68",
    //                 "r": true,
    //                 "s": "21",
    //                 "t": {
    //                     "trigger": {
    //                         "isMarket": true,
    //                         "tpsl": "tp",
    //                         "triggerPx": "4"
    //                     }
    //                 }
    //             }
    //         ],
    //         "type": "order"
    //     },
    //     "isFrontend": true,
    //     "nonce": 1732047758486,
    //     "signature": {
    //         "r": "0xb35f9b10a234ecd7d9469c0a71548cfc97b3021656103a4124ffe37059d970f5",
    //         "s": "0x7c03e973a1d21c5ff375e1849bcfe36d57e01828e3b91917d938630b99e1b95f",
    //         "v": 28
    //     },
    //     "vaultAddress": null
    // }
    // Normal limit
    //   {
    //     "action": {
    //         "grouping": "na",
    //         "orders": [
    //             {
    //                 "a": 98,
    //                 "b": true,
    //                 "p": "3.2",
    //                 "r": false,
    //                 "s": "21",
    //                 "t": {
    //                     "limit": {
    //                         "tif": "Gtc"
    //                     }
    //                 }
    //             }
    //         ],
    //         "type": "order"
    //     },
    //     "isFrontend": true,
    //     "nonce": 1732047799936,
    //     "signature": {
    //         "r": "0x9ec9f3a93a0cef9f1823d72a7a891c657bf14639cc374e85fe5d24d3fde275dc",
    //         "s": "0x7c6bb426e249978c153651394e7f6ac6f97ced0bf440ff50b8dba9f1ef0bc150",
    //         "v": 27
    //     },
    //     "vaultAddress": null
    // }
    // const req = omitUndefined({
    //   category: this.getCategoryForSymbol(opts.symbol),
    //   symbol: opts.symbol,
    //   side: inverseObj(ORDER_SIDE)[opts.side],
    //   orderType: inverseObj(ORDER_TYPE)[type],
    //   qty: `${amount}`,
    //   price: opts.type === OrderType.Limit ? `${price}` : undefined,
    //   stopLoss: opts.stopLoss ? `${stopLoss}` : undefined,
    //   takeProfit: opts.takeProfit ? `${takeProfit}` : undefined,
    //   reduceOnly: opts.reduceOnly || false,
    //   triggerPrice: opts.triggerPrice ? `${triggerPrice}` : undefined,
    //   triggerDirection: triggerDirection ? `${triggerDirection}` : undefined,
    //   triggerBy:
    //     opts.type === OrderType.StopLoss
    //       ? opts.slTriggerType === OrderStopTriggerType.LastPrice
    //         ? 'LastPrice'
    //         : 'MarkPrice'
    //       : opts.type === OrderType.TakeProfit
    //         ? opts.tpTriggerType === OrderStopTriggerType.MarkPrice
    //           ? 'MarkPrice'
    //           : 'LastPrice'
    //         : undefined,
    //   slTriggerBy: opts.stopLoss
    //     ? opts.slTriggerType === OrderStopTriggerType.LastPrice
    //       ? 'LastPrice'
    //       : 'MarkPrice'
    //     : undefined,
    //   tpTriggerBy: opts.takeProfit
    //     ? opts.slTriggerType === OrderStopTriggerType.MarkPrice
    //       ? 'MarkPrice'
    //       : 'LastPrice'
    //     : undefined,
    //   timeInForce: opts.type === OrderType.Limit ? timeInForce : undefined,
    //   closeOnTrigger: false,
    //   positionIdx,
    // });

    const assetIndex = market?.info.hyperliquidId;

    const coin = symbolToCoin(market.symbol);
    // Adjust size to market precision (matches CCXT approach)
    const orderSize = adjust(opts.amount, market.precision.amount);

    const orderWire = orderRequestToOrderWire(
      {
        coin,
        is_buy: opts.side === OrderSide.Buy,
        sz: orderSize,
        limit_px: +this.priceToPrecision(market.symbol, opts.price || 0),
        order_type: {
          limit: {
            tif: opts.timeInForce === OrderTimeInForce.PostOnly ? 'Alo' : 'Gtc',
          },
        },
        reduce_only: this.resolveReduceOnly(market, Boolean(opts.reduceOnly)),
        cloid,
      },
      assetIndex,
    );

    const orderWires = [orderWire];

    //                 "a": 98,
    //                 "b": false,
    //                 "p": "1.84",
    //                 "r": true,
    //                 "s": "21",
    //                 "t": {
    //                     "trigger": {
    //                         "isMarket": true,
    //                         "tpsl": "sl",
    //                         "triggerPx": "2"
    //                     }
    //                 }

    if (opts.stopLoss) {
      orderWires.push(
        orderRequestToOrderWire(
          {
            coin,
            is_buy: opts.side !== OrderSide.Buy,
            sz: orderSize,
            limit_px: +this.priceToPrecision(market.symbol, opts.price || 0),
            order_type: {
              trigger: {
                isMarket: true,
                tpsl: 'sl',
                triggerPx: +this.priceToPrecision(market.symbol, opts.stopLoss),
              },
            },
            reduce_only: this.resolveReduceOnly(market, true),
            cloid: mintHyperliquidCloid(),
          },
          assetIndex,
        ),
      );
    }

    if (opts.takeProfit) {
      orderWires.push(
        orderRequestToOrderWire(
          {
            coin,
            is_buy: opts.side !== OrderSide.Buy,
            sz: orderSize,
            limit_px: +this.priceToPrecision(market.symbol, opts.price || 0),
            order_type: {
              trigger: {
                isMarket: true,
                tpsl: 'tp',
                triggerPx: +this.priceToPrecision(market.symbol, opts.takeProfit),
              },
            },
            reduce_only: this.resolveReduceOnly(market, true),
            cloid: mintHyperliquidCloid(),
          },
          assetIndex,
        ),
      );
    }

    return orderWires;
  };

  placeLimitOrder = async (opts: PlaceOrderOpts, cloid: string = mintHyperliquidCloid()) => {
    if (opts.type !== OrderType.Limit) {
      throw new Error('Not supported');
    }

    const orderWires = this.createLimitOrderWires(opts, cloid);

    // Get builder info if wallet is approved for builder fee
    const builderInfo = await this.getBuilderInfo();

    const action =
      opts.takeProfit || opts.stopLoss
        ? orderWiresToNormalTPSLAction(orderWires, builderInfo)
        : orderWiresToOrderAction(orderWires, builderInfo);

    const nonce = this.generateNonce();
    const signature = await signL1Action(
      createWalletFromPrivateKey(this.options.secret),
      action,
      this.options.vault || null,
      nonce,
      !this.options.testnet,
    );

    const payload = { action, nonce, signature, ...(this.options.vault ? { vaultAddress: this.options.vault } : {}) };

    // Record every wire BEFORE the REST call so the WS handler identifies each
    // order by its cloid, even when the frame beats the response.
    const market = this.store.marketsBySymbol[opts.symbol];
    if (market) this.recordPlacement(orderWires, market.symbol);
    // Fallback for a frame that arrives without its cloid (see recordPlacement).
    if (market?.id.includes(':')) {
      const coin = symbolToCoin(market.symbol);
      this.addToPrePlacementCache(coin, opts.side, OrderType.Limit, opts.price || 0, opts.amount, cloid);
    }

    const data = await this.postOrderAction(payload);

    const orderIds = this.validateResponseAndExtractOrderIds(data);

    // orderWires[0] is always the main limit wire; any extras are TP/SL triggers,
    // which have no time-in-force of their own.
    const [limitOrderId] = orderIds;
    if (limitOrderId) {
      this.rememberLimitOrderTimeInForce(
        limitOrderId,
        opts.timeInForce === OrderTimeInForce.PostOnly ? OrderTimeInForce.PostOnly : OrderTimeInForce.GoodTillCancel,
      );
    }

    // For HIP-3 limit orders, add optimistic order immediately
    if (market?.id.includes(':')) {
      for (const orderId of orderIds) {
        // Add to shared pending cache so any WS handler instance can find it
        this.addPendingHip3Order(orderId, OrderType.Limit, opts.price || 0);

        // Add optimistic order with "Creating" status so UI shows immediate feedback
        const optimisticOrder: Order = {
          id: orderId,
          status: OrderStatus.Creating,
          symbol: market.symbol,
          type: OrderType.Limit,
          side: opts.side,
          price: opts.price || 0,
          amount: opts.amount,
          filled: 0,
          remaining: opts.amount,
          reduceOnly: Boolean(opts.reduceOnly),
          timeInForce:
            opts.timeInForce === OrderTimeInForce.PostOnly
              ? OrderTimeInForce.PostOnly
              : OrderTimeInForce.GoodTillCancel,
          lastUpdate: virtualClock.getCurrentTimeMs(),
        };
        this.store.addOrUpdateOrder(optimisticOrder);
      }
      this.refreshHip3OrdersWithRetry(orderIds);
    }

    return orderIds;
  };

  /**
   * Single chokepoint for the reduce-only wire flag.
   *
   * Spot markets have no positions, so there is nothing to reduce — every
   * spot wire must send `r: false`, including the TP/SL legs, which are
   * unconditionally reduce-only on perps. Keep every `reduce_only:` site
   * routed through here so the spot rule can't be missed when a new order
   * shape is added.
   */
  private resolveReduceOnly = (market: Market, requested: boolean): boolean => (market.isSpot ? false : requested);

  // The exchange-server holds no secret, so it cannot sign here. Send the action
  // unsigned and let the browser's replay fill the signature in — see
  // hyperliquid.api.ts `addSigningInterceptor`.
  private signActionOrDefer = async (action: any, nonce: number) => {
    if (!this.options.secret) return undefined;
    return await signL1Action(
      createWalletFromPrivateKey(this.options.secret),
      action,
      this.options.vault || null,
      nonce,
      !this.options.testnet,
    );
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    // The caller's ID goes on the main wire verbatim; otherwise we mint one.
    const cloid = this.resolveCallerClientOrderId(opts) ?? mintHyperliquidCloid();
    return this.placeOrderWithCloid(opts, cloid);
  };

  // `cloid` is the main wire's client order ID. `updateOrder` calls this directly
  // so it knows which cloid its replacement went out with.
  private placeOrderWithCloid = async (opts: PlaceOrderOpts, cloid: string): Promise<string[]> => {
    if (opts.type === OrderType.TrailingStopLoss) {
      throw new Error('Not supported');
    }

    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    if (opts.type === OrderType.Market) {
      //   {
      //     "action": {
      //         "grouping": "na",
      //         "orders": [
      //             {
      //                 "a": 98,
      //                 "b": false,
      //                 "p": "3.0384",
      //                 "r": false,
      //                 "s": "4",
      //                 "t": {
      //                     "limit": {
      //                         "tif": "FrontendMarket"
      //                     }
      //                 }
      //             }
      //         ],
      //         "type": "order"
      //     },
      //     "isFrontend": true,
      //     "nonce": 1732059038287,
      //     "signature": {
      //         "r": "0x20fc85f12a44a2f8354553677023215acd77006f72f8f6b84b8d07bd305ced13",
      //         "s": "0x5a4bd1858897836e0cd9b7f139ae376a80d59b0c05b4d051dfb37c703a7acd83",
      //         "v": 28
      //     },
      //     "vaultAddress": null
      // }
      const assetIndex = market?.info.hyperliquidId;

      const nAmount = market.precision.amount;

      // For market orders, use mark price (closer to oracle) to avoid "price too far from oracle" errors
      // This is especially important for HIP-3 markets with thin liquidity where last price can diverge significantly
      // Spot prefers `last`: only `allMids` is guaranteed to carry spot coins
      // and it updates `last`/`midPrice` only, so a spot ticker's `mark` is
      // frozen at the startup snapshot. Being truthy-but-stale, it would win
      // this `||` chain and anchor the +/-5% marketable band to an hours-old
      // price — a spot market order on a pair that has moved more than 5%
      // would rest as a limit instead of crossing. Perps keep mark-first
      // (closer to oracle, avoids "price too far from oracle" rejections).
      const basePrice = this.getOrderBasePrice(market, 'market order');

      const orderWire = orderRequestToOrderWire(
        {
          coin: symbolToCoin(market.symbol),
          is_buy: opts.side === OrderSide.Buy,
          sz: adjust(opts.amount, nAmount),
          limit_px: +this.priceToPrecision(market.symbol, basePrice * (opts.side === OrderSide.Buy ? 1.05 : 0.95)),
          order_type: {
            limit: {
              tif: 'Gtc' as any,
            },
          },
          reduce_only: this.resolveReduceOnly(market, Boolean(opts.reduceOnly)),
          cloid,
        },
        assetIndex,
      );

      // Get builder info if wallet is approved for builder fee
      const builderInfo = await this.getBuilderInfo();

      const action = orderWiresToOrderAction([orderWire], builderInfo);

      const nonce = this.generateNonce();
      const signature = await signL1Action(
        createWalletFromPrivateKey(this.options.secret),
        action,
        this.options.vault || null,
        nonce,
        !this.options.testnet,
      );

      const payload = { action, nonce, signature, ...(this.options.vault ? { vaultAddress: this.options.vault } : {}) };
      // A market order is a Gtc limit at 5% slippage; an unfilled remainder rests as one.
      this.recordPlacement([orderWire], market.symbol);
      const data = await this.postOrderAction(payload);

      const orderIds = this.validateResponseAndExtractOrderIds(data);

      // A market order is a Gtc limit at 5% slippage, so an unfilled remainder rests.
      if (orderIds[0]) this.rememberLimitOrderTimeInForce(orderIds[0], OrderTimeInForce.GoodTillCancel);

      return orderIds;
    } else if (opts.type === OrderType.Limit) {
      return await this.placeLimitOrder(opts, cloid);
    } else if (opts.type === OrderType.TakeProfit || opts.type === OrderType.StopLoss) {
      // Position TP/SL
      //   {
      //     "action": {
      //         "grouping": "positionTpsl",
      //         "orders": [
      //             {
      //                 "a": 5,
      //                 "b": false,
      //                 "p": "165.6",
      //                 "r": true,
      //                 "s": "0",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "sl",
      //                         "triggerPx": "180"
      //                     }
      //                 }
      //             },
      //             {
      //                 "a": 5,
      //                 "b": false,
      //                 "p": "239.2",
      //                 "r": true,
      //                 "s": "0",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "tp",
      //                         "triggerPx": "260"
      //                     }
      //                 }
      //             }
      //         ],
      //         "type": "order"
      //     },
      //     "isFrontend": true,
      //     "nonce": 1732047841729,
      //     "signature": {
      //         "r": "0x53976d9941a662557383848cf5424ad01f786cd42d144087cb677467a83f6df2",
      //         "s": "0x434a5f106f66fff488bd621ab2f77815cae25f69f1bda077b983ad723d5db96b",
      //         "v": 27
      //     },
      //     "vaultAddress": null
      // }

      // Single Position TP
      //   {
      //     "action": {
      //         "grouping": "positionTpsl",
      //         "orders": [
      //             {
      //                 "a": 0,
      //                 "b": false,
      //                 "p": "92000",
      //                 "r": true,
      //                 "s": "0",
      //                 "t": {
      //                     "trigger": {
      //                         "isMarket": true,
      //                         "tpsl": "tp",
      //                         "triggerPx": "100000"
      //                     }
      //                 }
      //             }
      //         ],
      //         "type": "order"
      //     },
      //     "isFrontend": true,
      //     "nonce": 1732048008175,
      //     "signature": {
      //         "r": "0x54ac036eb50cb71bd1910dd907ecf6d348b52c51d641cb04ae2dbf16386e9bec",
      //         "s": "0x3406757f696e8151512280aa80c205ea550fdf19259c7dbf3944c1f95bf7d055",
      //         "v": 28
      //     },
      //     "vaultAddress": null
      // }

      // Partial Position TP/SL (uses normalTpsl grouping for explicit size)
      //   {
      //     "grouping": "normalTpsl",
      //     "orders": [
      //         {
      //             "a": 98,
      //             "b": true,
      //             "p": "2.16",
      //             "r": true,
      //             "s": "1",
      //             "t": {
      //                 "trigger": {
      //                     "isMarket": true,
      //                     "tpsl": "tp",
      //                     "triggerPx": "2"
      //                 }
      //             }
      //         }
      //     ],
      //     "type": "order"
      // }
      const assetIndex = market?.info.hyperliquidId;

      // Use mark price for limit_px to avoid "price too far from oracle" errors
      // Spot prefers `last`: only `allMids` is guaranteed to carry spot coins
      // and it updates `last`/`midPrice` only, so a spot ticker's `mark` is
      // frozen at the startup snapshot. Being truthy-but-stale, it would win
      // this `||` chain and anchor the +/-5% marketable band to an hours-old
      // price — a spot market order on a pair that has moved more than 5%
      // would rest as a limit instead of crossing. Perps keep mark-first
      // (closer to oracle, avoids "price too far from oracle" rejections).
      const basePrice = this.getOrderBasePrice(market, `${opts.type} order`);

      // Adjust size to market precision (matches CCXT approach)
      const orderSize = opts.amount === 0 ? 0 : adjust(opts.amount, market.precision.amount);
      const usePositionTpsl = this.shouldUsePositionTpsl(opts, market, orderSize);
      const wireSize = usePositionTpsl ? 0 : orderSize;
      const triggerPrice = Number(opts.price);
      if (!isFinitePositiveNumber(triggerPrice)) {
        throw new Error(`Cannot place Hyperliquid ${opts.type} for ${market.symbol}: invalid trigger price`);
      }

      const orderWire = orderRequestToOrderWire(
        {
          coin: symbolToCoin(market.symbol),
          is_buy: opts.side === OrderSide.Buy,
          sz: wireSize,
          limit_px: +this.priceToPrecision(market.symbol, basePrice * (opts.side === OrderSide.Buy ? 1.05 : 0.95)),
          order_type: {
            trigger: {
              isMarket: true,
              tpsl: opts.type === OrderType.TakeProfit ? 'tp' : 'sl',
              triggerPx: +this.priceToPrecision(market.symbol, triggerPrice),
            },
          },
          reduce_only: this.resolveReduceOnly(market, Boolean(opts.reduceOnly)),
          cloid,
        },
        assetIndex,
      );
      // Get builder info if wallet is approved for builder fee
      const builderInfo = await this.getBuilderInfo();

      // Use positionTpsl for full position close (amount = 0 or explicit full-size reduce)
      // Use na grouping for standalone partial TP/SL orders with explicit size
      // Note: normalTpsl requires a main non-trigger order first, so we use na for standalone trigger orders
      const action = usePositionTpsl
        ? orderWiresToPositionalTPSLAction([orderWire], builderInfo)
        : orderWiresToOrderAction([orderWire], builderInfo);

      const nonce = this.generateNonce();
      const signature = await signL1Action(
        createWalletFromPrivateKey(this.options.secret),
        action,
        this.options.vault || null,
        nonce,
        !this.options.testnet,
      );

      const payload = { action, nonce, signature, ...(this.options.vault ? { vaultAddress: this.options.vault } : {}) };

      // Record BEFORE the REST call: the WS handler identifies this order by its
      // cloid — exact type, trigger price and reduce-only — even when the frame
      // beats the response, and even for a positional TP/SL, whose response
      // (`"waitingForTrigger"`) carries no oid at all.
      this.recordPlacement([orderWire], market.symbol);

      // Fallback for a frame that arrives without its cloid (see recordPlacement).
      // Core perps too, but only for a STOP ENTRY (non-reduce-only trigger):
      // its first `orderUpdates` frame carries no trigger fields and would
      // otherwise insert as a Limit at the slippage cap. Reduce-only stops
      // need no cache — the WS handler defers those to REST outright.
      if (market.id.includes(':') || !opts.reduceOnly) {
        const coin = symbolToCoin(market.symbol);
        this.addToPrePlacementCache(coin, opts.side, opts.type, opts.price || 0, opts.amount, cloid);
      }

      const data = await this.postOrderAction(payload);

      const orderIds = this.validateResponseAndExtractOrderIds(data);
      if (orderIds.length === 0 && this.responseHasDeferredOrderStatus(data)) {
        this.scheduleOrderRefresh();
      }

      // HIP-3 orders: Add optimistic order to store immediately
      // This prevents the 10-30 second delay waiting for Hyperliquid's REST API
      // WebSocket will skip this order (since it's already in store with correct type)
      if (market.id.includes(':')) {
        for (const orderId of orderIds) {
          // Add to shared pending cache so any WS handler instance can find it
          this.addPendingHip3Order(orderId, opts.type, opts.price || 0);

          // Add optimistic order with "Creating" status so UI shows immediate feedback
          // Status will be updated to "Open" when REST or WS confirms the order
          const optimisticOrder: Order = {
            id: orderId,
            status: OrderStatus.Creating,
            symbol: market.symbol,
            type: opts.type,
            side: opts.side,
            price: opts.price || 0, // triggerPx for TP/SL
            amount: opts.amount,
            filled: 0,
            remaining: opts.amount,
            reduceOnly: Boolean(opts.reduceOnly),
            lastUpdate: virtualClock.getCurrentTimeMs(),
          };
          this.store.addOrUpdateOrder(optimisticOrder);
        }

        // Still refresh to get any additional fields from REST when available
        this.refreshHip3OrdersWithRetry(orderIds);
      }

      return orderIds;
    } else {
      throw new Error(`Unknown order type ${opts.type}`);
    }
  };

  // Limit Cancel
  //   {
  //     "action": {
  //         "cancels": [
  //             {
  //                 "a": 98,
  //                 "o": 48582502044
  //             }
  //         ],
  //         "type": "cancel"
  //     },
  //     "isFrontend": true,
  //     "nonce": 1732048930058,
  //     "signature": {
  //         "r": "0xcf986988d160b2f2377e065fcc491956a9fad5e76cb9c28ff8ca66c57489b7a0",
  //         "s": "0x6ce55cd0deb7e7de466afb97728b17434966f459779ab653ab1a86e16e33ad4b",
  //         "v": 28
  //     },
  //     "vaultAddress": null
  // }

  // TP/SL Cancel
  //   {
  //     "action": {
  //         "cancels": [
  //             {
  //                 "a": 98,
  //                 "o": 48371398850
  //             }
  //         ],
  //         "type": "cancel"
  //     },
  //     "isFrontend": true,
  //     "nonce": 1732048959055,
  //     "signature": {
  //         "r": "0x662415bab5157fe08d4a2ec03e6ad22d42df4fac4fecc2e2ef650a6e87aefa87",
  //         "s": "0x2098eebc76e5cde9e05521dc28c0951405029b91de304528d67cf18f7f6c9c6f",
  //         "v": 28
  //     },
  //     "vaultAddress": null
  // }

  cancelOrders = async (orders: Order[]) => {
    // Use O(1) hashmap lookup instead of O(n) find in loop
    const marketsBySymbol = this.store.marketsBySymbol;
    const cancels = orders.map((order) => {
      const market = marketsBySymbol[order.symbol];
      return { a: +market?.info.hyperliquidId, o: +order.id };
    });

    const action = {
      type: 'cancel',
      cancels,
    };

    const nonce = this.generateNonce();
    const signature = await signL1Action(
      createWalletFromPrivateKey(this.options.secret),
      action,
      this.options.vault || null,
      nonce,
      !this.options.testnet,
    );

    const payload = { action, nonce, signature, ...(this.options.vault ? { vaultAddress: this.options.vault } : {}) };
    try {
      await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
        headers: {
          'Content-Type': 'application/json',
        },
      });
    } catch (error) {
      // Venue already canceled/filled these oids (typical after a market flatten
      // auto-cancels positional TP/SL). Local cleanup below still applies.
      const message = error instanceof Error ? error.message : String(error);
      if (!isOnlyHyperliquidTerminalCancelError(message)) {
        throw error;
      }
    }

    // Optimistic removal: suppress cancelled orders from being re-added by stale WS snapshots.
    // REST backfill will be the source of truth — if the cancel somehow failed, fetchOrders()
    // will un-suppress and re-add the order after the trust window expires.
    for (const order of orders) {
      this.suppressCancelledOrder(order.id);
    }

    // Clear pre-placement cache entries for cancelled orders' coin+side combinations.
    // Without this, stale pre-placement entries accumulate and can poison future
    // matchPrePlacement calls — e.g., after cancel-all + immediate re-place,
    // the old SL entry could be matched instead of the new TP entry.
    const clearedCoinSides = new Set<string>();
    for (const order of orders) {
      const market = this.store.marketsBySymbol[order.symbol];
      if (market?.id.includes(':')) {
        const coin = symbolToCoin(order.symbol);
        const key = `${coin}|${order.side}`;
        if (!clearedCoinSides.has(key)) {
          clearedCoinSides.add(key);
          getCrossCache().clearPrePlacement(coin, order.side);
        }
      }
    }

    this.store.removeOrders(orders.map((o) => ({ id: o.id })));
  };

  /**
   * Schedule Hyperliquid's account-wide dead man's switch at an absolute UTC
   * millisecond deadline. Passing null/undefined removes an existing schedule.
   * Lifecycle ownership stays with the caller: an automated client should
   * periodically move the deadline forward while it remains healthy.
   */
  scheduleCancel = async (time?: number | null): Promise<void> => {
    const action: ScheduleCancelAction = {
      type: 'scheduleCancel',
    };

    if (time !== null && time !== undefined) {
      if (!Number.isSafeInteger(time)) {
        throw new Error('Hyperliquid scheduleCancel time must be a safe integer UTC timestamp in milliseconds');
      }

      const minimumDeadline = virtualClock.getCurrentTimeMs() + 5_000;
      if (time < minimumDeadline) {
        throw new Error('Hyperliquid scheduleCancel time must be at least 5 seconds in the future');
      }
      action.time = time;
    }

    const nonce = this.generateNonce();
    const signature = await this.signActionOrDefer(action, nonce);
    const payload = {
      action,
      nonce,
      ...(signature ? { signature } : {}),
      ...(this.options.vault ? { vaultAddress: this.options.vault } : {}),
    };

    await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
      headers: {
        'Content-Type': 'application/json',
      },
    });
  };

  public _deadOrders = new Set<string>();

  // Cancel+replace. The replacement gets a FRESH cloid (a caller's ID stays on
  // the cancelled order — packages/safe-cex/CLAUDE.md, "Client order ID
  // contract") and the result carries it as `clientOrderId`: a positional
  // TP/SL replacement answers `"waitingForTrigger"` with no oid, and the cloid
  // is then the only thing that identifies it.
  updateOrder = async ({ order, update }: UpdateOrderOpts): Promise<ReplacementOrderIds> => {
    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
      // Cancel+replace rebuilds the wire from scratch, so without this an Alo
      // order silently comes back as Gtc after a price edit.
      timeInForce: order.timeInForce,
    };

    if ('tpTriggerType' in order) newOrder.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) newOrder.slTriggerType = order.slTriggerType;

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    // Re-evaluate TP/SL order type based on new price vs current market price
    // This prevents accidental position closure when dragging TP/SL past current price
    if (order.type === OrderType.TakeProfit || order.type === OrderType.StopLoss) {
      const ticker = this.store.tickersBySymbol[order.symbol];
      const markPrice = Number(ticker?.mark);
      const lastPrice = Number(ticker?.last);
      const indexPrice = Number(ticker?.index);
      const newTriggerPrice = Number(newOrder.price);
      const referencePriceCandidates = [markPrice, lastPrice, indexPrice].filter(
        (price): price is number => Number.isFinite(price) && price > 0,
      );

      if (referencePriceCandidates.length > 0 && Number.isFinite(newTriggerPrice) && newTriggerPrice > 0) {
        // Prefer mark for classification (closest to trigger logic), then last/index fallback.
        const [referencePrice, ...secondaryPrices] = referencePriceCandidates;
        const maxSecondaryDivergence = secondaryPrices.reduce((max, price) => {
          const divergence = Math.abs(price - referencePrice) / referencePrice;
          return Math.max(max, divergence);
        }, 0);

        // If ticker sources disagree heavily, keep user-selected order type rather than
        // guessing and potentially flipping SL -> TP (which can trigger immediately).
        if (maxSecondaryDivergence > 0.25) {
          this.log(
            `Skipping TP/SL type re-evaluation for ${order.symbol}: inconsistent reference prices (mark=${markPrice}, last=${lastPrice}, index=${indexPrice})`,
            LogSeverity.Warning,
          );
        } else {
          // For SELL orders (closing long): TP is above price, SL is below price
          // For BUY orders (closing short): TP is below price, SL is above price
          const isSell = order.side === OrderSide.Sell;
          const newPriceAboveMarket = newTriggerPrice > referencePrice;

          // Determine correct order type based on price position relative to market
          // SELL + price above market = Take Profit (price rises to target)
          // SELL + price below market = Stop Loss (price falls to stop)
          // BUY + price below market = Take Profit (price falls to target)
          // BUY + price above market = Stop Loss (price rises to stop)
          const shouldBeTP = isSell ? newPriceAboveMarket : !newPriceAboveMarket;
          const projectedType = shouldBeTP ? OrderType.TakeProfit : OrderType.StopLoss;

          // Safety guard: only auto-switch TP -> SL.
          // We intentionally do not auto-switch SL -> TP because a stale/lagging
          // reference price can otherwise flip a valid stop into TP and close
          // the position immediately. TP -> SL still prevents the known issue
          // where dragging TP through market would otherwise trigger instantly.
          if (order.type === OrderType.TakeProfit && projectedType === OrderType.StopLoss) {
            newOrder.type = OrderType.StopLoss;
            this.log(
              `Order type changed from ${order.type} to ${newOrder.type} based on new price ${newTriggerPrice} vs market ${referencePrice}`,
              LogSeverity.Info,
            );
          } else if (order.type === OrderType.StopLoss && projectedType === OrderType.TakeProfit) {
            this.log(
              `Skipping SL->TP auto-switch for ${order.symbol} at trigger ${newTriggerPrice}; keeping stop-loss type`,
              LogSeverity.Warning,
            );
          }
        }
      }
    }

    // For HIP-3 orders, immediately show a placeholder at the new position with "Updating" status
    // This provides visual feedback during the cancel+replace operation
    const market = this.store.marketsBySymbol[order.symbol];
    const isHip3 = market?.id.includes(':');
    if (isHip3) {
      const updatingOrder: Order = {
        id: order.id,
        status: OrderStatus.Updating,
        symbol: order.symbol,
        type: newOrder.type,
        side: order.side,
        price: newOrder.price || order.price,
        amount: newOrder.amount || order.amount,
        filled: 0,
        remaining: newOrder.amount || order.amount,
        reduceOnly: Boolean(newOrder.reduceOnly),
        lastUpdate: virtualClock.getCurrentTimeMs(),
      };
      this.store.addOrUpdateOrder(updatingOrder);
    }

    await this.cancelOrders([order]);

    this._deadOrders.add(order.id);

    const cloid = mintHyperliquidCloid();
    return withReplacementClientOrderId(await this.placeOrderWithCloid(newOrder, cloid), cloid);
  };

  priceToPrecision = (symbol: string, price: any): string => {
    const priceStr = numberToString(price);
    if (!priceStr) {
      return '';
    }

    const integerPart = priceStr.split('.')[0];
    const significantDigits = Math.max(5, integerPart.length);
    const result = decimalToPrecision(price, ROUND, significantDigits, SIGNIFICANT_DIGITS, NO_PADDING);
    // Use O(1) hashmap lookup instead of O(n) find
    const market = this.store.marketsBySymbol[symbol];
    // Hyperliquid accepts 8 price decimals on spot but only 6 on perps.
    // Sending a perp-capped price for a spot pair silently truncates the
    // order price, so this must be per-market now that both live in one store.
    const maxDecimals = market?.isSpot ? SPOT_MAX_PRICE_DECIMALS : PERP_MAX_PRICE_DECIMALS;
    const prAmount = market?.precision.amount;
    const subtractedValue = maxDecimals - precisionFromString(prAmount ? String(prAmount) : '1');
    return decimalToPrecision(result, ROUND, subtractedValue, DECIMAL_PLACES, NO_PADDING);
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    this.assertCallerClientOrderIds(orders);
    const orderIds = await mapSeries(orders, (order) => this.placeOrder(order));
    return orderIds.flat();
  };
}
