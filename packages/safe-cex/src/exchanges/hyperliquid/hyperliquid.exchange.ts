import type { Axios } from 'axios';
import type { Store } from '../../store/store.interface';
import type { BuilderInfo } from './hyperliquid.types';

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
  CoinBalance,
  ExchangeName,
  ExchangeOptions,
  Execution,
  ExecutionsOptions,
  LogSeverity,
  MarginType,
  Market,
  OHLCVOptions,
  Order,
  OrderBook,
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
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { createWalletFromPrivateKey } from '../../utils/ethereum/createWalletFromPrivateKey';
import { validateWalletAndPrivateKey } from '../../utils/ethereum/validateWalletAndPrivateKey';
import { v } from '../../utils/get-key';
import { loop } from '../../utils/loop';
import { adjust } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createExchangeAPI, createInfoAPI } from './hyperliquid.api';
import { checkBuilderFeeApproved } from './hyperliquid.builderFee';
import { getBuilderAddress, TEALSTREET_BUILDER_FEE } from './hyperliquid.constants';
import { getCrossCache, hasSeenFill, markFillSeen } from './hyperliquid.cross-cache';
import {
  fetchCoreMetaAndAssetCtxsWithFallback,
  fetchHip3MetaAndAssetCtxsWithRetry,
} from './hyperliquid.market-startup-helpers';
import { inferHyperliquidOrderTypeAndPrice, parseHyperliquidOrderTimeInForce } from './hyperliquid.order-inference';
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
  PerpDex,
  PerpDexs,
  SpotClearinghouseState,
} from './hyperliquid.types';
import { calculatePricePrecision, symbolToCoin } from './hyperliquid.utils';
import { HyperliquidPrivateWebSocket } from './hyperliquid.ws-private';
import { HyperliquidPublicWebSocket } from './hyperliquid.ws-public';

// Export fill deduplication functions for use by WebSocket handlers
export { hasSeenFill, markFillSeen };

export class HyperliquidExchange extends BaseExchange {
  xhr: Axios; // For /exchange endpoint (trading operations)
  infoXhr: Axios; // For /info endpoint (read-only operations)
  unlimitedXHR: Axios;

  public publicWebsocket: HyperliquidPublicWebSocket;
  public privateWebsocket: HyperliquidPrivateWebSocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  private lastNonce: number = 0;
  private static readonly BUILDER_FEE_APPROVAL_TIMEOUT_MS = 8_000;
  private static readonly HIP3_DEX_DISCOVERY_TIMEOUT_MS = 6_000;
  private static readonly HIP3_MARKETS_LOAD_TIMEOUT_MS = 8_000;
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
  public addToPrePlacementCache(coin: string, side: OrderSide, type: OrderType, price: number, amount: number): void {
    getCrossCache().addPrePlacement(coin, side, type, price, amount);
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

  // Cache for fake positions - invalidated when markets change
  private _fakePositionsCache: Position[] | null = null;

  // Builder fee approval status: null = unknown, true = approved, false = not approved
  private _builderFeeApproved: boolean | null = null;

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
   * Mark an order as recently cancelled. Stale WS messages will be suppressed
   * until REST backfill confirms the cancellation or the suppression expires.
   */
  private suppressCancelledOrder(orderId: string): void {
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

  public shouldRequestPortfolioMarginSpotState = (): boolean =>
    this.userAbstractionMode === 'portfolioMargin';

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

    const ticker =
      this.store.tickersById[normalizedCoin] ||
      this.store.tickersById[upperCoin] ||
      this.store.tickersBySymbol[`${upperCoin}-PERP`];
    const markPrice = Number(ticker?.mark ?? ticker?.last ?? ticker?.index);
    return Number.isFinite(markPrice) && markPrice > 0 ? markPrice : 0;
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
    this.store.update({
      balance: {
        total: accountValue - upnl,
        used,
        free: accountValue - used,
        upnl,
        info: {
          userAbstractionMode: this.userAbstractionMode,
          source: 'clearinghouseState',
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
    if (!this.shouldUseSpotCollateralBalance()) {
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
    if (!this.shouldUseSpotCollateralBalance()) {
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
      return;
    }

    try {
      const result = await this.withStartupTimeout(
        checkBuilderFeeApproved(
          this.options.applicationId,
          getBuilderAddress(this.options.testnet ?? false),
          this.options.testnet ?? false,
        ),
        HyperliquidExchange.BUILDER_FEE_APPROVAL_TIMEOUT_MS,
        'Builder fee approval check',
        { approved: false },
      );
      this._builderFeeApproved = result.approved;
      this.log(
        `Builder fee approval status: ${result.approved ? 'approved' : 'not approved'}`,
        result.approved ? LogSeverity.Info : LogSeverity.Debug,
      );
    } catch (err) {
      this.log(`Failed to check builder fee approval: ${err}`, LogSeverity.Warning);
      this._builderFeeApproved = null; // Keep as unknown on error
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

    // Return builder info only if approved
    if (this._builderFeeApproved) {
      return {
        b: getBuilderAddress(this.options.testnet ?? false).toLowerCase(),
        f: TEALSTREET_BUILDER_FEE,
      };
    }

    return undefined;
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
      if (this.shouldUseSpotCollateralBalance()) {
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
    try {
      const { data } = await this.infoXhr.post(
        ENDPOINTS.INFO,
        {
          type: 'userFees',
          user: this.options.key,
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
        const prevDayPx = parseFloat(v(sourceTicker, 'prevDayPx'));
        const volume = parseFloat(v(sourceTicker, 'dayNtlVlm'));

        tickers.push({
          id,
          symbol,
          bid: 0,
          ask: 0,
          last: price,
          mark: parseFloat(v(sourceTicker, 'markPx')),
          index: parseFloat(v(sourceTicker, 'oraclePx')),
          midPrice: price,
          percentage: ((price - prevDayPx) / prevDayPx) * 100,
          fundingRate: parseFloat(v(sourceTicker, 'funding')),
          fundingTime: 0,
          volume,
          quoteVolume: volume * price,
          openInterest: 0,
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
  ): Promise<{ markets: Market[]; tickers: Ticker[] }> => {
    const parsedResponse = await fetchHip3MetaAndAssetCtxsWithRetry({
      xhr: this.xhr,
      dexName,
      log: (message, severity = LogSeverity.Info) => this.log(message, severity),
    });
    if (!parsedResponse) {
      return { markets: [], tickers: [] };
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

          const prevDayPx = parseFloat(v(sourceTicker, 'prevDayPx'));
          const volume = parseFloat(v(sourceTicker, 'dayNtlVlm'));

          tickers.push({
            id,
            symbol,
            bid: 0,
            ask: 0,
            last: price,
            mark: parseFloat(v(sourceTicker, 'markPx')),
            index: parseFloat(v(sourceTicker, 'oraclePx')),
            midPrice: price,
            percentage: ((price - prevDayPx) / prevDayPx) * 100,
            fundingRate: parseFloat(v(sourceTicker, 'funding')),
            fundingTime: 0,
            volume,
            quoteVolume: volume * price,
            openInterest: 0,
            info: sourceTicker,
          });
        } catch (e) {
          this.log(`Error processing HIP-3 market ${rawName}: ${e}`, LogSeverity.Error);
        }
      }

      return { markets, tickers };
    } catch (error) {
      this.log(`Failed to fetch HIP-3 markets for ${dexName}: ${error}`, LogSeverity.Warning);
      return { markets: [], tickers: [] };
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
    const [coreMetaAndAssets, fees, hip3Dexs] = await Promise.all([
      fetchCoreMetaAndAssetCtxs(),
      this.fetchFee(),
      this.withStartupTimeout(
        this.fetchHip3Dexs(),
        HyperliquidExchange.HIP3_DEX_DISCOVERY_TIMEOUT_MS,
        'HIP-3 DEX discovery',
        [],
      ),
    ]);

    const [meta, assets] = coreMetaAndAssets;
    const { markets, tickers } = this.fetchMarketsHelper(meta, assets, fees);

    // Fetch HIP-3 markets with a strict startup budget.
    // Avoid spawning a large parallel burst that keeps the shared rate-limited queue busy
    // long after timeout fallback has already chosen to continue with core perps only.
    if (hip3Dexs.length > 0) {
      const discoveredHip3DexNames = hip3Dexs.map((d) => d.name);
      const hip3Results: Array<{ markets: Market[]; tickers: Ticker[] }> = [];
      const hip3Deadline = Date.now() + HyperliquidExchange.HIP3_MARKETS_LOAD_TIMEOUT_MS;
      let hip3TimedOut = false;

      for (const dex of hip3Dexs) {
        const remainingBudgetMs = hip3Deadline - Date.now();
        if (remainingBudgetMs <= 0) {
          hip3TimedOut = true;
          break;
        }

        const result = await this.withStartupTimeout<{ markets: Market[]; tickers: Ticker[] } | null>(
          this.fetchHip3Markets(dex.name, dex.dexPosition, fees),
          remainingBudgetMs,
          `HIP-3 markets loading (${dex.name})`,
          null,
        );

        if (!result) {
          hip3TimedOut = true;
          break;
        }

        hip3Results.push(result);
      }

      // If startup timed out before HIP-3 market metadata completed, continue with core perps only.
      if (!hip3TimedOut && hip3Results.length === hip3Dexs.length) {
        this.hip3DexNames = discoveredHip3DexNames;
        this.log(`Found ${hip3Dexs.length} HIP-3 DEXes: ${this.hip3DexNames.join(', ')}`, LogSeverity.Info);

        for (const result of hip3Results) {
          markets.push(...result.markets);
          tickers.push(...result.tickers);
        }

        this.log(`Loaded ${markets.length} total markets (including HIP-3)`, LogSeverity.Info);
      } else {
        this.hip3DexNames = [];
        this.log(
          `HIP-3 market metadata did not finish within startup budget (${hip3Results.length}/${hip3Dexs.length} dexes loaded); continuing with core perps only`,
          LogSeverity.Warning,
        );
      }
    }

    return [markets, tickers];
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
  private validateResponseAndExtractOrderIds(data: any): string[] {
    if (!data?.response?.data?.statuses) {
      throw new Error(`Invalid response from Hyperliquid API: ${JSON.stringify(data)}`);
    }

    const errors = data.response.data.statuses.filter((status: any) => status?.error);
    if (errors.length > 0) {
      throw new Error(`Hyperliquid order error: ${errors.map((e: any) => e.error).join(', ')}`);
    }

    return data.response.data.statuses
      .map((status: any) => {
        const oid = status?.resting?.oid || status?.filled?.oid;
        return oid !== undefined ? String(oid) : undefined;
      })
      .filter((oid: string | undefined): oid is string => oid !== undefined);
  }

  private generateFakePositionsFromMarkets = (markets: Market[]) => {
    const fakePositions = markets.map((market) => {
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

    try {
      // Fetch core perp orders (without dex param)
      const { data: coreOrders } = await this.xhr.post(
        ENDPOINTS.INFO,
        { type: INFO_TYPES.FRONTEND_OPEN_ORDERS, user },
        { headers: { 'Content-Type': 'application/json' } },
      );

      if (Array.isArray(coreOrders)) {
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
      const fetchedOrderIds = new Set(orders.map((o) => o.id));
      const currentOrders = [...this.store.orders]; // Snapshot the CURRENT array (after WS updates)
      const currentOrdersById = new Map(currentOrders.map((o) => [o.id, o]));
      const now = Date.now();

      // 1. Add new orders and update changed orders (one at a time, no array replacement)
      for (const newOrder of orders) {
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
          existing.timeInForce !== newOrder.timeInForce
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
        if (!fetchedOrderIds.has(cancelledId)) {
          this._cancelledOrders.delete(cancelledId);
        }
      }

      return orders;
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
    const marketsById = this.store.marketsById;
    return openOrders
      .filter((openOrder) => {
        const oid = `${openOrder.oid}`;
        // Filter out dead orders (from updateOrder cancel+replace) — stringify to match Set<string>
        if (this._deadOrders.has(oid)) return false;
        // Filter out recently cancelled orders (suppressed until REST confirms)
        if (this.isOrderCancelled(oid)) return false;
        return true;
      })
      .map((openOrder): Order | null => {
        // Try exact match first, then case-insensitive for HIP-3 markets (O(1) map lookup)
        let market: Market | undefined = marketsById[openOrder.coin];

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

        return {
          id: `${openOrder.oid}`,
          status: OrderStatus.Open,
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
          info: openOrder,
        };
      })
      .filter((order): order is Order => order !== null);
  };

  fetchExecutions = async (opts: ExecutionsOptions): Promise<Execution[]> => {
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
        `fetchExecutions capped to last 30 days for ${symbol} (requestedFrom=${requestedFromMilliseconds}, cappedFrom=${effectiveFromMilliseconds})`,
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
      this.log('Skipping fetchExecutions: applicationId missing', LogSeverity.Warning);
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
    const coin = market.id;
    const activeAssetData = this.activeAssetData.get(coin);
    const timestamp = virtualClock.getCurrentTimeMs();
    const action = {
      type: 'updateLeverage',
      asset: market.info.hyperliquidId,
      isCross: activeAssetData?.leverage.type === 'cross',
      leverage: +inputLeverage,
    };
    const signature = await signL1Action(
      createWalletFromPrivateKey(this.options.secret),
      action,
      this.options.vault || null,
      timestamp,
      !this.options.testnet,
    );
    await this.xhr.post(
      ENDPOINTS.EXCHANGE,
      {
        action,
        nonce: timestamp,
        signature,
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
    const coin = market.id;
    const activeAssetData = this.activeAssetData.get(coin);
    const timestamp = virtualClock.getCurrentTimeMs();
    const action = {
      type: 'updateLeverage',
      asset: market.info.hyperliquidId,
      isCross: !isolated,
      leverage: activeAssetData?.leverage.value,
    };
    const signature = await signL1Action(
      createWalletFromPrivateKey(this.options.secret),
      action,
      this.options.vault || null,
      timestamp,
      !this.options.testnet,
    );
    await this.xhr.post(
      ENDPOINTS.EXCHANGE,
      {
        action,
        nonce: timestamp,
        signature,
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

  createLimitOrderWires = (opts: PlaceOrderOpts) => {
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
        reduce_only: Boolean(opts.reduceOnly),
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
            reduce_only: true,
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
            reduce_only: true,
          },
          assetIndex,
        ),
      );
    }

    return orderWires;
  };

  placeLimitOrder = async (opts: PlaceOrderOpts) => {
    if (opts.type !== OrderType.Limit) {
      throw new Error('Not supported');
    }

    const orderWires = this.createLimitOrderWires(opts);

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

    // Add to pre-placement cache BEFORE REST call so WS handler can match this order
    const market = this.store.marketsBySymbol[opts.symbol];
    if (market?.id.includes(':')) {
      const coin = symbolToCoin(market.symbol);
      this.addToPrePlacementCache(coin, opts.side, OrderType.Limit, opts.price || 0, opts.amount);
    }

    const { data } = await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
      headers: {
        'Content-Type': 'application/json',
      },
    });

    const orderIds = this.validateResponseAndExtractOrderIds(data);

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

  placeOrder = async (opts: PlaceOrderOpts) => {
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

      // Use O(1) hashmap lookup instead of O(n) find
      const ticker = this.store.tickersBySymbol[market.symbol];
      const nAmount = market.precision.amount;

      // For market orders, use mark price (closer to oracle) to avoid "price too far from oracle" errors
      // This is especially important for HIP-3 markets with thin liquidity where last price can diverge significantly
      const basePrice = ticker?.mark || ticker?.last || 0;

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
          reduce_only: Boolean(opts.reduceOnly),
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
      const { data } = await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
        headers: {
          'Content-Type': 'application/json',
        },
      });

      const orderIds = this.validateResponseAndExtractOrderIds(data);

      return orderIds;
    } else if (opts.type === OrderType.Limit) {
      return await this.placeLimitOrder(opts);
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

      // Use O(1) hashmap lookup instead of O(n) find
      const ticker = this.store.tickersBySymbol[market.symbol];

      // Use mark price for limit_px to avoid "price too far from oracle" errors
      const basePrice = ticker?.mark || ticker?.last || 0;

      // Adjust size to market precision (matches CCXT approach)
      const orderSize = opts.amount === 0 ? 0 : adjust(opts.amount, market.precision.amount);

      const orderWire = orderRequestToOrderWire(
        {
          coin: symbolToCoin(market.symbol),
          is_buy: opts.side === OrderSide.Buy,
          sz: orderSize,
          limit_px: +this.priceToPrecision(market.symbol, basePrice * (opts.side === OrderSide.Buy ? 1.05 : 0.95)),
          order_type: {
            trigger: {
              isMarket: true,
              tpsl: opts.type === OrderType.TakeProfit ? 'tp' : 'sl',
              triggerPx: +this.priceToPrecision(market.symbol, opts.price || 0),
            },
          },
          reduce_only: Boolean(opts.reduceOnly),
        },
        assetIndex,
      );
      // Get builder info if wallet is approved for builder fee
      const builderInfo = await this.getBuilderInfo();

      // Use positionTpsl for full position close (amount = 0)
      // Use na grouping for standalone partial TP/SL orders with explicit size
      // Note: normalTpsl requires a main non-trigger order first, so we use na for standalone trigger orders
      const action =
        opts.amount > 0
          ? orderWiresToOrderAction([orderWire], builderInfo)
          : orderWiresToPositionalTPSLAction([orderWire], builderInfo);

      const nonce = this.generateNonce();
      const signature = await signL1Action(
        createWalletFromPrivateKey(this.options.secret),
        action,
        this.options.vault || null,
        nonce,
        !this.options.testnet,
      );

      const payload = { action, nonce, signature, ...(this.options.vault ? { vaultAddress: this.options.vault } : {}) };

      // Add to pre-placement cache BEFORE REST call so WS handler can match this order
      // even if WS arrives before REST returns
      if (market.id.includes(':')) {
        const coin = symbolToCoin(market.symbol);
        this.addToPrePlacementCache(coin, opts.side, opts.type, opts.price || 0, opts.amount);
      }

      const { data } = await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
        headers: {
          'Content-Type': 'application/json',
        },
      });

      const orderIds = this.validateResponseAndExtractOrderIds(data);

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
    await this.xhr.post(ENDPOINTS.EXCHANGE, payload, {
      headers: {
        'Content-Type': 'application/json',
      },
    });

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

  public _deadOrders = new Set<string>();

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
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

    return await this.placeOrder(newOrder);
  };

  priceToPrecision = (symbol: string, price: any): string => {
    const priceStr = numberToString(price);
    if (!priceStr) {
      return '';
    }

    const integerPart = priceStr.split('.')[0];
    const significantDigits = Math.max(5, integerPart.length);
    const result = decimalToPrecision(price, ROUND, significantDigits, SIGNIFICANT_DIGITS, NO_PADDING);
    const maxDecimals = 6;
    // Use O(1) hashmap lookup instead of O(n) find
    const prAmount = this.store.marketsBySymbol[symbol]?.precision.amount;
    const subtractedValue = maxDecimals - precisionFromString(prAmount ? String(prAmount) : '1');
    return decimalToPrecision(result, ROUND, subtractedValue, DECIMAL_PLACES, NO_PADDING);
  };

  placeOrders = async (orders: PlaceOrderOpts[]) => {
    const orderIds = await mapSeries(orders, (order) => this.placeOrder(order));
    return orderIds.flat();
  };
}
