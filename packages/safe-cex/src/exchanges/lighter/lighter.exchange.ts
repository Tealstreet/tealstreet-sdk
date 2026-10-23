import type { ManipulateType } from 'dayjs';
import type { Store } from '../../store/store.interface';
import type { ExchangeName } from '../../types';
import type { LighterAccountTier } from './lighter.account-tier';
import type { LighterBuilderFeeConfig } from './lighter.builder-fee';
import type { LighterCreateGroupedOrdersRequest, LighterCreateOrderRequest, LighterSignerAdapter } from './lighter.signer';

import dayjs from 'dayjs';
import chunk from 'lodash/chunk';
import orderBy from 'lodash/orderBy';
import uniq from 'lodash/uniq';
import uniqBy from 'lodash/uniqBy';
import { forEachSeries } from 'p-iteration';

import { CandlestickApi, LighterApiClient, OrderApi } from '@tealstreet/lighter-sdk';
import { getCryptoApi, initWasm, isWasmReady } from '@tealstreet/poseidon-crypto';
import { sleep } from '@tealstreet/sleep/src/sleep';
import {
  Balance,
  Candle,
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
  UpdateOrderOpts,
  Writable,
} from '../../types';
import { createPositionId } from '../../utils/createPositionId';
import { v } from '../../utils/get-key';
import { loop } from '../../utils/loop';
import { mergeUpdatesUsingLastUpdate } from '../../utils/mergeUpdatesUsingLastUpdate';
import { adjust, subtract } from '../../utils/safe-math';
import { virtualClock } from '../../utils/virtualClock';
import { BaseExchange } from '../base';
import { createLighterSDKClient } from './lighter.api';
import {
  extractLighterAccountTierFromPayload,
  hasNonZeroLighterIntegratorFee,
  isLighterAccountTierEligibleForPartnerAttribution,
  isLighterPartnerAttributionApprovalMissingRejection,
  isLighterPartnerAttributionTierRejection,
  LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE,
  LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE,
} from './lighter.account-tier';
import { resolveLighterBuilderFeeConfig } from './lighter.builder-fee';
import { createLighterSignerAdapter } from './lighter.signer';
import {
  ENDPOINTS,
  GRAFANA_ENDPOINTS_EVERY,
  GRAFANA_ENDPOINTS_PERIODIC,
  LighterDeploymentProfile,
  ORDER_SIDE,
  ORDER_STATUS,
  ORDER_TIME_IN_FORCE,
  ORDER_TYPE,
  resolveLighterDeploymentProfile,
  resolveLighterInterval,
} from './lighter.types';
import {
  getLighterAmountDecimals,
  getLighterOrderMinimums,
  getLighterPriceDecimals,
  isLighterL1Address,
  parseLighterApiKeyIndex,
  parseLighterPositiveIntegerString,
  resolveLighterCancelTargets,
  toLighterApiKeyIndexString,
  toLighterScaledInt,
} from './lighter.utils';
import { LighterPrivateWebsocket } from './lighter.ws-private';
import { LighterPublicWebsocket } from './lighter.ws-public';

const INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY = 1_000;
const LIGHTER_OHLCV_POLL_INTERVAL_MS = 5_000;
const LIGHTER_OHLCV_WS_STALE_MS = 120_000;
const LIGHTER_OHLCV_FALLBACK_403_COOLDOWN_MS = 60_000;
const LIGHTER_ACCOUNT_LIMITS_CACHE_MS = 5 * 60_000;
const LIGHTER_ACCOUNT_LIMITS_RETRY_MS = 60_000;
const LIGHTER_ACCOUNT_SNAPSHOT_REUSE_MS = 1_000;
const LIGHTER_ORDER_RECONCILIATION_COOLDOWN_MS = 3_000;
const LIGHTER_INITIAL_TICK_MAX_DELAY_MS = 2_000;
const LIGHTER_TRANSIENT_SNAPSHOT_WARNING_COOLDOWN_MS = 60_000;
const LIGHTER_DEFAULT_QUOTE = 'USDC';
const LIGHTER_KNOWN_QUOTES = ['USDT', 'USDC', 'USDG'];
const LIGHTER_MARGIN_FRACTION_TICK = 10_000;
const LIGHTER_DEFAULT_STOP_SLIPPAGE_PCT = 1;
const LIGHTER_MAX_STOP_SLIPPAGE_PCT = 50;
const LIGHTER_STANDARD_RATE_LIMIT_GUIDANCE_MESSAGE =
  'Lighter Standard account rate limit temporarily active. Standard accounts have very small API limits; please wait before retrying or switch the Lighter account tier to Plus or Premium if this persists.';
const LIGHTER_ACCOUNT_RATE_LIMIT_GUIDANCE_MESSAGE =
  'Lighter rate limit temporarily active for this account/session. Please wait before retrying.';
const LIGHTER_UNKNOWN_TIER_RATE_LIMIT_GUIDANCE_MESSAGE =
  'Lighter rate limit temporarily active for this account/session. Please wait before retrying; if this persists, check your Lighter account tier and connectivity route.';

type LighterPlacedOrderHint = {
  type: OrderType;
  side: OrderSide;
  reduceOnly: boolean;
  timeInForce?: OrderTimeInForce;
  price: number;
  symbol: string;
  createdAt: number;
};

type LighterAccountLookupParams = {
  by: 'index' | 'l1_address';
  value: string;
};

const parseLighterMarketSymbol = (
  symbol: string,
  defaultQuote: LighterDeploymentProfile['quoteCurrency'] = LIGHTER_DEFAULT_QUOTE,
): { base: string; quote: string } => {
  if (symbol.includes('/')) {
    const [base, quote] = symbol.split('/');
    return { base, quote: quote || defaultQuote };
  }

  const quote = LIGHTER_KNOWN_QUOTES.find((knownQuote) => symbol.endsWith(knownQuote));
  if (quote) {
    return { base: symbol.slice(0, -quote.length), quote };
  }

  return { base: symbol, quote: defaultQuote };
};

const isLighterPerpMarket = (market: Record<string, any>): boolean => {
  const marketType = String(market.market_type ?? market.marketType ?? '').toLowerCase();
  return market.status === 'active' && marketType !== 'spot';
};

const getLighterMarketLeverageMax = (market: Record<string, any>): number => {
  const marginFraction = Number.parseFloat(
    String(
      market.min_initial_margin_fraction ??
        market.minInitialMarginFraction ??
        market.default_initial_margin_fraction ??
        market.defaultInitialMarginFraction ??
        '',
    ),
  );

  if (!Number.isFinite(marginFraction) || marginFraction <= 0) {
    return 50;
  }

  return LIGHTER_MARGIN_FRACTION_TICK / marginFraction;
};

const parseLighterMarginFractionRatio = (value: unknown): number | undefined => {
  const parsed = Number.parseFloat(String(value ?? ''));
  if (!Number.isFinite(parsed) || parsed <= 0) {
    return undefined;
  }
  if (parsed > 100) {
    return parsed / LIGHTER_MARGIN_FRACTION_TICK;
  }
  if (parsed > 1) {
    return parsed / 100;
  }
  return parsed;
};

const normalizeLighterStopSlippagePct = (value: unknown): number | undefined => {
  const parsed = Number.parseFloat(String(value ?? '').trim());
  if (!Number.isFinite(parsed) || parsed < 0) {
    return undefined;
  }

  return Math.min(parsed, LIGHTER_MAX_STOP_SLIPPAGE_PCT);
};

export class LighterExchange extends BaseExchange {
  private static readonly SIGNER_RETRY_COOLDOWN_MS = 30_000;
  private static readonly SIGNER_WARNING_LOG_COOLDOWN_MS = 10_000;

  lighterApiClient: LighterApiClient;
  signerAdapter?: LighterSignerAdapter;
  readonly lighterDeploymentProfile: LighterDeploymentProfile;

  // Specialized API clients
  orderApi: OrderApi;
  candlestickApi: CandlestickApi;

  publicWebsocket: LighterPublicWebsocket;
  privateWebsocket: LighterPrivateWebsocket;

  grafanaEndpointsEvery = GRAFANA_ENDPOINTS_EVERY;
  grafanaEndpointsPeriodic = GRAFANA_ENDPOINTS_PERIODIC;

  lastWebsocketPositionsUpdate: number | undefined;

  // Track recently cancelled orders to prevent re-adding them before API processes cancellation
  recentlyCancelledOrders: Map<string, number> = new Map();

  // Store account index from API
  private accountIndex?: bigint;
  private hasWarnedMissingOrderAuth = false;
  private hasWarnedMissingOrderAccountIndex = false;
  private hasWarnedInvalidOrderAuthAccount = false;
  private signerInitErrorMessage?: string;
  private signerAdapterInitPromise?: Promise<LighterSignerAdapter>;
  private signerRetryBlockedUntil = 0;
  private lastSignerWarningAt = 0;
  private placedOrderHints: Map<string, LighterPlacedOrderHint> = new Map();
  private lastClientOrderIndex = 0n;
  private ohlcvFallbackFetchByKey: Map<string, Promise<Candle | undefined>> = new Map();
  private ohlcvFallbackCooldownUntilByKey: Map<string, number> = new Map();
  private marginModeBySymbol: Map<string, 0 | 1> = new Map();
  private accountSnapshotFetchPromise?: Promise<Record<string, any>>;
  private accountSnapshotFetchVersion = 0;
  private accountSnapshotCacheVersion = 0;
  private recentAccountSnapshot?: { fetchedAt: number; cacheVersion: number; value: Record<string, any> };
  private ordersFetchPromise?: Promise<Order[]>;
  private lastOrdersFetchAt = 0;
  private accountLimits?: Record<string, any>;
  private accountLimitsFetchPromise?: Promise<Record<string, any> | undefined>;
  private accountLimitsNextFetchAt = 0;
  private lastTransientSnapshotWarningAt = 0;
  private hasWarnedAccountLimitsLookup = false;

  constructor(exchangeName: ExchangeName, opts: ExchangeOptions, store: Store) {
    super(exchangeName, opts, store);

    this.lighterDeploymentProfile = resolveLighterDeploymentProfile({
      exchangeName,
      testnet: opts.testnet,
    });
    this.lighterApiClient = createLighterSDKClient(opts, this);

    // Initialize specialized API clients
    this.orderApi = new OrderApi(this.lighterApiClient);
    this.candlestickApi = new CandlestickApi(this.lighterApiClient);

    this.publicWebsocket = new LighterPublicWebsocket(this);
    this.privateWebsocket = new LighterPrivateWebsocket(this);
  }

  private parseAccountIndex = (value: unknown): bigint | undefined => {
    if (value === null || value === undefined) {
      return undefined;
    }

    try {
      return BigInt(String(value));
    } catch {
      return undefined;
    }
  };

  private reserveClientOrderIndex = (): bigint => {
    const maxIndex = (1n << 48n) - 1n;
    const candidate = (BigInt(Date.now()) * 100n + BigInt(Math.floor(Math.random() * 100))) % maxIndex;
    const next = candidate > this.lastClientOrderIndex ? candidate : this.lastClientOrderIndex + 1n;
    if (next > maxIndex) {
      return this.rejectPreflight('Lighter client order index namespace exhausted.');
    }
    // Reserve synchronously, including failed/unknown sends. Split and bracket
    // legs and batched placements must share this instance-local namespace.
    this.lastClientOrderIndex = next;
    return next;
  };

  private rememberPlacedOrderHint = (orderId: bigint, hint: Omit<LighterPlacedOrderHint, 'createdAt'>) => {
    this.placedOrderHints.set(orderId.toString(), { ...hint, createdAt: Date.now() });

    // Keep cache bounded in long-running sessions.
    while (this.placedOrderHints.size > 2000) {
      const oldestKey = this.placedOrderHints.keys().next().value;
      if (!oldestKey) break;
      this.placedOrderHints.delete(oldestKey);
    }
  };

  private readPlacedOrderHint = (orderId?: string): LighterPlacedOrderHint | undefined => {
    if (!orderId) {
      return undefined;
    }

    const hint = this.placedOrderHints.get(orderId);
    if (!hint) {
      return undefined;
    }

    // Drop stale hints after 10 minutes.
    if (Date.now() - hint.createdAt > 10 * 60 * 1000) {
      this.placedOrderHints.delete(orderId);
      return undefined;
    }

    return hint;
  };

  private inferStopLossTypeFromReduceOnlyLimit = (
    order: Pick<Order, 'symbol' | 'type' | 'side' | 'price' | 'reduceOnly'>,
  ): OrderType | undefined => {
    if (
      order.type !== OrderType.Limit ||
      !order.reduceOnly ||
      !Number.isFinite(order.price) ||
      order.price <= 0
    ) {
      return undefined;
    }

    const ticker = this.store.tickersBySymbol[order.symbol];
    const referencePrice = this.getPositivePriceCandidate(ticker?.mark, ticker?.last, ticker?.index);
    if (!referencePrice) {
      return undefined;
    }

    // If sync loses the explicit SL type, a loss-side reduce-only limit would
    // be marketable and close the position. Treat only that loss-side shape as
    // SL; profit-side reduce-only limits can be legitimate resting close orders.
    if (order.side === OrderSide.Sell) {
      return order.price <= referencePrice ? OrderType.StopLoss : undefined;
    }
    return order.price >= referencePrice ? OrderType.StopLoss : undefined;
  };

  private extractAccountFromResponse = (
    payload: any,
  ): {
    accountIndex?: bigint;
    referrer?: string;
    errorMessage?: string;
  } => {
    // Legacy response shape: { success, data: { accountIndex, referrer } }
    if (payload?.success === true && payload?.data) {
      const accountIndex = this.parseAccountIndex(payload.data.accountIndex);
      return {
        accountIndex,
        referrer: payload.data.referrer,
      };
    }

    // Current response shape: { code, total, accounts: [{ index | account_index, ... }] }
    if (payload?.code === 200 && Array.isArray(payload?.accounts)) {
      const firstAccount = payload.accounts[0];
      const accountIndex = this.parseAccountIndex(firstAccount?.account_index ?? firstAccount?.index);
      return {
        accountIndex,
        referrer: firstAccount?.referrer,
      };
    }

    return {
      errorMessage: payload?.message || payload?.error?.message,
    };
  };

  private fetchParsedAccountLookup = async (
    params?: LighterAccountLookupParams,
  ): Promise<{ accountIndex: bigint; referrer?: string }> => {
    const payload = await this.lighterApiClient.get<any>(ENDPOINTS.ACCOUNT, params ? { params } : undefined);
    const parsed = this.extractAccountFromResponse(payload);
    if (parsed.accountIndex === undefined) {
      throw new Error(parsed.errorMessage || 'Failed to resolve Lighter account index');
    }

    return {
      accountIndex: parsed.accountIndex,
      referrer: parsed.referrer,
    };
  };

  private fetchAccountFromL1AddressLookup = async (
    l1Address: string,
  ): Promise<{ accountIndex: bigint; referrer?: string }> => {
    const payload = await this.lighterApiClient.get<any>(ENDPOINTS.ACCOUNTS_BY_L1_ADDRESS, {
      params: {
        l1_address: l1Address,
      },
    });

    const accounts = [
      ...(Array.isArray(payload?.sub_accounts) ? payload.sub_accounts : []),
      ...(Array.isArray(payload?.accounts) ? payload.accounts : []),
      ...(Array.isArray(payload?.data?.sub_accounts) ? payload.data.sub_accounts : []),
      ...(Array.isArray(payload?.data?.accounts) ? payload.data.accounts : []),
    ];
    const firstAccount = accounts[0];
    const accountIndex = this.parseAccountIndex(firstAccount?.account_index ?? firstAccount?.index);
    if (accountIndex === undefined) {
      throw new Error(
        payload?.message || payload?.error?.message || 'Failed to resolve Lighter account index by L1 address',
      );
    }

    return {
      accountIndex,
      referrer: firstAccount?.referrer,
    };
  };

  private ensureAccountIndex = async (): Promise<bigint> => {
    if (this.accountIndex !== undefined) {
      return this.accountIndex;
    }

    const account = await this.fetchAccountInfo();
    this.accountIndex = account.accountIndex;
    return this.accountIndex;
  };

  private getValidatedApiKeyIndex = (): number => {
    const apiKeyIndex = parseLighterApiKeyIndex(this.options.apiKeyIndex);
    if (apiKeyIndex === undefined || apiKeyIndex < 0 || apiKeyIndex > 255) {
      throw new Error('Invalid API key index. Expected an integer from 0 to 255.');
    }
    return apiKeyIndex;
  };

  private extractL1AddressCandidate = (value: string | undefined): string | undefined => {
    if (!value) {
      return undefined;
    }

    const trimmed = value.trim();
    if (isLighterL1Address(trimmed)) {
      return trimmed;
    }

    const embeddedAddressMatch = trimmed.match(/0x[a-fA-F0-9]{40}/);
    return embeddedAddressMatch?.[0];
  };

  private fetchAccountInfo = async (): Promise<{ accountIndex: bigint; referrer?: string }> => {
    const key = this.options.key?.trim();
    const applicationId = this.options.applicationId?.trim();
    let lastErrorMessage: string | undefined;

    const configuredAccountIndex = parseLighterPositiveIntegerString(applicationId);
    const accountIndexCandidate = parseLighterApiKeyIndex(this.options.apiKeyIndex);
    const lookupCandidates: LighterAccountLookupParams[] = [];
    const l1AddressLookupCandidates = uniq(
      [this.extractL1AddressCandidate(applicationId), this.extractL1AddressCandidate(key)].filter(
        (value): value is string => Boolean(value),
      ),
    );

    // Preferred explicit account lookup: account index provided in applicationId.
    if (configuredAccountIndex !== undefined) {
      lookupCandidates.push({
        by: 'index',
        value: configuredAccountIndex,
      });
    }

    // If applicationId is an L1 address, resolve account by L1 address.
    for (const l1AddressCandidate of l1AddressLookupCandidates) {
      lookupCandidates.push({
        by: 'l1_address',
        value: l1AddressCandidate,
      });
    }

    // Compatibility fallback: legacy setups may have stored account index in apiKeyIndex.
    if (accountIndexCandidate !== undefined) {
      lookupCandidates.push({
        by: 'index',
        value: String(accountIndexCandidate),
      });
    }

    for (const candidate of lookupCandidates) {
      try {
        return await this.fetchParsedAccountLookup(candidate);
      } catch (err: any) {
        lastErrorMessage = err?.message || lastErrorMessage;
      }
    }

    // Fallback for API variants that expose account lookup through accountsByL1Address.
    for (const l1AddressCandidate of l1AddressLookupCandidates) {
      try {
        return await this.fetchAccountFromL1AddressLookup(l1AddressCandidate);
      } catch (err: any) {
        lastErrorMessage = err?.message || lastErrorMessage;
      }
    }

    const guidance = 'Provide your Lighter account index in Application ID.';
    throw new Error(
      lastErrorMessage ? `${lastErrorMessage}. ${guidance}` : `Failed to resolve Lighter account index. ${guidance}`,
    );
  };

  private fetchAccountSnapshot = async (opts: { allowRecent?: boolean } = {}): Promise<Record<string, any>> => {
    const now = virtualClock.getCurrentTimeMs();
    if (
      opts.allowRecent &&
      this.recentAccountSnapshot &&
      this.recentAccountSnapshot.cacheVersion === this.accountSnapshotCacheVersion &&
      now - this.recentAccountSnapshot.fetchedAt < LIGHTER_ACCOUNT_SNAPSHOT_REUSE_MS
    ) {
      return this.recentAccountSnapshot.value;
    }

    if (this.accountSnapshotFetchPromise && this.accountSnapshotFetchVersion === this.accountSnapshotCacheVersion) {
      return this.accountSnapshotFetchPromise;
    }

    const fetchVersion = this.accountSnapshotCacheVersion;
    this.accountSnapshotFetchVersion = fetchVersion;
    this.accountSnapshotFetchPromise = (async () => {
      const accountIndex = await this.ensureAccountIndex();
      const payload = await this.lighterApiClient.get<any>(ENDPOINTS.ACCOUNT, {
        params: {
          by: 'index',
          value: accountIndex.toString(),
        },
      });

      // Current response shape: { code, accounts: [...] }
      if (payload?.code === 200 && Array.isArray(payload?.accounts)) {
        const account = payload.accounts[0];
        if (account) {
          this.rememberRecentAccountSnapshot(account, fetchVersion);
          return account;
        }
        throw new Error('Lighter account lookup returned no accounts');
      }

      // Legacy fallback shape
      if (payload?.success === true && payload?.data) {
        this.rememberRecentAccountSnapshot(payload.data, fetchVersion);
        return payload.data;
      }

      throw new Error(payload?.message || payload?.error?.message || 'Failed to fetch account snapshot');
    })().finally(() => {
      if (this.accountSnapshotFetchVersion === fetchVersion) {
        this.accountSnapshotFetchPromise = undefined;
      }
    });

    return this.accountSnapshotFetchPromise;
  };

  private rememberRecentAccountSnapshot = (value: Record<string, any>, fetchVersion: number): void => {
    if (fetchVersion !== this.accountSnapshotCacheVersion) {
      return;
    }

    this.recentAccountSnapshot = {
      fetchedAt: virtualClock.getCurrentTimeMs(),
      cacheVersion: fetchVersion,
      value,
    };
  };

  invalidateAccountSnapshotCache = (): void => {
    this.accountSnapshotCacheVersion += 1;
    this.recentAccountSnapshot = undefined;
  };

  private fetchAccountLimits = async (): Promise<Record<string, any> | undefined> => {
    const now = virtualClock.getCurrentTimeMs();
    if (now < this.accountLimitsNextFetchAt) {
      return this.accountLimits;
    }

    if (this.accountLimitsFetchPromise) {
      return this.accountLimitsFetchPromise;
    }

    this.accountLimitsFetchPromise = (async () => {
      const accountIndex = await this.ensureAccountIndex();
      const authToken = await this.getPrivateWsAuthToken(60);

      if (!authToken) {
        this.accountLimitsNextFetchAt = now + LIGHTER_ACCOUNT_LIMITS_RETRY_MS;
        return this.accountLimits;
      }

      try {
        const accountLimitsRequestConfig = {
          params: {
            account_index: accountIndex.toString(),
          },
          headers: {
            authorization: authToken,
          },
          __suppressExchangeError: true,
        } as any;

        const payload = await this.lighterApiClient.get<any>(ENDPOINTS.ACCOUNT_LIMITS, accountLimitsRequestConfig);

        if (payload?.code === 200) {
          this.accountLimits = payload;
          this.accountLimitsNextFetchAt = now + LIGHTER_ACCOUNT_LIMITS_CACHE_MS;
          return payload;
        }

        throw new Error(payload?.message || 'Lighter account limits lookup failed');
      } catch (err: any) {
        this.accountLimitsNextFetchAt = now + LIGHTER_ACCOUNT_LIMITS_RETRY_MS;
        if (!this.hasWarnedAccountLimitsLookup) {
          this.hasWarnedAccountLimitsLookup = true;
          this.log(`Lighter account tier lookup failed: ${err?.message || String(err)}`, LogSeverity.Debug);
        }
        return this.accountLimits;
      }
    })().finally(() => {
      this.accountLimitsFetchPromise = undefined;
    });

    return this.accountLimitsFetchPromise;
  };

  private getCachedAccountTier = (): LighterAccountTier | undefined => {
    return extractLighterAccountTierFromPayload(this.accountLimits);
  };

  private ensurePartnerAttributionTierEligible = async (
    builderFeeConfig: LighterBuilderFeeConfig | undefined,
  ): Promise<void> => {
    if (!builderFeeConfig || !hasNonZeroLighterIntegratorFee(builderFeeConfig)) {
      return;
    }

    let accountLimits: Record<string, any> | undefined;
    try {
      accountLimits = await this.fetchAccountLimits();
    } catch (error: any) {
      this.log(
        `Lighter partner-attribution account tier lookup skipped: ${error?.message || String(error)}`,
        LogSeverity.Debug,
      );
      return;
    }

    const tier = extractLighterAccountTierFromPayload(accountLimits);
    if (tier && !isLighterAccountTierEligibleForPartnerAttribution(tier)) {
      this.emitAndLogError(LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE);
      throw new Error(LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE);
    }
  };

  private ensurePartnerAttributionApproved = async (
    builderFeeConfig: LighterBuilderFeeConfig | undefined,
  ): Promise<void> => {
    if (!builderFeeConfig || !hasNonZeroLighterIntegratorFee(builderFeeConfig)) {
      return;
    }

    let account: Record<string, any> | undefined;
    try {
      account = await this.fetchAccountSnapshot({ allowRecent: true });
    } catch (error: any) {
      // Approval lookup is best-effort; sendTx still normalizes code 21149 on rejection.
      this.log(
        `Lighter partner-attribution approval lookup skipped: ${error?.message || String(error)}`,
        LogSeverity.Debug,
      );
      return;
    }

    const approvedIntegrators = Array.isArray(account?.approved_integrators) ? account!.approved_integrators : [];
    const integratorIndexString = builderFeeConfig.integratorAccountIndex.toString();
    const now = virtualClock.getCurrentTimeMs();
    const hasValidApproval = approvedIntegrators.some((entry: any) => {
      const entryIndex = String(entry?.account_index ?? entry?.accountIndex ?? '').trim();
      if (entryIndex !== integratorIndexString) return false;
      const expiryRaw = entry?.approval_expiry ?? entry?.approvalExpiry ?? 0;
      const expiry = Number(expiryRaw);
      return Number.isFinite(expiry) && expiry > now;
    });

    if (!hasValidApproval) {
      this.emitAndLogError(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE);
      throw new Error(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE);
    }
  };

  getAccountRateLimitGuidanceMessage = (): string => {
    const tier = this.getCachedAccountTier();

    if (tier === 'standard') {
      return LIGHTER_STANDARD_RATE_LIMIT_GUIDANCE_MESSAGE;
    }

    if (tier === 'premium' || tier === 'plus') {
      return LIGHTER_ACCOUNT_RATE_LIMIT_GUIDANCE_MESSAGE;
    }

    return LIGHTER_UNKNOWN_TIER_RATE_LIMIT_GUIDANCE_MESSAGE;
  };

  changeAccountTier = async (newTier: LighterAccountTier): Promise<void> => {
    const accountIndex = await this.ensureAccountIndex();
    const authToken = await this.getPrivateWsAuthToken(60);

    if (!authToken) {
      throw new Error(
        'Lighter auth token unavailable. Please wait for the account signer to finish loading and try again.',
      );
    }

    const body = new URLSearchParams({
      account_index: accountIndex.toString(),
      new_tier: newTier,
    });

    const requestConfig = {
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
        authorization: authToken,
      },
      __suppressExchangeError: true,
    } as any;

    const payload = await this.lighterApiClient.post<any>(
      ENDPOINTS.CHANGE_ACCOUNT_TIER,
      body.toString(),
      requestConfig,
    );

    if (payload?.code !== 200) {
      throw new Error(payload?.message || `Failed to switch Lighter account tier to ${newTier}`);
    }

    this.accountLimits = undefined;
    this.accountLimitsNextFetchAt = 0;

    const balance = await this.fetchRawBalance();
    if (!this.isDisposed) {
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    }
  };

  private warmupSignerRuntime = async (): Promise<void> => {
    if (!isWasmReady()) {
      await initWasm();
    }
    // Force-read the runtime to catch cases where wasm reports ready but exports are not yet live.
    getCryptoApi();
  };

  private normalizeSignerErrorMessage = (error: unknown): string => {
    if (error instanceof Error && error.message) {
      return error.message;
    }
    return String(error || '');
  };

  private isWasmRuntimeIssueMessage = (message: string): boolean => {
    const normalized = message.toLowerCase();
    return (
      normalized.includes('wasm not initialized') ||
      normalized.includes('failed to fetch dynamically imported module') ||
      normalized.includes('signerclient export not found') ||
      normalized.includes('leverage signer unavailable in runtime') ||
      normalized.includes('node:fs')
    );
  };

  private shouldShortCircuitSignerRetry = (): boolean => {
    const now = virtualClock.getCurrentTimeMs();
    const hasWasmIssue = this.isWasmRuntimeIssueMessage(this.signerInitErrorMessage || '');
    if (hasWasmIssue && !isWasmReady()) {
      return true;
    }
    return now < this.signerRetryBlockedUntil;
  };

  private getSignerRetryBlockMessage = (): string => {
    const now = virtualClock.getCurrentTimeMs();
    const cooldownRemainingMs = Math.max(0, this.signerRetryBlockedUntil - now);
    const hasWasmIssue = this.isWasmRuntimeIssueMessage(this.signerInitErrorMessage || '');
    const retryHint =
      hasWasmIssue && !isWasmReady()
        ? 'WASM runtime is still unavailable.'
        : cooldownRemainingMs > 0
          ? `Retrying is paused for ${Math.ceil(cooldownRemainingMs / 1000)}s.`
          : 'Retrying is temporarily paused.';
    const reason = this.signerInitErrorMessage ? ` Last init failure: ${this.signerInitErrorMessage}` : '';
    return `Lighter signer unavailable; execution is disabled until signer runtime recovers. ${retryHint}${reason}`;
  };

  private warnSignerUnavailable = (message: string): void => {
    const now = virtualClock.getCurrentTimeMs();
    if (now - this.lastSignerWarningAt < LighterExchange.SIGNER_WARNING_LOG_COOLDOWN_MS) {
      return;
    }
    this.lastSignerWarningAt = now;
    this.log(message, LogSeverity.Warning);
  };

  private ensureSignerAdapter = async (): Promise<LighterSignerAdapter> => {
    if (this.signerAdapter) {
      return this.signerAdapter;
    }

    if (this.signerAdapterInitPromise) {
      return await this.signerAdapterInitPromise;
    }

    if (!this.options.secret) {
      throw new Error('Lighter private key is missing');
    }

    if (this.shouldShortCircuitSignerRetry()) {
      const blockedMessage = this.getSignerRetryBlockMessage();
      this.warnSignerUnavailable(blockedMessage);
      throw new Error(blockedMessage);
    }

    const accountIndex = await this.ensureAccountIndex();
    const apiKeyIndex = this.getValidatedApiKeyIndex();

    this.signerAdapterInitPromise = (async () => {
      try {
        await this.warmupSignerRuntime();
        const createAdapter = async (): Promise<LighterSignerAdapter> => {
          return await createLighterSignerAdapter({
            exchangeName: this.name,
            options: this.options,
            apiClient: this.lighterApiClient,
            accountIndex,
            apiKeyIndex,
          });
        };

        try {
          this.signerAdapter = await createAdapter();
        } catch (err: any) {
          const message = String(err?.message || err || '').toLowerCase();
          const isWasmInitIssue = message.includes('wasm not initialized');
          if (!isWasmInitIssue) {
            throw err;
          }
          this.log(
            'Lighter signer init hit WASM-not-initialized race; retrying once after explicit runtime warmup.',
            LogSeverity.Warning,
          );
          await this.warmupSignerRuntime();
          this.signerAdapter = await createAdapter();
        }

        this.signerInitErrorMessage = undefined;
        this.signerRetryBlockedUntil = 0;
        return this.signerAdapter;
      } catch (err: any) {
        this.signerAdapter = undefined;
        this.signerInitErrorMessage = this.normalizeSignerErrorMessage(err);
        const now = virtualClock.getCurrentTimeMs();
        const hasWasmIssue = this.isWasmRuntimeIssueMessage(this.signerInitErrorMessage);
        // WASM/runtime failures should hard-stop retries while runtime is still not ready.
        if (hasWasmIssue && !isWasmReady()) {
          this.signerRetryBlockedUntil = 0;
        } else {
          this.signerRetryBlockedUntil = now + LighterExchange.SIGNER_RETRY_COOLDOWN_MS;
        }
        throw new Error(`Lighter signer initialization failed: ${this.signerInitErrorMessage}`);
      } finally {
        this.signerAdapterInitPromise = undefined;
      }
    })();

    return await this.signerAdapterInitPromise;
  };

  dispose = () => {
    super.dispose();
    this.ohlcvFallbackFetchByKey.clear();
    this.publicWebsocket.dispose();
    this.privateWebsocket.dispose();
    if (this.signerAdapter?.dispose) {
      void this.signerAdapter.dispose();
    }
  };

  getAccount = async () => {
    const account = await this.fetchAccountInfo();

    if (this.accountIndex === undefined) {
      this.accountIndex = account.accountIndex;
    }

    return {
      userId: account.accountIndex.toString(),
      affiliateId: account.referrer,
    };
  };

  validateAccount = async () => {
    try {
      try {
        this.getValidatedApiKeyIndex();
      } catch {
        return { error: 'Invalid API key index. Expected an integer from 0 to 255.' };
      }

      // Validate account lookup and cache account index
      const account = await this.fetchAccountInfo();
      this.accountIndex = account.accountIndex;

      // Validate private key if provided
      if (this.options.secret) {
        try {
          // Tealstreet signer depends on poseidon wasm.
          if (!isWasmReady()) {
            await initWasm();
          }
          getCryptoApi();
        } catch (error: any) {
          // Browser bundles can defer/handle WASM differently; don't hard-fail linking here.
          this.log(
            `WASM signer precheck warning during validateAccount: ${error?.message || String(error)}`,
            LogSeverity.Warning,
          );
        }
      }
    } catch (err: any) {
      const message = err?.response?.data?.message || err?.message;
      this.emitAndLogError(message);
      if (
        String(message || '')
          .toLowerCase()
          .includes('invalid account l1 address')
      ) {
        return { error: (
          'Invalid L1 address for account lookup. For Lighter, the Public Key field is an API key, not an L1 address. ' +
          'Enter account index in Application ID.'
        ) };
      }
      if (
        String(message || '')
          .toLowerCase()
          .includes('not found')
      ) {
        return { error: (
          'Lighter account lookup failed. API Key Index is not account index. ' +
          'Enter your account index in Application ID.'
        ) };
      }

      return { error: message || 'Invalid API key or credentials' };
    }

    return { error: '' };
  };

  start = async () => {
    // Lighter is net-position based: opposite-side fills reduce/flip exposure rather than opening a hedge.
    this.store.setSetting('isHedged', false);

    const useExternalPublicData = await this.initExternalPublicData();
    const publicDataOnly =
      this.options.publicDataOnly || (!this.options.secret && !this.options.remoteSigner) || !this.options.apiKeyIndex;
    const signerWarmupPromise: Promise<LighterSignerAdapter | undefined> =
      !publicDataOnly && this.options.secret
        ? this.ensureSignerAdapter().catch((err: any) => {
            this.log(`Lighter signer unavailable at startup: ${err?.message || String(err)}`, LogSeverity.Warning);
            return undefined;
          })
        : Promise.resolve(undefined);

    if (!useExternalPublicData) {
      // Load initial market data. Do not block public streams on signer warmup.
      await this.fetchMarkets().then((markets) => {
        if (this.isDisposed) return [];
        this.log(`Loaded ${markets.length} Lighter markets`);
        this.store.update({
          markets,
          tickers: markets.map((market) => ({
            id: market.id,
            symbol: market.symbol,
            bid: 0,
            ask: 0,
            last: 0,
            volume: 0,
            quoteVolume: 0,
            mark: 0,
            index: 0,
            percentage: 0,
            openInterest: 0,
            fundingRate: 0,
            fundingTime: 0,
          })),
          loaded: { ...this.store.loaded, markets: true, tickers: true },
        });
        return markets;
      });

      if (this.isDisposed) return;

      // Start websocket streams
      this.publicWebsocket.connectAndSubscribe();
    } else {
      // Still need to warm up signer even with external public data
      await signerWarmupPromise;
      if (this.isDisposed) return;
    }

    this.log(`Ready to trade on Lighter`);

    // Start ticking live data
    if (publicDataOnly) {
      this.store.update({
        loaded: { ...this.store.loaded, balance: true, positions: true, orders: true },
      });
      return;
    }

    await signerWarmupPromise;

    if (this.signerAdapter) {
      this.privateWebsocket.connectAndSubscribe();
    } else {
      this.log('Lighter private websocket disabled (signer unavailable)', LogSeverity.Warning);
    }

    try {
      const [, balance] = await Promise.all([
        this.fetchPositions(undefined, { allowRecentAccountSnapshot: true }).then((positions) => {
          if (this.isDisposed) return [];
          this.store.addOrUpdatePositionsFull(positions);
          this.store.update({
            loaded: { ...this.store.loaded, positions: true, orders: true },
          });
          return positions;
        }),
        this.fetchRawBalance({ allowRecentAccountSnapshot: true, includeAccountLimits: false }),
      ]);

      if (this.isDisposed) return;
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    } catch (err: any) {
      this.handlePrivateSnapshotFetchError(err);
    }
    void loop(() => this.tick(), this.getInitialTickDelay());
  };

  private getInitialTickDelay = (): number => {
    const tickInterval = this.getTickInterval();
    return tickInterval && tickInterval > 0
      ? Math.min(tickInterval, LIGHTER_INITIAL_TICK_MAX_DELAY_MS)
      : LIGHTER_INITIAL_TICK_MAX_DELAY_MS;
  };

  fetchAndSetPrivateSnapshot = async () => {
    await this.fetchAndSetPrivateSnapshotHelper();
  };

  fetchAndSetPrivateSnapshotHelper = async () => {
    try {
      const [fetchedPositions, fetchedOrders] = await Promise.all([
        this.fetchPositions(undefined, { allowRecentAccountSnapshot: true }),
        this.fetchOrders(),
      ]);

      if (this.isDisposed) return;

      const positions =
        this.lastWebsocketPositionsUpdate !== undefined &&
        virtualClock.getCurrentTimeMs() - this.lastWebsocketPositionsUpdate <
          INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY
          ? mergeUpdatesUsingLastUpdate(fetchedPositions, this.store.positions)
          : fetchedPositions;

      // Update positions
      this.store.addOrUpdatePositionsFull(positions);
      this.store.addOrUpdateOrdersFull(fetchedOrders);

      this.store.update({
        loaded: { ...this.store.loaded, positions: true, orders: true },
      });

      const balance = await this.fetchBalance({ allowRecentAccountSnapshot: true });

      if (this.isDisposed) return;
      this.store.update({
        balance,
        loaded: { ...this.store.loaded, balance: true },
      });
    } catch (err: any) {
      this.handlePrivateSnapshotFetchError(err);
    }
  };

  tick = async () => {
    if (!this.isDisposed) {
      try {
        await this.fetchAndSetPrivateSnapshotHelper();
      } catch (err: any) {
        this.emitAndLogError(err?.message);
      }

      loop(() => this.tick(), this.getTickInterval());
    }
  };

  fetchBalance = async (opts: { allowRecentAccountSnapshot?: boolean } = {}) => {
    const rawBalance = await this.fetchRawBalance(opts);
    if (this.isDisposed) return this.store.balance;
    return rawBalance;
  };

  fetchAndSetBalance = async (
    opts: { allowRecentAccountSnapshot?: boolean; includeAccountLimits?: boolean } = {},
  ): Promise<void> => {
    const balance = await this.fetchRawBalance(opts);
    if (this.isDisposed) return;
    this.store.update({
      balance,
      loaded: { ...this.store.loaded, balance: true },
    });
  };

  fetchRawBalance = async (
    opts: { allowRecentAccountSnapshot?: boolean; includeAccountLimits?: boolean } = {},
  ): Promise<Balance> => {
    const balance = await this.fetchAccountSnapshot({ allowRecent: opts.allowRecentAccountSnapshot });
    const accountLimits = opts.includeAccountLimits === false ? this.accountLimits : await this.fetchAccountLimits();

    // Calculate unrealized PnL from positions
    const upnl = this.store.positions.reduce((acc, p) => {
      const ticker = this.store.tickersBySymbol[p.symbol];

      if (!ticker) {
        return acc;
      }

      return (
        acc + (p.side === PositionSide.Long ? ticker!.last - p.entryPrice : p.entryPrice - ticker!.last) * p.contracts
      );
    }, 0);

    const totalEquity = parseFloat(balance.total_asset_value || balance.collateral || balance.cross_asset_value || '0');
    const free = parseFloat(balance.available_balance || balance.availableBalance || '0');
    const totalUsed = Math.max(0, subtract(totalEquity, free));

    const total = subtract(totalEquity, upnl);

    return {
      total,
      upnl,
      used: totalUsed,
      free,
      info: accountLimits ? { ...balance, lighterAccountLimits: accountLimits } : balance,
    };
  };

  private parseMarginMode = (value: unknown): 0 | 1 | undefined => {
    if (value === undefined || value === null) {
      return undefined;
    }
    if (value === 0 || value === '0') {
      return 0;
    }
    if (value === 1 || value === '1') {
      return 1;
    }

    const normalized = String(value).toLowerCase();
    if (normalized === 'cross') {
      return 0;
    }
    if (normalized === 'isolated') {
      return 1;
    }

    return undefined;
  };

  private marginModeToMarginType = (marginMode: 0 | 1 | undefined): MarginType => {
    return marginMode === 1 ? MarginType.Isolated : MarginType.Cross;
  };

  private marginTypeToMarginMode = (marginType: MarginType | undefined): 0 | 1 => {
    return marginType === MarginType.Isolated ? 1 : 0;
  };

  private getMarketConfiguredMarginMode = (market?: Market): 0 | 1 | undefined => {
    const info = market?.info as Record<string, any> | undefined;
    return this.parseMarginMode(
      info?.market_config?.market_margin_mode ??
        info?.marketConfig?.marketMarginMode ??
        info?.market_margin_mode ??
        info?.marketMarginMode,
    );
  };

  private getCurrentMarginModeForSymbol = (symbol: string): 0 | 1 => {
    const cachedMarginMode = this.marginModeBySymbol.get(symbol);
    if (cachedMarginMode !== undefined) {
      return cachedMarginMode;
    }

    const market = this.store.marketsBySymbol[symbol];
    const activePosition = this.store.positions.find(
      (position) => position.symbol === symbol && position.contracts > 0,
    );
    if (activePosition) {
      return this.marginTypeToMarginMode(activePosition.marginType);
    }

    const configuredMarginMode = this.getMarketConfiguredMarginMode(market);
    if (configuredMarginMode !== undefined) {
      return configuredMarginMode;
    }

    const firstPosition = this.store.positions.find((position) => position.symbol === symbol);
    if (firstPosition) {
      return this.marginTypeToMarginMode(firstPosition.marginType);
    }

    return 0;
  };

  private getCurrentLeverageForSymbol = (symbol: string, market: Market): number => {
    const positionWithExposure = this.store.positions.find(
      (position) => position.symbol === symbol && position.contracts > 0 && Number.isFinite(position.leverage),
    );
    const fallbackPosition = this.store.positions.find(
      (position) => position.symbol === symbol && Number.isFinite(position.leverage),
    );
    const leverageCandidate =
      positionWithExposure?.leverage ?? fallbackPosition?.leverage ?? market.limits.leverage.max ?? 10;
    const clampedLeverage = Math.min(
      Math.max(leverageCandidate, market.limits.leverage.min),
      market.limits.leverage.max,
    );
    return this.snapToLeverageTier(clampedLeverage);
  };

  fetchPositions = async (_symbols?: string[], opts: { allowRecentAccountSnapshot?: boolean } = {}) => {
    const account = await this.fetchAccountSnapshot({ allowRecent: opts.allowRecentAccountSnapshot });
    const rawPositions = Array.isArray(account?.positions) ? account.positions : [];

    const positions: Position[] =
      rawPositions.map(this.mapPosition.bind(this)).filter((p: Position | null): p is Position => p !== null) || [];

    const leverageBySymbol = rawPositions.reduce(
      (acc, rawPosition) => {
        const rawMarketId = rawPosition.marketId ?? rawPosition.market_id;
        const market = this.store.marketsById[rawMarketId] || this.store.marketsBySymbol[rawPosition.symbol];
        if (!market) {
          return acc;
        }
        acc[market.symbol] = this.resolvePositionLeverage(rawPosition, market);
        return acc;
      },
      {} as Record<string, number>,
    );
    const marginModeBySymbol = rawPositions.reduce(
      (acc, rawPosition) => {
        const rawMarketId = rawPosition.marketId ?? rawPosition.market_id;
        const market = this.store.marketsById[rawMarketId] || this.store.marketsBySymbol[rawPosition.symbol];
        if (!market) {
          return acc;
        }
        const parsedMarginMode = this.parseMarginMode(rawPosition.marginMode ?? rawPosition.margin_mode);
        if (parsedMarginMode !== undefined) {
          acc[market.symbol] = parsedMarginMode;
        }
        return acc;
      },
      {} as Record<string, 0 | 1>,
    );
    for (const symbol in marginModeBySymbol) {
      const marginMode = marginModeBySymbol[symbol];
      if (marginMode === undefined) {
        continue;
      }
      this.marginModeBySymbol.set(symbol, marginMode);
    }

    // Create one-way placeholder positions for all markets.
    const fakePositions: Position[] = this.store.markets.reduce((acc: Position[], m) => {
      const positionMarginType = this.marginModeToMarginType(
        marginModeBySymbol[m.symbol] ?? this.marginModeBySymbol.get(m.symbol) ?? this.getMarketConfiguredMarginMode(m),
      );
      acc.push({
        id: createPositionId(m.symbol, PositionSide.Long, PositionMode.OneWay),
        symbol: m.symbol,
        side: PositionSide.Long,
        entryPrice: 0,
        notional: 0,
        unrealizedPnl: 0,
        contracts: 0,
        leverage: leverageBySymbol[m.symbol] || m.limits.leverage.max || 10,
        liquidationPrice: 0,
        positionMode: PositionMode.OneWay,
        marginType: positionMarginType,
      });
      return acc;
    }, []);

    return uniqBy([...positions, ...fakePositions], 'id');
  };

  shouldSkipOrderReconciliationFetch = (cooldownMs = LIGHTER_ORDER_RECONCILIATION_COOLDOWN_MS): boolean =>
    Boolean(this.ordersFetchPromise) ||
    (this.lastOrdersFetchAt > 0 && virtualClock.getCurrentTimeMs() - this.lastOrdersFetchAt < cooldownMs);

  fetchOrders = async (): Promise<Order[]> => {
    if (this.ordersFetchPromise) {
      return this.ordersFetchPromise;
    }

    this.ordersFetchPromise = this.fetchOrdersOnce().finally(() => {
      this.lastOrdersFetchAt = virtualClock.getCurrentTimeMs();
      this.ordersFetchPromise = undefined;
    });

    return this.ordersFetchPromise;
  };

  private fetchOrdersOnce = async (): Promise<Order[]> => {
    const accountIndex = await this.ensureAccountIndex().catch((err: any) => {
      if (!this.hasWarnedMissingOrderAccountIndex) {
        this.hasWarnedMissingOrderAccountIndex = true;
        this.log(
          `Lighter account index is unavailable for accountActiveOrders; private order sync is temporarily disabled (${
            err?.message || String(err)
          })`,
          LogSeverity.Warning,
        );
      }
      return undefined;
    });
    if (accountIndex === undefined) {
      return this.store.orders;
    }
    this.hasWarnedMissingOrderAccountIndex = false;

    const authToken = await this.getPrivateWsAuthToken(60);
    if (!authToken) {
      if (!this.hasWarnedMissingOrderAuth) {
        this.hasWarnedMissingOrderAuth = true;
        this.log(
          'Lighter auth token unavailable for accountActiveOrders; private order sync is temporarily disabled',
          LogSeverity.Warning,
        );
      }
      return this.store.orders;
    }
    this.hasWarnedMissingOrderAuth = false;

    const isInvalidAuthAccountError = (err: unknown): boolean => {
      const error = err as
        | {
            message?: unknown;
            response?: { data?: { message?: unknown; error?: { message?: unknown } } };
          }
        | undefined;
      const candidates = [
        error?.message,
        error?.response?.data?.message,
        error?.response?.data?.error?.message,
      ];
      return candidates.some((message) => /invalid auth:\s*couldn'?t find account/i.test(String(message || '')));
    };

    const handleInvalidAuthAccountError = (err: unknown): Order[] => {
      if (!this.hasWarnedInvalidOrderAuthAccount) {
        this.hasWarnedInvalidOrderAuthAccount = true;
        const message = err instanceof Error ? err.message : String(err);
        this.log(
          `Lighter accountActiveOrders auth was rejected; private order sync is temporarily disabled. Verify the account index, API key index, and private key all belong to the same Lighter deployment (${message})`,
          LogSeverity.Warning,
        );
      }
      return this.store.orders;
    };

    const readOrders = (payload: any): any[] => {
      const candidates = [
        payload?.orders,
        payload?.data?.orders,
        payload?.result?.orders,
        payload?.data,
        payload?.result,
        payload,
      ];

      const flattenCandidate = (candidate: unknown): any[] => {
        if (Array.isArray(candidate)) {
          return candidate;
        }
        if (candidate && typeof candidate === 'object') {
          return Object.values(candidate as Record<string, unknown>).flatMap((entry) =>
            Array.isArray(entry) ? entry : entry && typeof entry === 'object' ? [entry] : [],
          );
        }
        return [];
      };
      const looksLikeOrder = (entry: unknown): entry is Record<string, unknown> => {
        if (!entry || typeof entry !== 'object') {
          return false;
        }
        const record = entry as Record<string, unknown>;
        return (
          'orderId' in record ||
          'order_id' in record ||
          'orderIndex' in record ||
          'order_index' in record ||
          'OrderId' in record ||
          'OrderIndex' in record ||
          'marketId' in record ||
          'market_id' in record ||
          'marketIndex' in record ||
          'MarketIndex' in record ||
          'symbol' in record ||
          'order' in record
        );
      };

      for (const candidate of candidates) {
        const parsed = flattenCandidate(candidate);
        if (parsed.length > 0) {
          const orderLike = parsed.filter(looksLikeOrder);
          return orderLike.length > 0 ? orderLike : parsed;
        }
      }
      return [];
    };

    const fetchOrdersForMarket = async (marketId: number) => {
      const payload = await this.lighterApiClient.get<any>(ENDPOINTS.OPEN_ORDERS, {
        params: {
          account_index: accountIndex.toString(),
          market_id: marketId,
          auth: authToken,
        },
      });
      return readOrders(payload);
    };

    let orders: any[] = [];
    try {
      // Lighter supports querying all markets via sentinel market_id=255.
      orders = await fetchOrdersForMarket(255);
    } catch (err: any) {
      const message = String(err?.message || '');
      const shouldFallbackByMarket =
        message.toLowerCase().includes('invalid param') || message.toLowerCase().includes('market');
      if (!shouldFallbackByMarket) {
        if (isInvalidAuthAccountError(err)) {
          return handleInvalidAuthAccountError(err);
        }
        throw err;
      }

      const marketIds = uniq(
        this.store.markets
          .map((market) => Number.parseInt(String(market.id), 10))
          .filter((marketId) => Number.isFinite(marketId) && marketId >= 0),
      );

      const batched = await Promise.allSettled(marketIds.map(async (marketId) => await fetchOrdersForMarket(marketId)));
      orders = batched.flatMap((result) => (result.status === 'fulfilled' ? result.value : []));
    }
    this.hasWarnedInvalidOrderAuthAccount = false;
    const now = Date.now();

    // Clean up expired cancelled orders.
    // In readonly WS fallback mode, accountActiveOrders can lag; keep suppression longer
    // so cancelled orders do not pop back into chart/order tables during stale snapshots.
    for (const [orderId, timestamp] of this.recentlyCancelledOrders.entries()) {
      if (now - timestamp > 30_000) {
        this.recentlyCancelledOrders.delete(orderId);
      }
    }

    // Filter out recently cancelled orders
    const filteredOrders = orders.filter((order: any) => {
      const orderId = String(
        order?.orderId ??
          order?.order_id ??
          order?.orderIndex ??
          order?.order_index ??
          order?.OrderId ??
          order?.OrderIndex ??
          order?.Index ??
          order?.index ??
          order?.order?.orderId ??
          order?.order?.order_id ??
          order?.order?.orderIndex ??
          order?.order?.order_index ??
          order?.order?.OrderId ??
          order?.order?.OrderIndex ??
          '',
      );
      return !this.recentlyCancelledOrders.has(orderId);
    });

    const mappedOrders = filteredOrders.flatMap((order: any) => this.mapOrder(order));

    return mappedOrders;
  };

  fetchHistoricalFills = async (opts: ExecutionsOptions): Promise<Execution[]> => {
    if (this.isPublicDataOnly) {
      return [];
    }

    const { symbol, orderId, from, to, limit = 100 } = opts;
    if (!symbol) {
      return [];
    }

    const market = this.store.marketsBySymbol[symbol];
    if (!market) {
      return [];
    }

    const normalizeToMilliseconds = (value?: number) => {
      if (value === undefined || value === null || !Number.isFinite(value)) {
        return undefined;
      }
      return value > 1_000_000_000_000 ? Math.floor(value) : Math.floor(value * 1000);
    };

    const normalizeToSeconds = (value: unknown) => {
      if (value === undefined || value === null) {
        return undefined;
      }
      if (typeof value === 'string' && !/^\d+$/.test(value)) {
        const parsedDate = Date.parse(value);
        return Number.isFinite(parsedDate) ? Math.floor(parsedDate / 1000) : undefined;
      }
      const parsed = Number.parseInt(String(value), 10);
      if (!Number.isFinite(parsed) || parsed <= 0) {
        return undefined;
      }
      if (parsed > 1_000_000_000_000_000) {
        return Math.floor(parsed / 1_000_000_000);
      }
      if (parsed > 1_000_000_000_000) {
        return Math.floor(parsed / 1000);
      }
      return parsed;
    };

    const maxLimit = Math.min(Math.max(limit, 1), 200);
    const fromMilliseconds = normalizeToMilliseconds(from);
    const toMilliseconds = normalizeToMilliseconds(to);

    const normalizeSymbolForMatching = (value: string) => value.replace(/[-_/]/g, '').toUpperCase();

    const parseSide = (row: Record<string, any>, accountIndex?: string) => {
      const sideRaw = String(row.side || row.order_side || '').toUpperCase();
      if (ORDER_SIDE[sideRaw]) {
        return ORDER_SIDE[sideRaw];
      }
      if (row.isBuy === true || String(row.isBuy).toLowerCase() === 'true') {
        return OrderSide.Buy;
      }
      if (row.isAsk === true || String(row.isAsk).toLowerCase() === 'true') {
        return OrderSide.Sell;
      }
      if (row.is_ask === true || String(row.is_ask).toLowerCase() === 'true') {
        return OrderSide.Sell;
      }
      if (row.is_ask === false || String(row.is_ask).toLowerCase() === 'false') {
        return OrderSide.Buy;
      }
      if (accountIndex) {
        const askAccount = String(row.ask_account_id ?? row.askAccountId ?? '');
        const bidAccount = String(row.bid_account_id ?? row.bidAccountId ?? '');
        if (askAccount && askAccount === accountIndex) {
          return OrderSide.Sell;
        }
        if (bidAccount && bidAccount === accountIndex) {
          return OrderSide.Buy;
        }
      }
      return undefined;
    };

    const extractRows = (payload: any): Array<Record<string, any>> => {
      const roots = [payload, payload?.data, payload?.result, payload?.data?.result];
      const flattened: Array<Record<string, any>> = [];
      for (const root of roots) {
        if (!root) continue;
        const candidates = [root?.fills, root?.trades, root?.orders, root?.results, root?.list, root?.data, root];
        for (const candidate of candidates) {
          if (Array.isArray(candidate)) {
            flattened.push(...candidate);
            continue;
          }
          if (candidate && typeof candidate === 'object') {
            const nestedArrays = Object.values(candidate).filter((entry) => Array.isArray(entry)) as Array<any[]>;
            for (const nested of nestedArrays) {
              flattened.push(...nested);
            }
          }
        }
      }
      return flattened.filter((row) => row && typeof row === 'object');
    };

    const parseRows = (rows: Array<Record<string, any>>, accountIndex?: string) =>
      rows.reduce<Execution[]>((acc, row) => {
        const rowMarket = this.store.marketsById[
          row.marketId || row.market_id || row.marketIndex || row.market_index
        ] as Market | undefined;
        const rowSymbol = String(
          row.symbol || row.marketSymbol || row.market_symbol || rowMarket?.symbol || market.symbol,
        ).trim();
        if (normalizeSymbolForMatching(rowSymbol) !== normalizeSymbolForMatching(market.symbol)) {
          return acc;
        }

        const side = parseSide(row, accountIndex);
        if (!side) {
          return acc;
        }

        const price = parseFloat(
          row.price ||
            row.avgPrice ||
            row.averagePrice ||
            row.avg_execution_price ||
            row.fillPrice ||
            row.execPrice ||
            row.executedPrice ||
            '0',
        );
        const qty = parseFloat(
          row.fillSize ||
            row.filledBaseAmount ||
            row.filled_base_amount ||
            row.filledSize ||
            row.filledAmount ||
            row.base_amount ||
            row.executedQty ||
            row.execQty ||
            row.size ||
            row.qty ||
            row.tradeSize ||
            '0',
        );
        if (!Number.isFinite(price) || !Number.isFinite(qty) || price <= 0 || qty <= 0) {
          return acc;
        }

        const timestamp = normalizeToSeconds(
          row.fillTime ||
            row.timestamp ||
            row.updatedAt ||
            row.updated_at ||
            row.createdAt ||
            row.created_at ||
            row.transaction_time ||
            row.transactTime ||
            row.time,
        );
        if (!timestamp) {
          return acc;
        }

        const executionOrderId = String(
          row.orderId ||
            row.order_id ||
            row.orderIndex ||
            row.order_index ||
            row.orderID ||
            row.clientOrderId ||
            (side === OrderSide.Sell ? row.ask_id || row.askId : row.bid_id || row.bidId) ||
            '',
        );
        if (orderId && executionOrderId && executionOrderId !== String(orderId)) {
          return acc;
        }

        const fee = parseFloat(
          row.fee ||
            row.execFee ||
            row.tradeFee ||
            (side === OrderSide.Sell ? row.maker_fee || row.taker_fee : row.taker_fee || row.maker_fee) ||
            '0',
        );
        acc.push({
          timestamp,
          symbol: market.symbol,
          orderId: executionOrderId || undefined,
          price,
          qty,
          side,
          commission: Number.isFinite(fee) ? Math.abs(fee) : undefined,
          commissionAsset: row.feeAsset || row.feeCurrency || market.quote,
        });
        return acc;
      }, []);

    const fetchRowsByEndpoint = async (
      endpoint: string,
      accountIndexValue: bigint,
      authToken: string,
    ): Promise<Array<Record<string, any>>> => {
      const accountIndexStr = accountIndexValue.toString();
      const marketId = Number.parseInt(String(market.id), 10);
      const inactiveOrderParams = {
        auth: authToken,
        account_index: accountIndexStr,
        market_id: Number.isFinite(marketId) ? marketId : undefined,
        order_index: orderId,
        limit: Math.min(maxLimit, 100),
      };
      const tradesParams = {
        auth: authToken,
        account_index: accountIndexStr,
        market_id: Number.isFinite(marketId) ? marketId : undefined,
        order_index: orderId,
        sort_by: 'timestamp',
        sort_dir: 'desc',
        from: fromMilliseconds,
        limit: Math.min(maxLimit, 100),
      };
      const payload = await this.lighterApiClient.get<any>(endpoint, {
        params: {
          ...(endpoint === ENDPOINTS.TRADES ? tradesParams : inactiveOrderParams),
        },
      });
      return extractRows(payload);
    };

    try {
      const accountIndex = await this.ensureAccountIndex();
      const authToken = await this.getPrivateWsAuthToken(60);
      if (!authToken) {
        return [];
      }

      const primaryRows = await fetchRowsByEndpoint(ENDPOINTS.INACTIVE_ORDERS, accountIndex, authToken);
      const fallbackRows =
        primaryRows.length > 0 ? [] : await fetchRowsByEndpoint(ENDPOINTS.TRADES, accountIndex, authToken);
      const deduped = new Map<string, Execution>();

      for (const execution of [
        ...parseRows(primaryRows, accountIndex.toString()),
        ...parseRows(fallbackRows, accountIndex.toString()),
      ]) {
        if (fromMilliseconds && execution.timestamp * 1000 < fromMilliseconds) {
          continue;
        }
        if (toMilliseconds && execution.timestamp * 1000 > toMilliseconds) {
          continue;
        }
        const key = `${execution.orderId || 'no-order'}:${execution.timestamp}:${execution.price}:${execution.qty}:${execution.side}`;
        deduped.set(key, execution);
      }

      return [...deduped.values()].sort((a, b) => b.timestamp - a.timestamp).slice(0, maxLimit);
    } catch (err: any) {
      this.log(`Failed to fetch executions: ${err?.message || err}`, LogSeverity.Warning);
      return [];
    }
  };

  fetchMarkets = async () => {
    const markets = await this.fetchMarketsHelper();
    if (markets.some((m) => m === null)) {
      this.emitAndLogError('Failed to fetch markets');
      return this.store.markets;
    }
    return markets as Market[];
  };

  private fetchLighterMarketRows = async (): Promise<Array<Record<string, any>>> => {
    try {
      const details = await this.lighterApiClient.get<Record<string, any>>(ENDPOINTS.ORDER_BOOK_DETAILS, {
        __suppressExchangeError: true,
      } as any);
      const detailRows = Array.isArray(details?.order_book_details)
        ? details.order_book_details
        : Array.isArray(details?.order_books)
          ? details.order_books
          : undefined;
      if (details?.code === 200 && detailRows) {
        return detailRows;
      }
      throw new Error((details as any)?.message || 'Failed to fetch Lighter market details');
    } catch (error: any) {
      this.log(
        `Lighter orderBookDetails unavailable; falling back to orderBooks (${error?.message || String(error)}).`,
        LogSeverity.Warning,
      );
    }

    const data = await this.lighterApiClient.get<Record<string, any>>(ENDPOINTS.MARKETS);
    if (data?.code !== 200 || !Array.isArray(data?.order_books)) {
      throw new Error((data as any)?.message || 'Failed to fetch markets');
    }

    return data.order_books;
  };

  fetchMarketsHelper = async () => {
    const marketRows = await this.fetchLighterMarketRows();

    const markets: Market[] = marketRows
      .filter(isLighterPerpMarket)
      .map((market: any) => {
        const symbol = String(market.symbol || '');
        const { base: parsedBase, quote: parsedQuote } = parseLighterMarketSymbol(
          symbol,
          this.lighterDeploymentProfile.quoteCurrency,
        );
        const amountDecimals = Number(market.supported_size_decimals ?? 3);
        const priceDecimals = Number(market.supported_price_decimals ?? 2);
        const amountIncrement = amountDecimals >= 0 ? 1 / 10 ** amountDecimals : 0.001;
        const priceIncrement = priceDecimals >= 0 ? 1 / 10 ** priceDecimals : 0.01;
        const maxLeverage = getLighterMarketLeverageMax(market);

        return {
          id: String(market.market_id),
          symbol,
          base: parsedBase,
          quote: parsedQuote,
          active: true,
          precision: {
            amount: amountIncrement,
            price: priceIncrement,
          },
          limits: {
            amount: {
              min: parseFloat(market.min_base_amount || '0'),
              max: parseFloat(market.order_quote_limit || '1000000'),
            },
            leverage: {
              min: 1,
              max: maxLeverage,
            },
          },
          isExpiring: false,
          isSpot: false,
          info: {
            ...market,
            amountDecimals,
            priceDecimals,
          },
        };
      });

    return markets;
  };

  fetchOHLCV = async (opts: OHLCVOptions) => {
    // Find market ID from symbol
    const market = this.store.marketsBySymbol[opts.symbol];
    if (!market) {
      throw new Error(`Market not found for symbol: ${opts.symbol}`);
    }

    const limit = Math.min(opts.limit || 500, 1500);
    const interval = resolveLighterInterval(opts.interval);
    const [, amount, unit] = opts.interval.split(/(\d+)/);

    const hypotheticalEnd = opts.to ? dayjs(opts.to) : dayjs();
    const end = interval === '1d' ? hypotheticalEnd : hypotheticalEnd;
    const start =
      !opts.limit && opts.from
        ? dayjs(opts.from)
        : hypotheticalEnd.subtract(parseFloat(amount) * limit, unit as ManipulateType);

    const marketId = parseInt(market.id, 10);
    const data = await this.lighterApiClient.get<any>(ENDPOINTS.CANDLES, {
      params: {
        market_id: marketId,
        resolution: interval,
        start_timestamp: start.valueOf(),
        end_timestamp: end.valueOf(),
        count_back: limit,
      },
      __suppressExchangeError: true,
    } as any);

    const rawCandles = data?.c || data?.candles || data?.candlesticks || [];
    if (!Array.isArray(rawCandles) || rawCandles.length === 0) {
      console.error(`No data for symbol ${opts.symbol}`);
      return [];
    }

    const candles: Candle[] = orderBy(
      rawCandles.map((candle: any) => {
        return {
          timestamp: Number(candle.t ?? candle.timestamp ?? 0) / 1000,
          open: parseFloat(candle.o ?? candle.open ?? '0'),
          high: parseFloat(candle.h ?? candle.high ?? '0'),
          low: parseFloat(candle.l ?? candle.low ?? '0'),
          close: parseFloat(candle.c ?? candle.close ?? '0'),
          volume: parseFloat(candle.v ?? candle.volume ?? '0'),
        };
      }),
      ['timestamp'],
      ['asc'],
    );

    return candles;
  };

  private getOhlcvFallbackKey = (opts: Pick<OHLCVOptions, 'symbol' | 'interval'>): string => {
    return `${opts.symbol}:${opts.interval}`;
  };

  private fetchLatestOhlcvWithSharedFallback = async (
    opts: Pick<OHLCVOptions, 'symbol' | 'interval'>,
    nowMs: number,
  ): Promise<Candle | undefined> => {
    const fallbackKey = this.getOhlcvFallbackKey(opts);
    const cooldownUntil = this.ohlcvFallbackCooldownUntilByKey.get(fallbackKey) || 0;
    if (virtualClock.getCurrentTimeMs() < cooldownUntil) {
      return undefined;
    }

    const existingFetch = this.ohlcvFallbackFetchByKey.get(fallbackKey);
    if (existingFetch) {
      return existingFetch;
    }

    const fetchPromise = (async () => {
      try {
        const candles = await this.fetchOHLCV({
          ...opts,
          limit: 3,
          to: nowMs,
        });
        return candles[candles.length - 1];
      } catch (error: any) {
        const statusCode = Number(error?.response?.status ?? error?.status);
        if (statusCode === 403) {
          this.ohlcvFallbackCooldownUntilByKey.set(
            fallbackKey,
            virtualClock.getCurrentTimeMs() + LIGHTER_OHLCV_FALLBACK_403_COOLDOWN_MS,
          );
          this.log(
            `[Lighter] listenOHLCV REST fallback returned 403 for ${opts.symbol} ${opts.interval}; pausing fallback requests.`,
            LogSeverity.Warning,
          );
          return undefined;
        }

        this.log(
          `[Lighter] listenOHLCV REST fallback failed for ${opts.symbol} ${opts.interval}: ${
            error?.message || String(error)
          }`,
          LogSeverity.Warning,
        );
        return undefined;
      } finally {
        this.ohlcvFallbackFetchByKey.delete(fallbackKey);
      }
    })();

    this.ohlcvFallbackFetchByKey.set(fallbackKey, fetchPromise);
    return fetchPromise;
  };

  listenOHLCV = (opts: OHLCVOptions, callback: (candle: Candle) => void) => {
    let isActive = true;
    let pollTimeout: NodeJS.Timeout | undefined;
    let lastWsCandleAt = 0;
    let lastEmittedCandleFingerprint = '';

    const getCandleFingerprint = (candle: Candle): string =>
      `${candle.timestamp}:${candle.open}:${candle.high}:${candle.low}:${candle.close}:${candle.volume}`;

    const emitIfChanged = (candle: Candle) => {
      const fingerprint = getCandleFingerprint(candle);
      if (fingerprint === lastEmittedCandleFingerprint) {
        return;
      }
      lastEmittedCandleFingerprint = fingerprint;
      callback(candle);
    };

    const wsUnsubscribe = this.publicWebsocket.listenOHLCV(opts, (candle) => {
      lastWsCandleAt = virtualClock.getCurrentTimeMs();
      emitIfChanged(candle);
    });

    const pollLatestCandle = async () => {
      if (!isActive || this.isDisposed) {
        return;
      }

      const now = virtualClock.getCurrentTimeMs();
      const hasFreshWsCandle = lastWsCandleAt > 0 && now - lastWsCandleAt <= LIGHTER_OHLCV_WS_STALE_MS;

      if (!hasFreshWsCandle) {
        this.publicWebsocket.maybeRecoverOhlcvStream(
          opts,
          `no WS candle update for ${opts.symbol} ${opts.interval}; using REST fallback`,
        );
        const latestCandle = await this.fetchLatestOhlcvWithSharedFallback(opts, now);
        if (!isActive || this.isDisposed) {
          return;
        }
        if (latestCandle) {
          emitIfChanged(latestCandle);
        }
      }

      pollTimeout = setTimeout(pollLatestCandle, LIGHTER_OHLCV_POLL_INTERVAL_MS);
    };

    void pollLatestCandle();

    return () => {
      isActive = false;
      wsUnsubscribe?.();
      if (pollTimeout) {
        clearTimeout(pollTimeout);
        pollTimeout = undefined;
      }
    };
  };

  listenOrderBook = (symbol: string, callback: (orderBook: OrderBook) => void) => {
    return this.publicWebsocket.listenOrderBook(symbol, callback);
  };

  getPrivateWsAuthToken = async (ttlSeconds: number = 60): Promise<string | undefined> => {
    if (!this.signerAdapter) {
      try {
        await this.ensureSignerAdapter();
      } catch (err: any) {
        this.warnSignerUnavailable(`Failed to initialize signer for private WS auth: ${err?.message || String(err)}`);
        return undefined;
      }
    }

    const signerAdapter = this.signerAdapter;
    if (!signerAdapter) {
      return undefined;
    }

    try {
      return await signerAdapter.getAuthToken(ttlSeconds);
    } catch (err: any) {
      this.log(`Failed to create private WS auth token: ${err?.message || String(err)}`, LogSeverity.Warning);
      return undefined;
    }
  };

  getPrivateWsAccountIndex = (): string => {
    if (this.accountIndex !== undefined) {
      return this.accountIndex.toString();
    }

    return toLighterApiKeyIndexString(this.options.apiKeyIndex);
  };

  getPrivateWsApiKeyIndex = (): string => {
    return toLighterApiKeyIndexString(this.options.apiKeyIndex);
  };

  private getReduceOnlyClosingPositionSide = (orderSide: OrderSide): PositionSide => {
    return orderSide === OrderSide.Buy ? PositionSide.Short : PositionSide.Long;
  };

  private hasOpenReduceOnlyTargetPosition = (
    symbol: string,
    orderSide: OrderSide,
    positions: Position[] = this.store.positions,
  ): boolean => {
    const closingPositionSide = this.getReduceOnlyClosingPositionSide(orderSide);
    return positions.some(
      (position) => position.symbol === symbol && position.side === closingPositionSide && position.contracts > 0,
    );
  };

  private buildNoPositionReduceOnlyMessage = (symbol: string, orderSide: OrderSide): string => {
    const closingSideLabel = orderSide === OrderSide.Buy ? 'short' : 'long';
    return `Lighter rejected reduce-only limit order for ${symbol}: no open ${closingSideLabel} position to reduce.`;
  };

  private isTransientLighterRestTransportError = (error: unknown): boolean => {
    const record = error as Record<string, any> | undefined;
    const code = String(record?.code ?? record?.response?.data?.code ?? '').toUpperCase();
    const message = this.extractOrderErrorMessage(error).toLowerCase();

    return (
      code === 'NO_RESPONSE' ||
      code === 'ECONNABORTED' ||
      code === 'ERR_NETWORK' ||
      message === 'no response received from server' ||
      message === 'network error' ||
      /timeout of \d+ms exceeded/i.test(message)
    );
  };

  private handlePrivateSnapshotFetchError = (error: unknown): void => {
    if (!this.isTransientLighterRestTransportError(error)) {
      this.emitAndLogError(
        error instanceof Error || typeof error === 'string' ? error : this.extractOrderErrorMessage(error),
      );
      return;
    }

    const now = virtualClock.getCurrentTimeMs();
    if (
      this.lastTransientSnapshotWarningAt > 0 &&
      now - this.lastTransientSnapshotWarningAt < LIGHTER_TRANSIENT_SNAPSHOT_WARNING_COOLDOWN_MS
    ) {
      return;
    }

    this.lastTransientSnapshotWarningAt = now;
    this.log(
      `[Lighter] Private snapshot fetch skipped after transient REST transport failure: ${this.extractOrderErrorMessage(
        error,
      )}`,
      LogSeverity.Warning,
    );
  };

  private extractOrderErrorMessage = (error: unknown): string => {
    if (typeof error === 'string') {
      return error;
    }
    if (error instanceof Error && error.message) {
      return error.message;
    }

    const record = error as Record<string, any> | undefined;
    const candidate =
      record?.response?.data?.error?.message ??
      record?.response?.data?.message ??
      record?.response?.data?.error ??
      record?.message;
    if (candidate !== undefined && candidate !== null) {
      return String(candidate);
    }

    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  };

  private isRateLimitOrderError = (error: unknown, message: string): boolean => {
    const record = error as Record<string, any> | undefined;
    const statusCode = Number(record?.statusCode ?? record?.response?.status);
    const code = String(record?.code ?? record?.response?.data?.code ?? '').toLowerCase();
    const normalizedMessage = message.toLowerCase();

    return (
      statusCode === 429 ||
      statusCode === 405 ||
      code === '429' ||
      code === '405' ||
      normalizedMessage.includes('rate limit') ||
      normalizedMessage.includes('too many requests') ||
      normalizedMessage.includes('too many websocket messages') ||
      normalizedMessage.includes('volume quota')
    );
  };

  private isInvalidMarginModeOrderError = (error: unknown, message: string): boolean => {
    const record = error as Record<string, any> | undefined;
    const code = String(record?.code ?? record?.response?.data?.code ?? '').toLowerCase();
    const normalizedMessage = message.toLowerCase();
    return code === '21613' || normalizedMessage.includes('invalid margin mode');
  };

  private isPartnerAttributionTierOrderError = (error: unknown, message: string): boolean => {
    const record = error as Record<string, any> | undefined;
    return isLighterPartnerAttributionTierRejection({
      code: record?.code ?? record?.response?.data?.code ?? record?.response?.data?.error?.code,
      message,
    });
  };

  private isPartnerAttributionApprovalMissingOrderError = (error: unknown, message: string): boolean => {
    const record = error as Record<string, any> | undefined;
    return isLighterPartnerAttributionApprovalMissingRejection({
      code: record?.code ?? record?.response?.data?.code ?? record?.response?.data?.error?.code,
      message,
    });
  };

  private buildInvalidMarginModeOrderMessage = (symbol: string): string => {
    const configuredMarginMode = this.getMarketConfiguredMarginMode(this.store.marketsBySymbol[symbol]);
    const requiredMode =
      configuredMarginMode === 1 ? 'isolated' : configuredMarginMode === 0 ? 'cross' : 'the intended margin mode';

    return `Lighter rejected this order because the ${symbol} margin mode appears out of sync. This market requires ${requiredMode} to trade.`;
  };

  private refreshMarginModeStateAfterInvalidOrder = async (symbol: string): Promise<void> => {
    const configuredMarginMode = this.getMarketConfiguredMarginMode(this.store.marketsBySymbol[symbol]);
    const hasOpenPosition = this.store.positions.some((position) => position.symbol === symbol && position.contracts > 0);
    if (configuredMarginMode !== undefined && !hasOpenPosition) {
      // If Lighter rejects create-order with invalid margin mode while our flat
      // placeholder came from market config, the account-level mode is stale.
      // Move the UI off the stale value so the user can explicitly re-apply it.
      this.marginModeBySymbol.set(symbol, configuredMarginMode === 1 ? 0 : 1);
    }

    this.invalidateAccountSnapshotCache();

    try {
      const positions = await this.fetchPositions();
      if (!this.isDisposed) {
        this.store.addOrUpdatePositionsFull(positions);
      }
    } catch (error: any) {
      this.log(
        `[Lighter] Failed to refresh margin mode state after invalid margin mode order rejection for ${symbol}: ${
          error?.message || String(error)
        }`,
        LogSeverity.Warning,
      );
    }
  };

  private normalizePlaceOrderErrorMessage = (opts: PlaceOrderOpts, error: unknown): string => {
    const rawMessage = this.extractOrderErrorMessage(error);
    const normalizedMessage = rawMessage.toLowerCase();
    if (this.isRateLimitOrderError(error, rawMessage)) {
      return this.getAccountRateLimitGuidanceMessage();
    }
    if (this.isPartnerAttributionTierOrderError(error, rawMessage)) {
      return LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE;
    }
    if (this.isPartnerAttributionApprovalMissingOrderError(error, rawMessage)) {
      return LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE;
    }
    if (this.isInvalidMarginModeOrderError(error, rawMessage)) {
      return this.buildInvalidMarginModeOrderMessage(opts.symbol);
    }

    const isAccidentalPriceError =
      normalizedMessage.includes('accidental price') ||
      (normalizedMessage.includes('accidental') && normalizedMessage.includes('price'));

    if (isAccidentalPriceError && opts.type === OrderType.Limit && typeof opts.price === 'number' && opts.price > 0) {
      const ticker = this.store.tickersBySymbol[opts.symbol];
      const markPrice = ticker?.mark || ticker?.last || ticker?.index;
      const hasMarkPrice = typeof markPrice === 'number' && Number.isFinite(markPrice) && markPrice > 0;
      const deviationPercent = hasMarkPrice ? Math.abs((opts.price - markPrice) / markPrice) * 100 : undefined;
      const roundedDeviationPercent =
        deviationPercent !== undefined && Number.isFinite(deviationPercent)
          ? Number(deviationPercent.toFixed(2))
          : undefined;
      const marketContext = hasMarkPrice
        ? ` Current mark is ~${markPrice}${
            roundedDeviationPercent !== undefined ? ` (deviation ${roundedDeviationPercent}%).` : '.'
          }`
        : '';
      return `${rawMessage}. Submitted ${opts.side} limit price ${opts.price} on ${opts.symbol}.${marketContext} Verify decimal placement and keep scaled bounds near market.`;
    }

    const isNoPositionReduceOnlyError =
      opts.type === OrderType.Limit &&
      Boolean(opts.reduceOnly) &&
      (normalizedMessage.includes('reduce only') ||
        normalizedMessage.includes('reduce-only') ||
        normalizedMessage.includes('nothing to reduce') ||
        normalizedMessage.includes('position does not exist') ||
        normalizedMessage.includes('no position') ||
        (normalizedMessage.includes('position') && normalizedMessage.includes('close')));

    if (!isNoPositionReduceOnlyError) {
      return rawMessage;
    }

    return this.buildNoPositionReduceOnlyMessage(opts.symbol, opts.side);
  };

  private assertReduceOnlyLimitHasPosition = async (opts: PlaceOrderOpts): Promise<void> => {
    const isReduceOnlyLimitOrder = opts.type === OrderType.Limit && Boolean(opts.reduceOnly);
    if (!isReduceOnlyLimitOrder) {
      return;
    }

    if (this.hasOpenReduceOnlyTargetPosition(opts.symbol, opts.side)) {
      return;
    }

    let refreshedPositions: Position[] | undefined;
    try {
      refreshedPositions = await this.fetchPositions();
      if (!this.isDisposed) {
        this.store.addOrUpdatePositionsFull(refreshedPositions);
      }
    } catch (error: any) {
      this.log(
        `[Lighter] Failed to refresh positions before reduce-only validation: ${error?.message || String(error)}`,
        LogSeverity.Warning,
      );
    }

    if (this.hasOpenReduceOnlyTargetPosition(opts.symbol, opts.side, refreshedPositions || this.store.positions)) {
      return;
    }

    const message = this.buildNoPositionReduceOnlyMessage(opts.symbol, opts.side);
    this.emitAndLogError(message);
    throw new Error(message);
  };

  private getOrderMinimumViolationReasons = (
    amount: number,
    executionPrice: number,
    minBaseAmount: number,
    minQuoteAmount: number,
  ): string | undefined => {
    const quoteAmount = amount * executionPrice;
    const isBelowBaseMinimum = minBaseAmount > 0 && amount < minBaseAmount;
    const isBelowQuoteMinimum = minQuoteAmount > 0 && quoteAmount < minQuoteAmount;
    if (!isBelowBaseMinimum && !isBelowQuoteMinimum) {
      return undefined;
    }

    return [
      isBelowBaseMinimum ? `base ${amount} < min_base_amount ${minBaseAmount}` : undefined,
      isBelowQuoteMinimum ? `quote ${quoteAmount} < min_quote_amount ${minQuoteAmount}` : undefined,
    ]
      .filter(Boolean)
      .join(', ');
  };

  private getPositivePriceCandidate = (...values: Array<number | undefined>): number | undefined => {
    return values.find((value) => typeof value === 'number' && Number.isFinite(value) && value > 0);
  };

  private resolveMarketOrderReferencePrice = (symbol: string, side: OrderSide): number => {
    const ticker = this.store.tickersBySymbol[symbol];
    if (!ticker) {
      throw new Error(`Ticker ${symbol} not found for market-style order`);
    }

    const midpoint =
      ticker.bid > 0 && ticker.ask > 0 && Number.isFinite(ticker.bid) && Number.isFinite(ticker.ask)
        ? (ticker.bid + ticker.ask) / 2
        : undefined;
    const sidePrice = side === OrderSide.Buy ? ticker.ask : ticker.bid;
    const oppositeSidePrice = side === OrderSide.Buy ? ticker.bid : ticker.ask;
    const referencePrice = this.getPositivePriceCandidate(
      sidePrice,
      ticker.mark,
      ticker.last,
      ticker.index,
      midpoint,
      oppositeSidePrice,
    );

    if (!referencePrice) {
      throw new Error(`No live Lighter price available for market-style order (${symbol})`);
    }

    return referencePrice;
  };

  private resolveStopSlippagePct = (opts: PlaceOrderOpts): number => {
    return (
      normalizeLighterStopSlippagePct(opts.maxSlippagePct) ??
      normalizeLighterStopSlippagePct(this.options.extra?.lighterStopSlippagePct) ??
      LIGHTER_DEFAULT_STOP_SLIPPAGE_PCT
    );
  };

  private resolveConditionalExecutionPrice = (triggerPrice: number, side: OrderSide, opts: PlaceOrderOpts): number => {
    const slippagePct = this.resolveStopSlippagePct(opts);
    const multiplier = side === OrderSide.Buy ? 1 + slippagePct / 100 : 1 - slippagePct / 100;
    return adjust(triggerPrice * multiplier, this.store.marketsBySymbol[opts.symbol].precision.price);
  };

  placeOrder = async (opts: PlaceOrderOpts) => {
    this.resolveCallerClientOrderId(opts);
    const signerAdapter = await this.ensureSignerAdapter();

    const market = this.store.marketsBySymbol[opts.symbol];

    if (!market) {
      throw new Error(`Market ${opts.symbol} not found`);
    }

    await this.assertReduceOnlyLimitHasPosition(opts);

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;
    const amountDecimals = getLighterAmountDecimals(market);
    const priceDecimals = getLighterPriceDecimals(market);
    const { minBaseAmount, minQuoteAmount } = getLighterOrderMinimums(market);

    const amount = adjust(opts.amount, pAmount);
    const price = opts.price ? adjust(opts.price, pPrice) : null;
    const isConditionalOrderType = opts.type === OrderType.StopLoss || opts.type === OrderType.TakeProfit;
    const isMarketOrderType = opts.type === OrderType.Market || opts.type === OrderType.TrailingStopLoss;

    const conditionalTriggerPrice = isConditionalOrderType
      ? adjust(opts.triggerPrice ?? opts.price ?? 0, pPrice)
      : null;
    if (isConditionalOrderType && (!conditionalTriggerPrice || conditionalTriggerPrice <= 0)) {
      throw new Error(`Trigger price is required for conditional Lighter orders (${opts.symbol})`);
    }
    // Lighter stop_loss / take_profit market variants reject GTC/FOK.
    // Normalize all conditional orders to IOC so chart-drag SL/TP submissions are valid.
    const normalizedTimeInForce = isConditionalOrderType ? OrderTimeInForce.ImmediateOrCancel : opts.timeInForce;

    // Lighter market orders are signed as slippage-bounded orders; use the freshest available live price.
    const executionPrice = isMarketOrderType
      ? adjust(
          this.resolveMarketOrderReferencePrice(opts.symbol, opts.side) *
            (opts.side === OrderSide.Buy ? 1.02 : 0.98),
          pPrice,
        )
      : isConditionalOrderType
        ? this.resolveConditionalExecutionPrice(conditionalTriggerPrice!, opts.side, opts)
        : price!;

    // Keep minimum guardrails for limit-style orders, but allow market orders
    // to follow exchange-side behavior (which currently accepts smaller notionals).
    if (!isMarketOrderType) {
      const reasons = this.getOrderMinimumViolationReasons(amount, executionPrice, minBaseAmount, minQuoteAmount);
      if (reasons) {
        throw new Error(
          `Lighter order amount is below exchange minimums for ${opts.symbol} (${reasons}). Increase order size or place closer to mark price.`,
        );
      }
    }

    const marketId = parseInt(market.id, 10);
    const orderId = this.reserveClientOrderIndex();
    const orderType: 'market' | 'limit' | 'stop_loss' | 'take_profit' = isConditionalOrderType
      ? opts.type === OrderType.StopLoss
        ? 'stop_loss'
        : 'take_profit'
      : isMarketOrderType
        ? 'market'
        : 'limit';
    const builderFeeConfig = resolveLighterBuilderFeeConfig(this.options, this.name);
    await this.ensurePartnerAttributionTierEligible(builderFeeConfig);
    await this.ensurePartnerAttributionApproved(builderFeeConfig);

    // Build order parameters for TransactionClient
    const orderParams = {
      orderId,
      marketId,
      isBuy: opts.side === OrderSide.Buy,
      price: toLighterScaledInt(executionPrice, priceDecimals),
      amount: toLighterScaledInt(amount, amountDecimals),
      postOnly: !isConditionalOrderType && opts.timeInForce === OrderTimeInForce.PostOnly,
      reduceOnly: opts.reduceOnly || false,
      orderType,
      triggerPrice: isConditionalOrderType ? toLighterScaledInt(conditionalTriggerPrice!, priceDecimals) : undefined,
      timeInForce: normalizedTimeInForce,
      ...(builderFeeConfig
        ? {
            integratorAccountIndex: builderFeeConfig.integratorAccountIndex,
            integratorMakerFee: builderFeeConfig.integratorMakerFee,
            integratorTakerFee: builderFeeConfig.integratorTakerFee,
          }
        : {}),
    };

    this.log(
      `Order request: ${JSON.stringify(orderParams, (_k, val) => (typeof val === 'bigint' ? val.toString() : val))}`,
      LogSeverity.Info,
    );

    const lots = amount > maxSize ? Math.ceil(amount / maxSize) : 1;
    const rest = amount > maxSize ? adjust(amount % maxSize, pAmount) : 0;
    const lotSize = adjust((amount - rest) / lots, pAmount);

    const lotAmounts = Array.from({ length: lots }, () => lotSize);
    if (rest > 0) {
      lotAmounts.push(rest);
    }
    if (!isMarketOrderType) {
      // Avoid sending a tiny trailing lot that exchange would reject.
      if (lotAmounts.length > 1 && minBaseAmount > 0) {
        const lastAmount = lotAmounts[lotAmounts.length - 1];
        if (lastAmount < minBaseAmount) {
          lotAmounts[lotAmounts.length - 2] = adjust(lotAmounts[lotAmounts.length - 2] + lastAmount, pAmount);
          lotAmounts.pop();
        }
      }

      for (const lotAmount of lotAmounts) {
        const reasons = this.getOrderMinimumViolationReasons(lotAmount, executionPrice, minBaseAmount, minQuoteAmount);
        if (reasons) {
          throw new Error(`Lighter order lot is below exchange minimums for ${opts.symbol} (${reasons}).`);
        }
      }
    }

    const shouldAttachTpSlSiblings =
      (opts.type === OrderType.Limit || opts.type === OrderType.Market) &&
      !opts.reduceOnly &&
      (opts.stopLoss !== undefined || opts.takeProfit !== undefined);
    const shouldUseGroupedAttachedTpSl = shouldAttachTpSlSiblings && opts.type === OrderType.Limit;
    if (shouldUseGroupedAttachedTpSl) {
      if (lotAmounts.length !== 1) {
        throw new Error('Lighter attached TP/SL limit orders cannot be split into multiple native lots.');
      }

      const inverseSide = opts.side === OrderSide.Buy ? OrderSide.Sell : OrderSide.Buy;
      const childRequests: LighterCreateOrderRequest[] = [];
      const childHints: Array<{ orderId: bigint; hint: Omit<LighterPlacedOrderHint, 'createdAt'> }> = [];
      const buildChildRequest = (childType: OrderType.StopLoss | OrderType.TakeProfit, triggerValue: number) => {
        const triggerPrice = adjust(triggerValue, pPrice);
        if (!Number.isFinite(triggerPrice) || triggerPrice <= 0) {
          throw new Error(`Trigger price is required for attached Lighter TP/SL orders (${opts.symbol})`);
        }
        const childOrderId = this.reserveClientOrderIndex();
        const childOrderType = childType === OrderType.StopLoss ? 'stop_loss' : 'take_profit';
        const executionGuardPrice = this.resolveConditionalExecutionPrice(triggerPrice, inverseSide, {
          ...opts,
          type: childType,
          side: inverseSide,
          price: triggerPrice,
          triggerPrice,
          amount: 0,
          reduceOnly: true,
          timeInForce: OrderTimeInForce.ImmediateOrCancel,
        });

        childHints.push({
          orderId: childOrderId,
          hint: {
            type: childType,
            side: inverseSide,
            reduceOnly: true,
            timeInForce: OrderTimeInForce.ImmediateOrCancel,
            price: triggerPrice,
            symbol: opts.symbol,
          },
        });

        childRequests.push({
          ...orderParams,
          orderId: childOrderId,
          isBuy: inverseSide === OrderSide.Buy,
          price: toLighterScaledInt(executionGuardPrice, priceDecimals),
          amount: 0n,
          postOnly: false,
          reduceOnly: true,
          orderType: childOrderType,
          triggerPrice: toLighterScaledInt(triggerPrice, priceDecimals),
          timeInForce: OrderTimeInForce.ImmediateOrCancel,
        });
      };

      if (opts.stopLoss !== undefined) {
        buildChildRequest(OrderType.StopLoss, opts.stopLoss);
      }
      if (opts.takeProfit !== undefined) {
        buildChildRequest(OrderType.TakeProfit, opts.takeProfit);
      }

      const groupedOrderIds = [orderId, ...childHints.map(({ orderId }) => orderId)].map((id) => id.toString());
      const groupedOrderRequest: LighterCreateGroupedOrdersRequest = {
        groupingType: childRequests.length === 2 ? 3 : 1,
        orders: [orderParams, ...childRequests],
        ...(builderFeeConfig
          ? {
              integratorAccountIndex: builderFeeConfig.integratorAccountIndex,
              integratorMakerFee: builderFeeConfig.integratorMakerFee,
              integratorTakerFee: builderFeeConfig.integratorTakerFee,
            }
          : {}),
      };

      this.log(
        `Grouped order request: ${JSON.stringify(groupedOrderRequest, (_k, val) =>
          typeof val === 'bigint' ? val.toString() : val,
        )}`,
        LogSeverity.Info,
      );

      this.rememberPlacedOrderHint(orderId, {
        type: opts.type,
        side: opts.side,
        reduceOnly: Boolean(opts.reduceOnly),
        timeInForce: normalizedTimeInForce,
        price: executionPrice,
        symbol: opts.symbol,
      });
      for (const childHint of childHints) {
        this.rememberPlacedOrderHint(childHint.orderId, childHint.hint);
      }

      try {
        await signerAdapter.createGroupedOrders(groupedOrderRequest);
        this.privateWebsocket.throttleFetchOrders();
        return groupedOrderIds;
      } catch (error: any) {
        for (const orderIdToForget of groupedOrderIds) {
          this.placedOrderHints.delete(orderIdToForget);
        }
        const rawOrderErrorMessage = this.extractOrderErrorMessage(error);
        if (this.isInvalidMarginModeOrderError(error, rawOrderErrorMessage)) {
          await this.refreshMarginModeStateAfterInvalidOrder(opts.symbol);
        }
        const normalizedPrimaryErrorMessage = this.normalizePlaceOrderErrorMessage(opts, error);
        const groupedErrorMessage = `Lighter attached TP/SL order placement failed: ${normalizedPrimaryErrorMessage}`;
        this.privateWebsocket.throttleFetchOrders();
        this.emitAndLogError(groupedErrorMessage);
        throw new Error(groupedErrorMessage);
      }
    }

    const orderIds: string[] = [];

    // Create orders in lots if needed.
    for (let i = 0; i < lotAmounts.length; i++) {
      const lotOrderId = i === 0 ? orderId : this.reserveClientOrderIndex();
      const lotAmount = toLighterScaledInt(lotAmounts[i], amountDecimals);
      this.rememberPlacedOrderHint(lotOrderId, {
        type: opts.type,
        side: opts.side,
        reduceOnly: Boolean(opts.reduceOnly),
        timeInForce: normalizedTimeInForce,
        price: isConditionalOrderType ? conditionalTriggerPrice! : executionPrice,
        symbol: opts.symbol,
      });
      try {
        await signerAdapter.createOrder({
          ...orderParams,
          orderId: lotOrderId,
          amount: lotAmount,
        });
        orderIds.push(lotOrderId.toString());
      } catch (error: any) {
        const rawOrderErrorMessage = this.extractOrderErrorMessage(error);
        if (this.isInvalidMarginModeOrderError(error, rawOrderErrorMessage)) {
          await this.refreshMarginModeStateAfterInvalidOrder(opts.symbol);
        }
        const normalizedPrimaryErrorMessage = this.normalizePlaceOrderErrorMessage(opts, error);

        this.placedOrderHints.delete(lotOrderId.toString());
        this.emitAndLogError(normalizedPrimaryErrorMessage);
        throw new Error(normalizedPrimaryErrorMessage);
      }

      if (i < lotAmounts.length - 1) {
        await sleep(100); // Small delay between orders
      }
    }

    if (shouldAttachTpSlSiblings) {
      const siblingOrders = this.deriveAlgoOrdersFromNormalOrdersOpts([opts]).map((siblingOrder) => ({
        ...siblingOrder,
        symbol: opts.symbol,
        reduceOnly: true,
        // Lighter stop_loss / take_profit orders require IOC TIF.
        // (GTT/GTC is only valid for stop/take-profit limit variants.)
        timeInForce: OrderTimeInForce.ImmediateOrCancel,
      }));

      for (const siblingOrder of siblingOrders) {
        try {
          const siblingOrderIds = await this.placeOrder(siblingOrder);
          orderIds.push(...siblingOrderIds);
        } catch (error: any) {
          const siblingType = siblingOrder.type === OrderType.StopLoss ? 'stop-loss' : 'take-profit';
          throw new Error(
            `Primary order was placed but failed to attach ${siblingType} sibling on Lighter: ${error?.message || String(error)}`,
          );
        }
      }
    }

    // Trigger order fetch via websocket handler
    this.privateWebsocket.throttleFetchOrders();

    return orderIds;
  };

  private shouldUseBatchedCreateOrders = (orders: PlaceOrderOpts[]): boolean => {
    if (orders.length <= 1) {
      return false;
    }

    const symbol = orders[0]?.symbol;
    return orders.every(
      (order) =>
        order.symbol === symbol &&
        order.type === OrderType.Limit &&
        !order.reduceOnly &&
        order.stopLoss === undefined &&
        order.takeProfit === undefined &&
        order.price !== undefined &&
        Number.isFinite(order.price) &&
        order.price > 0,
    );
  };

  private placeOrdersSequentially = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    const orderIds: string[] = [];
    for (const order of orders) {
      orderIds.push(...(await this.placeOrder(order)));
    }
    return orderIds;
  };

  placeOrders = async (orders: PlaceOrderOpts[]): Promise<string[]> => {
    this.assertCallerClientOrderIds(orders);
    if (!this.shouldUseBatchedCreateOrders(orders)) {
      return await this.placeOrdersSequentially(orders);
    }

    const market = this.store.marketsBySymbol[orders[0].symbol];
    if (!market) {
      return await this.placeOrdersSequentially(orders);
    }

    const maxSize = market.limits.amount.max;
    const pPrice = market.precision.price;
    const pAmount = market.precision.amount;
    const amountDecimals = getLighterAmountDecimals(market);
    const priceDecimals = getLighterPriceDecimals(market);
    const { minBaseAmount, minQuoteAmount } = getLighterOrderMinimums(market);
    const marketId = parseInt(market.id, 10);
    if (!Number.isFinite(marketId)) {
      return await this.placeOrdersSequentially(orders);
    }

    const builderFeeConfig = resolveLighterBuilderFeeConfig(this.options, this.name);
    await this.ensurePartnerAttributionTierEligible(builderFeeConfig);
    await this.ensurePartnerAttributionApproved(builderFeeConfig);

    const preparedOrders: Array<{
      orderId: bigint;
      request: LighterCreateOrderRequest;
      hint: Omit<LighterPlacedOrderHint, 'createdAt'>;
      sourceOrder: PlaceOrderOpts;
    }> = [];

    for (let index = 0; index < orders.length; index++) {
      const order = orders[index];
      const amount = adjust(order.amount, pAmount);
      const price = adjust(order.price!, pPrice);
      if (amount > maxSize) {
        return await this.placeOrdersSequentially(orders);
      }

      const reasons = this.getOrderMinimumViolationReasons(amount, price, minBaseAmount, minQuoteAmount);
      if (reasons) {
        throw new Error(
          `Lighter order amount is below exchange minimums for ${order.symbol} (${reasons}). Increase order size or place closer to mark price.`,
        );
      }

      const orderId = this.reserveClientOrderIndex();
      const normalizedTimeInForce = order.timeInForce;
      preparedOrders.push({
        orderId,
        sourceOrder: order,
        hint: {
          type: order.type,
          side: order.side,
          reduceOnly: false,
          timeInForce: normalizedTimeInForce,
          price,
          symbol: order.symbol,
        },
        request: {
          orderId,
          marketId,
          isBuy: order.side === OrderSide.Buy,
          price: toLighterScaledInt(price, priceDecimals),
          amount: toLighterScaledInt(amount, amountDecimals),
          postOnly: order.timeInForce === OrderTimeInForce.PostOnly,
          reduceOnly: false,
          orderType: 'limit',
          timeInForce: normalizedTimeInForce,
          ...(builderFeeConfig
            ? {
                integratorAccountIndex: builderFeeConfig.integratorAccountIndex,
                integratorMakerFee: builderFeeConfig.integratorMakerFee,
                integratorTakerFee: builderFeeConfig.integratorTakerFee,
              }
            : {}),
        },
      });
    }

    const signerAdapter = await this.ensureSignerAdapter();
    for (const preparedOrder of preparedOrders) {
      this.rememberPlacedOrderHint(preparedOrder.orderId, preparedOrder.hint);
    }

    try {
      await signerAdapter.createOrders(preparedOrders.map((preparedOrder) => preparedOrder.request));
      this.privateWebsocket.throttleFetchOrders();
      return preparedOrders.map((preparedOrder) => preparedOrder.orderId.toString());
    } catch (error: any) {
      for (const preparedOrder of preparedOrders) {
        this.placedOrderHints.delete(preparedOrder.orderId.toString());
      }
      const firstOrder = preparedOrders[0]?.sourceOrder ?? orders[0];
      const rawOrderErrorMessage = this.extractOrderErrorMessage(error);
      if (this.isInvalidMarginModeOrderError(error, rawOrderErrorMessage)) {
        await this.refreshMarginModeStateAfterInvalidOrder(firstOrder.symbol);
      }
      const normalizedErrorMessage = this.normalizePlaceOrderErrorMessage(firstOrder, error);
      const batchErrorMessage = `Lighter batch order placement failed: ${normalizedErrorMessage}. Please check open orders before retrying.`;
      this.privateWebsocket.throttleFetchOrders();
      this.emitAndLogError(batchErrorMessage);
      throw new Error(batchErrorMessage);
    }
  };

  updateOrder = async ({ order, update }: UpdateOrderOpts) => {
    // Lighter doesn't support order modification, need to cancel and recreate
    const inferredProtectiveType = this.inferStopLossTypeFromReduceOnlyLimit({
      ...order,
      price: 'price' in update ? update.price : order.price,
    });
    const newOrder: Writable<PlaceOrderOpts> = {
      symbol: order.symbol,
      type: inferredProtectiveType ?? order.type,
      side: order.side,
      price: order.price,
      amount: order.amount,
      reduceOnly: order.reduceOnly || false,
    };
    const existingMaxSlippagePct = (order as Partial<PlaceOrderOpts>).maxSlippagePct;
    if (existingMaxSlippagePct !== undefined) newOrder.maxSlippagePct = existingMaxSlippagePct;

    if ('tpTriggerType' in order) newOrder.tpTriggerType = order.tpTriggerType;
    if ('slTriggerType' in order) newOrder.slTriggerType = order.slTriggerType;

    if ('price' in update) newOrder.price = update.price;
    if ('amount' in update) newOrder.amount = update.amount;

    if (inferredProtectiveType && 'price' in update) {
      const ticker = this.store.tickersBySymbol[order.symbol];
      const referencePrice = this.getPositivePriceCandidate(ticker?.mark, ticker?.last, ticker?.index);
      if (referencePrice) {
        const isSellSide = order.side === OrderSide.Sell;
        const isValidStopLoss = isSellSide ? update.price <= referencePrice : update.price >= referencePrice;
        const isValidTakeProfit = isSellSide ? update.price >= referencePrice : update.price <= referencePrice;
        const isValid =
          inferredProtectiveType === OrderType.StopLoss
            ? isValidStopLoss
            : inferredProtectiveType === OrderType.TakeProfit
              ? isValidTakeProfit
              : true;

        if (!isValid) {
          throw new Error('Invalid Lighter protective trigger price for current market');
        }
      }
    }

    await this.cancelOrders([order]);
    return await this.placeOrder(newOrder);
  };

  cancelOrders = async (orders: Order[]) => {
    const invalidOrder = orders.find((order) => !parseLighterPositiveIntegerString(order.id));
    if (invalidOrder) {
      throw new Error(
        `Cannot cancel Lighter order ${invalidOrder.id}: expected a numeric client order id. Please wait for open-order sync before moving or cancelling this order.`,
      );
    }

    const signerAdapter = await this.ensureSignerAdapter();

    const cancelTargets = resolveLighterCancelTargets(orders, this.store.marketsBySymbol);
    const ordersToCancel = cancelTargets.map((target) => target.orderId);
    const allOrderIdsToRemove = orders.map((order) => order.id);

    // Add to blacklist BEFORE making API calls to prevent race conditions
    const now = Date.now();
    ordersToCancel.forEach((orderId) => {
      this.recentlyCancelledOrders.set(orderId, now);
    });

    // Immediately remove all cancelled orders from the store
    this.store.removeOrders(allOrderIdsToRemove.map((id) => ({ id })));

    // Group into lots of 10
    const lots = chunk(ordersToCancel, 10);

    await forEachSeries(lots, async (orderIds, i) => {
      if (i > 0) await sleep(1000);

      await forEachSeries(orderIds, async (orderId) => {
        const cancelTarget = cancelTargets.find((target) => target.orderId === orderId);
        if (!cancelTarget) {
          console.warn(`[Lighter] Cancel target not found for order ${orderId}`);
          return;
        }

        await signerAdapter.cancelOrder({
          orderId: BigInt(orderId),
          marketId: cancelTarget.marketId,
        });
      });
    });

    // Force a near-term reconciliation pass after cancel batches.
    this.privateWebsocket.throttleFetchOrders();
  };

  cancelSymbolOrders = async (symbol: string) => {
    const orders = this.store.orders.filter((o) => o.symbol === symbol);
    if (orders.length > 0) {
      await this.cancelOrders(orders);
    }
  };

  private static readonly VALID_LEVERAGE_TIERS = [1, 2, 3, 5, 10, 15, 20, 25, 30, 50] as const;

  private snapToLeverageTier = (requested: number): number => {
    const tiers = LighterExchange.VALID_LEVERAGE_TIERS;
    let closest: (typeof tiers)[number] = tiers[0];
    let closestDist = Math.abs(requested - closest);
    for (const tier of tiers) {
      const dist = Math.abs(requested - tier);
      if (dist < closestDist) {
        closest = tier;
        closestDist = dist;
      }
    }
    return closest;
  };

  setLeverage = async (symbol: string, inputLeverage: number, _side?: PositionSide) => {
    const market = this.store.marketsBySymbol[symbol];
    if (!market) throw new Error(`Market ${symbol} not found`);

    const signerAdapter = await this.ensureSignerAdapter();
    const marketId = Number.parseInt(String(market.id), 10);
    if (!Number.isFinite(marketId)) {
      throw new Error(`Invalid market id for ${symbol}: ${String(market.id)}`);
    }

    const clamped = Math.min(Math.max(inputLeverage, market.limits.leverage.min), market.limits.leverage.max);
    const leverage = this.snapToLeverageTier(clamped);
    const marginMode = this.getCurrentMarginModeForSymbol(symbol);
    try {
      await signerAdapter.updateLeverage({
        marketId,
        leverage,
        marginMode,
      });
    } catch (error: any) {
      const message = String(error?.message || error || '');
      const normalizedMessage = message.toLowerCase();
      const isJurisdictionRestricted = normalizedMessage.includes('restricted jurisdiction');
      const isInvalidMarginFraction = normalizedMessage.includes('invalid initial margin fraction');
      const isUnsupported =
        normalizedMessage.includes('updateleverage is not available') ||
        normalizedMessage.includes('signerclient export not found') ||
        normalizedMessage.includes('leverage signer unavailable in runtime') ||
        normalizedMessage.includes('failed to fetch dynamically imported module') ||
        normalizedMessage.includes('node:fs') ||
        isJurisdictionRestricted ||
        isInvalidMarginFraction;
      if (!isUnsupported) {
        await this.syncLeverageFromExchange(symbol);
        throw error;
      }
      this.log(`Lighter leverage update failed (${message}); refreshing exchange leverage state.`, LogSeverity.Warning);
      await this.syncLeverageFromExchange(symbol);

      if (isJurisdictionRestricted) {
        throw new Error(
          'Lighter leverage updates are restricted in this environment. Leverage remains unchanged on exchange.',
        );
      }
      if (isInvalidMarginFraction) {
        throw new Error(
          `Lighter rejected leverage ${leverage}x for ${symbol}. This market/account does not currently allow that leverage tier.`,
        );
      }
      throw new Error(message || `Failed to update leverage on Lighter for ${symbol}`);
    }
    await this.syncLeverageFromExchange(symbol);
    this.log(
      `Updated Lighter leverage via signed transaction to ${leverage}x (${marginMode === 1 ? 'isolated' : 'cross'} mode).`,
      LogSeverity.Info,
    );
  };

  changeMarginType = async (isolated: boolean, symbol?: string) => {
    if (!symbol) {
      throw new Error('Lighter margin mode update requires a symbol.');
    }
    const market = this.store.marketsBySymbol[symbol];
    if (!market) {
      throw new Error(`Market ${symbol} not found`);
    }

    const signerAdapter = await this.ensureSignerAdapter();
    const marketId = Number.parseInt(String(market.id), 10);
    if (!Number.isFinite(marketId)) {
      throw new Error(`Invalid market id for ${symbol}: ${String(market.id)}`);
    }

    const targetMarginMode: 0 | 1 = isolated ? 1 : 0;
    const leverage = this.getCurrentLeverageForSymbol(symbol, market);

    try {
      await signerAdapter.updateLeverage({
        marketId,
        leverage,
        marginMode: targetMarginMode,
      });
    } catch (error: any) {
      const message = String(error?.message || error || '');
      const normalizedMessage = message.toLowerCase();
      const isActivePositionOrOrderGuard =
        normalizedMessage.includes('21132') ||
        normalizedMessage.includes('margin mode change on a market with position or open order');
      if (isActivePositionOrOrderGuard) {
        throw new Error(
          `Lighter rejected margin mode switch for ${symbol}: close open positions and orders on this market first.`,
        );
      }
      throw error;
    }

    this.marginModeBySymbol.set(symbol, targetMarginMode);
    await this.syncLeverageFromExchange(symbol);
    this.log(
      `Updated Lighter margin mode for ${symbol} to ${isolated ? 'isolated' : 'cross'} via signed transaction.`,
      LogSeverity.Info,
    );
  };

  private syncLeverageFromExchange = async (symbol: string): Promise<void> => {
    try {
      const positions = await this.fetchPositions();
      if (this.isDisposed) {
        return;
      }
      this.store.addOrUpdatePositionsFull(positions);
    } catch (refreshError: any) {
      this.log(
        `[Lighter] Failed to refresh leverage state for ${symbol}: ${refreshError?.message || String(refreshError)}`,
        LogSeverity.Warning,
      );
    }
  };

  private resolvePositionLeverage = (p: Record<string, any>, market: Market): number => {
    const minLeverage = market.limits?.leverage?.min || 1;
    const maxLeverage = market.limits?.leverage?.max || 10;
    const clamp = (value: number) => Math.min(Math.max(value, minLeverage), maxLeverage);

    const directLeverageCandidates = [p.leverage, p.initialLeverage, p.initial_leverage];
    for (const candidate of directLeverageCandidates) {
      const parsed = Number.parseFloat(String(candidate ?? ''));
      if (Number.isFinite(parsed) && parsed > 0) {
        return clamp(parsed);
      }
    }

    const initialMarginFraction = parseLighterMarginFractionRatio(
      v(p, 'initial_margin_fraction') ?? v(p, 'initialMarginFraction'),
    );
    if (initialMarginFraction) {
      const derivedLeverage = 1 / initialMarginFraction;
      if (Number.isFinite(derivedLeverage) && derivedLeverage > 0) {
        return clamp(derivedLeverage);
      }
    }

    return maxLeverage;
  };

  private resolvePositionLiquidationPrice = (
    p: Record<string, any>,
    market: Market,
    side: PositionSide,
    entryPrice: number,
    contracts: number,
    leverage: number,
    marginMode: 0 | 1 | undefined,
  ): number => {
    const rawLiquidationPrice = Number.parseFloat(String(v(p, 'liquidationPrice') ?? v(p, 'liquidation_price') ?? 0));
    const rawIsFinite = Number.isFinite(rawLiquidationPrice) && rawLiquidationPrice > 0;
    const rawIsProfitSide =
      rawIsFinite &&
      Number.isFinite(entryPrice) &&
      entryPrice > 0 &&
      ((side === PositionSide.Long && rawLiquidationPrice >= entryPrice) ||
        (side === PositionSide.Short && rawLiquidationPrice <= entryPrice));

    if (rawIsFinite && (!rawIsProfitSide || marginMode !== 1)) {
      return rawLiquidationPrice;
    }

    if (
      marginMode !== 1 ||
      !Number.isFinite(entryPrice) ||
      entryPrice <= 0 ||
      !Number.isFinite(contracts) ||
      contracts <= 0
    ) {
      return rawIsFinite ? rawLiquidationPrice : 0;
    }

    const rawAllocatedMargin = Number.parseFloat(
      String(v(p, 'allocatedMargin') ?? v(p, 'allocated_margin') ?? v(p, 'marginSize') ?? v(p, 'margin_size') ?? ''),
    );
    const entryNotional = entryPrice * contracts;
    const fallbackMargin = Number.isFinite(leverage) && leverage > 0 ? entryNotional / leverage : Number.NaN;
    const allocatedMargin =
      Number.isFinite(rawAllocatedMargin) && rawAllocatedMargin > 0 ? rawAllocatedMargin : fallbackMargin;
    const maintenanceMarginFraction =
      parseLighterMarginFractionRatio(v(p, 'maintenanceMarginFraction') ?? v(p, 'maintenance_margin_fraction')) ??
      parseLighterMarginFractionRatio(
        (market.info as Record<string, any> | undefined)?.maintenance_margin_fraction ??
          (market.info as Record<string, any> | undefined)?.maintenanceMarginFraction,
      ) ??
      0;

    if (
      !Number.isFinite(allocatedMargin) ||
      allocatedMargin <= 0 ||
      !Number.isFinite(maintenanceMarginFraction) ||
      maintenanceMarginFraction < 0 ||
      maintenanceMarginFraction >= 1
    ) {
      return rawIsFinite ? rawLiquidationPrice : 0;
    }

    const derivedLiquidationPrice =
      side === PositionSide.Long
        ? (entryNotional - allocatedMargin) / (contracts * (1 - maintenanceMarginFraction))
        : (entryNotional + allocatedMargin) / (contracts * (1 + maintenanceMarginFraction));

    if (Number.isFinite(derivedLiquidationPrice) && derivedLiquidationPrice > 0) {
      return derivedLiquidationPrice;
    }

    return rawIsFinite ? rawLiquidationPrice : 0;
  };

  mapPosition(p: Record<string, any>) {
    const entryPrice = parseFloat(
      v(p, 'avgEntryPrice') || v(p, 'avg_entry_price') || v(p, 'entryPrice') || v(p, 'entry_price') || 0,
    );
    const rawContracts = parseFloat(p.size ?? p.position ?? p.qty ?? 0);
    const sign = Number(p.sign ?? 0);
    const contracts = Math.abs(rawContracts);
    if (!contracts) {
      return null;
    }

    const side =
      p.side === 'SHORT' || p.side === 'SELL' || sign < 0 || rawContracts < 0 ? PositionSide.Short : PositionSide.Long;

    const unrealizedPnl = parseFloat(v(p, 'unrealizedPnl') || v(p, 'unrealized_pnl') || 0);

    const marketId = p.marketId ?? p.market_id;
    const market = this.store.marketsById[marketId] || this.store.marketsBySymbol[p.symbol];

    if (!market) {
      this.log(`Market not found for position: ${p.marketId || p.symbol}`, LogSeverity.Warning);
      return null;
    }

    const marginMode = this.parseMarginMode(v(p, 'marginMode') ?? v(p, 'margin_mode'));
    if (marginMode !== undefined) {
      this.marginModeBySymbol.set(market.symbol, marginMode);
    }

    const leverage = this.resolvePositionLeverage(p, market);
    const liquidationPrice = this.resolvePositionLiquidationPrice(
      p,
      market,
      side,
      entryPrice,
      contracts,
      leverage,
      marginMode,
    );
    const id = createPositionId(market.symbol, side, PositionMode.OneWay);

    const position: Position = {
      id,
      symbol: market.symbol,
      side,
      entryPrice,
      notional: +contracts * +entryPrice,
      leverage,
      unrealizedPnl,
      realizedPnl: parseFloat(v(p, 'realizedPnl') || v(p, 'realized_pnl') || 0),
      contracts,
      liquidationPrice,
      positionMode: PositionMode.OneWay,
      marginType: this.marginModeToMarginType(marginMode),
      info: p,
      lastUpdate: virtualClock.getCurrentTimeMs(),
    };

    return position;
  }

  mapOrder(o: Record<string, any>) {
    const nestedOrder = o?.order && typeof o.order === 'object' ? o.order : undefined;
    const source = nestedOrder ? { ...o, ...nestedOrder } : o;
    const sourceEntries = Object.entries(source);

    const readLooseField = (...keys: string[]): unknown => {
      for (const key of keys) {
        const direct = v(source, key);
        if (direct !== undefined) {
          return direct;
        }
      }

      for (const [rawKey, rawValue] of sourceEntries) {
        const normalizedRawKey = rawKey.replace(/[_\s-]/g, '').toLowerCase();
        for (const key of keys) {
          const normalizedCandidate = key.replace(/[_\s-]/g, '').toLowerCase();
          if (normalizedRawKey === normalizedCandidate) {
            return rawValue;
          }
        }
      }
      return undefined;
    };
    const parseLooseNumber = (value: unknown): number | undefined => {
      if (value === undefined || value === null) {
        return undefined;
      }
      const parsed = Number.parseFloat(String(value).trim());
      if (!Number.isFinite(parsed)) {
        return undefined;
      }
      return parsed;
    };
    const parseLooseBoolean = (value: unknown): boolean | undefined => {
      if (typeof value === 'boolean') {
        return value;
      }
      if (typeof value === 'number') {
        return value !== 0;
      }
      if (typeof value === 'string') {
        const normalized = value.trim().toLowerCase();
        if (normalized === 'true' || normalized === '1') {
          return true;
        }
        if (normalized === 'false' || normalized === '0') {
          return false;
        }
      }
      return undefined;
    };

    const rawType = readLooseField('orderTypeValue', 'order_type_value', 'Type', 'orderType', 'order_type', 'type');
    const normalizedType = String(rawType ?? '')
      .toUpperCase()
      .replace(/[-\s]/g, '_');
    const rawTif = readLooseField(
      'timeInForceValue',
      'time_in_force_value',
      'TimeInForce',
      'timeInForce',
      'time_in_force',
    );
    const normalizedTif = String(rawTif ?? '')
      .toUpperCase()
      .replace(/-/g, '_');
    const postOnly = Boolean(readLooseField('postOnly', 'post_only'));

    const typeByNumericCode: Record<string, OrderType> = {
      '0': OrderType.Limit,
      '1': OrderType.Market,
      '2': OrderType.StopLoss,
      '3': OrderType.StopLoss,
      '4': OrderType.TakeProfit,
      '5': OrderType.TakeProfit,
    };
    const tifByNumericCode: Record<string, OrderTimeInForce> = {
      '0': OrderTimeInForce.ImmediateOrCancel,
      '1': OrderTimeInForce.GoodTillCancel,
      '2': OrderTimeInForce.PostOnly,
    };

    let type = ORDER_TYPE[normalizedType] || typeByNumericCode[String(rawType ?? '')] || OrderType.Limit;
    let timeInForce = postOnly
      ? OrderTimeInForce.PostOnly
      : ORDER_TIME_IN_FORCE[normalizedTif] || tifByNumericCode[String(rawTif ?? '')] || OrderTimeInForce.GoodTillCancel;

    const marketId = readLooseField('marketId', 'market_id', 'marketIndex', 'market_index');
    const symbol = String(readLooseField('symbol', 'marketSymbol', 'market_symbol') ?? '');
    const market = this.store.marketsById[marketId as any] || this.store.marketsBySymbol[symbol];

    if (!market) {
      console.warn('[Lighter] Market not found for order:', marketId || symbol);
      return [];
    }

    const parseOrderId = (...candidates: unknown[]): string | undefined => {
      const maxSignedInt64 = (1n << 63n) - 1n;
      for (const candidate of candidates) {
        if (candidate === undefined || candidate === null) {
          continue;
        }
        const raw = String(candidate).trim();
        if (!/^\d+$/.test(raw)) {
          continue;
        }
        try {
          const parsed = BigInt(raw);
          if (parsed >= 0n && parsed <= maxSignedInt64) {
            return parsed.toString();
          }
        } catch {
          // ignore invalid bigint candidate
        }
      }
      return undefined;
    };

    const rawOrderId = parseOrderId(
      readLooseField('orderId', 'order_id'),
      readLooseField('orderIndex', 'order_index'),
      readLooseField('OrderId', 'OrderIndex'),
    );
    let placedOrderHint = this.readPlacedOrderHint(rawOrderId);
    const existingOrder = rawOrderId ? this.store.orders.find((order) => order.id === rawOrderId) : undefined;

    const hasExplicitType = rawType !== undefined && rawType !== null && String(rawType).trim() !== '';
    if (type === OrderType.Limit && placedOrderHint?.type && placedOrderHint.type !== OrderType.Limit) {
      // Trust direct submission hints by id, even when snapshots mark Type=0.
      type = placedOrderHint.type;
    }
    if (type === OrderType.Limit && existingOrder?.type && existingOrder.type !== OrderType.Limit) {
      // Sticky type by id: once order is known as conditional, don't downgrade to limit.
      type = existingOrder.type;
    }
    if (!hasExplicitType && type === OrderType.Limit) {
      if (placedOrderHint?.type && placedOrderHint.type !== OrderType.Limit) {
        type = placedOrderHint.type;
      } else if (existingOrder && existingOrder.type !== OrderType.Limit) {
        type = existingOrder.type;
      }
    }

    const hasExplicitTif = rawTif !== undefined && rawTif !== null && String(rawTif).trim() !== '';
    if (!hasExplicitTif) {
      if (placedOrderHint?.timeInForce) {
        timeInForce = placedOrderHint.timeInForce;
      } else if (existingOrder?.timeInForce) {
        timeInForce = existingOrder.timeInForce;
      }
    }
    const rawAmount = readLooseField(
      'initialBaseAmount',
      'initial_base_amount',
      'quantity',
      'size',
      'baseAmount',
      'base_amount',
      'qty',
      'base_size',
      'origQty',
      'orderQuantity',
      'amount',
    );
    const rawFilled = readLooseField(
      'filledBaseAmount',
      'filled_base_amount',
      'filledQuantity',
      'filledAmount',
      'filledSize',
      'filled_quantity',
      'filledQty',
      'filled_base_size',
      'executedQty',
      'cumQty',
    );
    const rawRemaining = readLooseField(
      'remainingBaseAmount',
      'remaining_base_amount',
      'remainingQuantity',
      'remainingAmount',
      'remainingSize',
      'remaining_quantity',
      'remainingQty',
      'remaining_base_size',
      'leavesQty',
      'openQty',
    );
    const rawQuoteAmount = readLooseField('quoteAmount', 'quote_amount', 'quoteQuantity', 'quote_quantity', 'notional');
    const rawRemainingQuote = readLooseField(
      'remainingQuoteAmount',
      'remaining_quote_amount',
      'remainingNotional',
      'remaining_notional',
      'remainingQuoteQty',
      'leavesQuoteQty',
      'openQuoteQty',
    );
    const parsedAmount = parseLooseNumber(rawAmount);
    const parsedFilled = parseLooseNumber(rawFilled);
    const parsedRemaining = parseLooseNumber(rawRemaining);
    const parsedQuoteAmount = parseLooseNumber(rawQuoteAmount);
    const parsedRemainingQuote = parseLooseNumber(rawRemainingQuote);
    const amount = parsedAmount !== undefined ? parsedAmount : (parsedFilled ?? 0) + (parsedRemaining ?? 0);
    const filled =
      parsedFilled !== undefined
        ? parsedFilled
        : parsedAmount !== undefined && parsedRemaining !== undefined
          ? Math.max(parsedAmount - parsedRemaining, 0)
          : 0;
    let remaining = parsedRemaining !== undefined ? parsedRemaining : Math.max(amount - filled, 0);
    const rawSide = String(readLooseField('side') ?? '').toUpperCase();
    const isBuyFlag = parseLooseBoolean(readLooseField('isBuy'));
    const isAskFlag = parseLooseBoolean(readLooseField('isAsk', 'is_ask'));
    const sideFromFlags =
      ORDER_SIDE[rawSide] ||
      (isAskFlag !== undefined
        ? isAskFlag
          ? OrderSide.Sell
          : OrderSide.Buy
        : isBuyFlag !== undefined
          ? isBuyFlag
            ? OrderSide.Buy
            : OrderSide.Sell
          : existingOrder?.side || OrderSide.Sell);
    const limitPrice = parseLooseNumber(readLooseField('price', 'Price', 'limitPrice', 'limit_price', 'orderPrice'));
    const triggerPrice = parseLooseNumber(
      readLooseField(
        'triggerPrice',
        'trigger_price',
        'TriggerPrice',
        'stopPrice',
        'stop_price',
        'triggerPx',
        'trigger_px',
      ),
    );

    if (type === OrderType.Limit && placedOrderHint?.type && placedOrderHint.type !== OrderType.Limit) {
      type = placedOrderHint.type;
    }
    if (!hasExplicitTif && placedOrderHint?.timeInForce) {
      timeInForce = placedOrderHint.timeInForce;
    }

    if (type === OrderType.Limit && triggerPrice !== undefined && triggerPrice > 0) {
      if (placedOrderHint?.type === OrderType.StopLoss || placedOrderHint?.type === OrderType.TakeProfit) {
        type = placedOrderHint.type;
      } else if (existingOrder?.type === OrderType.StopLoss || existingOrder?.type === OrderType.TakeProfit) {
        type = existingOrder.type;
      }
    }

    if (type === OrderType.StopLoss || type === OrderType.TakeProfit) {
      if (placedOrderHint?.timeInForce) {
        timeInForce = placedOrderHint.timeInForce;
      } else if (existingOrder?.timeInForce) {
        timeInForce = existingOrder.timeInForce;
      }
    }

    const isProtectiveOrder = type === OrderType.StopLoss || type === OrderType.TakeProfit;
    let price = (isProtectiveOrder && triggerPrice !== undefined && triggerPrice > 0 ? triggerPrice : limitPrice) ?? 0;
    if ((!Number.isFinite(price) || price <= 0) && placedOrderHint?.price && placedOrderHint.price > 0) {
      price = placedOrderHint.price;
    }
    const rawStatus = String(readLooseField('status') ?? '').toUpperCase();
    const status = ORDER_STATUS[rawStatus] || OrderStatus.Open;
    const isOpenLikeStatus = status === OrderStatus.Open || rawStatus === 'PENDING' || rawStatus === 'PARTIALLY_FILLED';

    // Some payload variants can carry stale/empty remaining fields while status is OPEN.
    // Fall back to full order amount so chart/order widgets keep quantity badges visible.
    if (isOpenLikeStatus && remaining <= 0 && amount > 0) {
      remaining = amount;
    }

    // Recover missing quantity fields from quote + price for open orders.
    if (isOpenLikeStatus && amount <= 0 && parsedRemainingQuote !== undefined && price > 0) {
      remaining = parsedRemainingQuote / price;
    }

    const remainingNotional =
      parsedRemainingQuote !== undefined
        ? parsedRemainingQuote
        : parsedQuoteAmount !== undefined && amount > 0
          ? (remaining / amount) * parsedQuoteAmount
          : remaining * price;

    const parsedReduceOnly = parseLooseBoolean(readLooseField('reduceOnly', 'reduce_only'));
    const resolvedSide = sideFromFlags || placedOrderHint?.side || existingOrder?.side || OrderSide.Sell;

    const resolvedReduceOnly = parsedReduceOnly ?? placedOrderHint?.reduceOnly ?? existingOrder?.reduceOnly ?? false;
    const inferredProtectiveType = this.inferStopLossTypeFromReduceOnlyLimit({
      symbol: market.symbol,
      type,
      side: resolvedSide,
      price,
      reduceOnly: resolvedReduceOnly,
    });
    if (inferredProtectiveType) {
      type = inferredProtectiveType;
    }

    const orders: Order[] = [
      {
        id: String(rawOrderId ?? ''),
        status,
        symbol: market.symbol,
        type,
        side: resolvedSide,
        price,
        amount,
        filled,
        reduceOnly: resolvedReduceOnly,
        remaining,
        extra: {
          remainingNative: remaining,
          remainingNotional: Number.isFinite(remainingNotional) ? remainingNotional : 0,
        },
        info: source,
        timeInForce,
        lastUpdate: virtualClock.getCurrentTimeMs(),
      },
    ];

    if (!rawOrderId) {
      return [];
    }

    return orders;
  }
}
