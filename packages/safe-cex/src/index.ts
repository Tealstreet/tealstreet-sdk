/**
 * Safe-CEX Exchange Factory
 *
 * This module provides the main entry point for creating exchange instances.
 * All exchanges are dynamically imported to reduce initial bundle size.
 * Exchange implementations are only loaded when createExchange() is called.
 */

import type { BaseExchange } from './exchanges/base';
import type { Store } from './store/store.interface';
import type { ExchangeName, ExchangeOptions, PublicExchangeOptions } from './types';

import { getExchangeMetadata } from './exchangeMetadata';
import { exchangeRegistry, getRegisteredExchangeNames } from './exchanges/registry';
import { assertBetaAccount, assertBetaVenue } from './exchanges/rust/betaVenues';
import { DefaultStore } from './store/store.base';
import { getTransport } from './transport/registry';
import { virtualClock } from './utils/virtualClock';

export {
  exchangeMetadata,
  exchangePrettyNames,
  getExchangeMetadata,
  isExchangeSupportedOnMobile,
} from './exchangeMetadata';

export { serializeCommandError, deserializeCommandError, exchangeErrorCategory } from './commandOutcome';
export type { SerializedCommandError, CommandOutcome, ExchangeErrorKind } from './commandOutcome';
export { historyCompletenessOf, withHistoryCompleteness, copyHistoryCompleteness } from './historyCompleteness';
export { describeStoppedAlgos } from './nukeResult';
export type { HistoryCompleteness, HistoricalRows } from './historyCompleteness';

export { ExchangeError, builderAuthorizationOf } from '@tealstreet/safe-cex-wasm/host';
export type { BuilderAuthorizationDetail } from '@tealstreet/safe-cex-wasm/host';

export { RustPublicFeedProvider } from './exchanges/rust/publicFeed';
export type { RustPublicFeedPolicy } from './exchanges/rust/publicFeed';

export { createEngineTransport, resolveEngineRoutingPlan } from './transport/engine';
export type { EngineRoutingPlan } from './transport/engine';

// Cache for loaded exchange classes
const exchangeCache = new Map<ExchangeName, typeof BaseExchange>();

/**
 * Load an exchange class dynamically
 * Returns cached class if already loaded
 */
async function loadExchange(name: ExchangeName): Promise<typeof BaseExchange> {
  assertBetaVenue(name);
  // Return cached if already loaded
  const cached = exchangeCache.get(name);
  if (cached) return cached;

  const loader = exchangeRegistry[name];
  if (!loader) {
    throw new Error(`Unknown exchange: ${name}`);
  }

  const Exchange = await loader();
  exchangeCache.set(name, Exchange);
  return Exchange;
}

/**
 * Create an exchange instance
 *
 * Exchanges are dynamically imported on first use to reduce initial bundle size.
 * Subsequent calls for the same exchange type use the cached class.
 *
 * @param exchangeName - The name of the exchange to create
 * @param options - Exchange configuration options
 * @param store - Optional store instance (defaults to DefaultStore)
 * @returns Promise resolving to the exchange instance
 *
 * @example
 * ```typescript
 * const exchange = await createExchange('binance', {
 *   key: 'your-api-key',
 *   secret: 'your-api-secret',
 * });
 * await exchange.start();
 * ```
 */
export const createExchange = async (
  exchangeName: ExchangeName,
  options: ExchangeOptions,
  store?: Store,
): Promise<BaseExchange> => {
  assertBetaAccount(exchangeName, options);
  const Exchange = await loadExchange(exchangeName);
  // Rejected venues never start the clock, load WASM or construct transports.
  const exchange = new Exchange(exchangeName, { ...options, engine: 'rust' }, store || new DefaultStore());
  virtualClock.start();
  return exchange;
};

export const buildPublicExchangeOptions = (
  exchangeName: ExchangeName,
  options?: Partial<Omit<PublicExchangeOptions, 'publicDataOnly'>>,
): ExchangeOptions => {
  const metadata = getExchangeMetadata(exchangeName);
  const { forceAllProxiesOff, ...exchangeOptions } = options || {};

  // Exchanges with requiresCors/enforceWsTunnel have IP-whitelisted APIs.
  // On server-side, route REST calls through the exchange proxy (whose IPs
  // are whitelisted on the exchange side). The proxy itself must accept the
  // caller's IP without JWT (e.g. K8s NAT gateway whitelisted in Lambda@Edge).
  const needsProxy = !forceAllProxiesOff && (metadata.requiresCors || metadata.enforceWsTunnel);
  const serverProxyUrl = needsProxy ? getTransport(exchangeOptions).serverProxyUrl(exchangeName) : undefined;
  const proxyOpts: Partial<ExchangeOptions> = serverProxyUrl ? { overrideProxyUrl: serverProxyUrl } : {};
  const disabledProxyOpts: Partial<ExchangeOptions> = forceAllProxiesOff
    ? {
        overrideProxyUrl: undefined,
        tealstreetProxy: false,
        tealstreetTunnel: false,
        tealstreetWsRoute: false,
      }
    : {};

  return {
    key: '',
    secret: '',
    ...proxyOpts,
    ...exchangeOptions,
    ...disabledProxyOpts,
    publicDataOnly: true,
  };
};

/**
 * Preload an exchange module before it's needed
 *
 * Call this early if you know a user will need a specific exchange soon,
 * such as when they hover over an exchange selector in the UI.
 *
 * @param name - The exchange name to preload
 *
 * @example
 * ```typescript
 * // Preload on hover
 * onMouseEnter={() => preloadExchange('binance')}
 * ```
 */
export const preloadExchange = async (name: ExchangeName): Promise<void> => {
  await loadExchange(name);
};

/**
 * Preload multiple exchange modules
 *
 * @param names - Array of exchange names to preload
 */
export const preloadExchanges = async (names: ExchangeName[]): Promise<void> => {
  await Promise.all(names.map(loadExchange));
};

/**
 * Check if an exchange class has been loaded
 *
 * @param name - The exchange name to check
 * @returns true if the exchange class is cached
 */
export const isExchangeLoaded = (name: ExchangeName): boolean => {
  return exchangeCache.has(name);
};

/**
 * Get list of all available exchange names
 *
 * @returns Array of all registered exchange names
 */
export const getAvailableExchanges = (): ExchangeName[] => {
  return getRegisteredExchangeNames();
};

/**
 * Create a public-data-only exchange instance (no API keys required)
 *
 * Convenience factory that sets `publicDataOnly: true` and provides
 * empty key/secret defaults. The resulting exchange will only connect
 * public WebSockets and fetch public REST data.
 *
 * @param exchangeName - The name of the exchange to create
 * @param options - Optional exchange configuration (key/secret not required)
 * @param store - Optional store instance (defaults to DefaultStore)
 * @returns Promise resolving to the exchange instance
 *
 * @example
 * ```typescript
 * const exchange = await createPublicExchange('binance');
 * await exchange.start();
 * ```
 */
export const createPublicExchange = async (
  exchangeName: ExchangeName,
  options?: Partial<Omit<PublicExchangeOptions, 'publicDataOnly'>>,
  store?: Store,
): Promise<BaseExchange> => {
  assertBetaVenue(exchangeName);
  return createExchange(exchangeName, buildPublicExchangeOptions(exchangeName, options), store);
};

export { setTelemetrySink } from './telemetry';
export type { TelemetryContext, TelemetrySink } from './telemetry';

/**
 * Account-based exchange creation
 */
export { normalizeExchangeAccountConfig } from './accounts/normalizeExchangeAccountConfig';
export {
  buildExchangeAccountConfig,
  getExchangeAccountCredentialSchema,
  getSupportedExchangeAccountCredentialSchemas,
  getSupportedExchangeAccountNames,
  ExchangeAccountConfigError,
  type ExchangeAccountCredentialField,
  type ExchangeAccountCredentialInput,
  type ExchangeAccountCredentialSchema,
} from './accounts/buildExchangeAccountConfig';
export { accountToExchangeOptions, createExchangeFromAccount } from './createExchangeFromAccount';
export type {
  ExchangeAccountConfig,
  PayloadSigner,
  PayloadSignerRequest,
  PayloadSignerResult,
  PublicExchangeOptions,
} from './types';
export type {
  BybitBboLevel,
  BybitBboOrderOpts,
  BybitBboSideType,
  BybitPlaceOrderOpts,
  BybitTickSizeMarketOrderOpts,
} from './exchanges/bybit/bybit.order-params';

/**
 * Hyperliquid builder fee utilities
 * Used for builder code integration (fee sharing)
 */
export {
  approveBuilderFeeWithBrowserWallet,
  checkBuilderFeeApproved,
  builderFeeRateFromTenthsBps,
  createBuilderInfo,
  DEFAULT_BUILDER_FEE_RATE,
  DEFAULT_BUILDER_FEE_TENTHS_BPS,
  getBuilderAddress,
  resolveHyperliquidBuilder,
  signBuilderFeeApproval,
  submitBuilderFeeApproval,
  TEALSTREET_BUILDER_ADDRESS_MAINNET,
  TEALSTREET_BUILDER_ADDRESS_TESTNET,
} from './exchanges/hyperliquid/hyperliquid.builderFee';

export type {
  BuilderFeeApprovalResult,
  BuilderFeeCheckResult,
  BuilderInfo,
} from './exchanges/hyperliquid/hyperliquid.types';

/**
 * Aster builder attribution and approval utilities
 */
export {
  approveAsterBuilderFeeWithBrowserWallet,
  buildAsterApproveBuilderTypedData,
  checkAsterBuilderApproved,
  getAsterBuilderApprovalConfig,
} from './exchanges/asterdex/asterdex.builder-approval';
export {
  ASTER_BUILDER_FEE_RATE_CAP,
  DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER,
  DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER,
  getAsterBuilderApprovalFeeRate,
  getAsterBuilderFeeRateForOrder,
  resolveAsterBuilderFeeConfig,
  TEALSTREET_ASTER_BUILDER_ADDRESS_MAINNET,
  TEALSTREET_ASTER_BUILDER_ADDRESS_TESTNET,
} from './exchanges/asterdex/asterdex.builder-fee';

export type {
  AsterBuilderApprovalCheckResult,
  AsterBuilderApprovalResult,
  AsterBuilderApprovalStatus,
} from './exchanges/asterdex/asterdex.builder-approval';

export type { AsterBuilderFeeConfig } from './exchanges/asterdex/asterdex.builder-fee';

/**
 * Hyperliquid agent (API) wallet utilities
 * Used for in-app API wallet creation
 */
export {
  connectBrowserWallet,
  createAgentWalletWithBrowserWallet,
  generateAgentWalletName,
  hasEthereumWallet,
} from './exchanges/hyperliquid/hyperliquid.agent';

export type { AgentWalletCreationResult } from './exchanges/hyperliquid/hyperliquid.agent';

/**
 * GRVT builder authorization utilities
 * Used for builder code integration (fee sharing)
 */
export {
  authorizeGrvtBuilderWithBrowserWallet,
  checkGrvtBuilderAuthorized,
  createGrvtBuilderInfo,
  DEFAULT_GRVT_BUILDER_FEE_RATE,
  getConnectedWalletAddress,
  getGrvtBuilderAddress,
  grvtBuilderFeeRateToRaw,
  GrvtBuilderUnauthorizedError,
  isBrowserWalletAvailable,
  requestWalletConnection,
  TEALSTREET_GRVT_BUILDER_ADDRESS,
  TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET,
  TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET,
} from './exchanges/grvt/grvt.builder';

export type {
  GrvtBuilderAuthorizationResult,
  GrvtBuilderCheckResult,
  GrvtBuilderInfo,
  GrvtBuilderUnauthorizedReason,
} from './exchanges/grvt/grvt.builder';

/**
 * Lighter builder/integrator approval utilities
 */
export {
  approveLighterBuilderFeeWithBrowserWallet,
  buildLighterApproveIntegratorL1Message,
  checkLighterBuilderApproved,
  createLighterL1SignatureHelperUrl,
  DEFAULT_LIGHTER_APPROVAL_FEE_CAPS,
  DEFAULT_LIGHTER_APPROVAL_FEE_VALUE,
  prepareLighterBuilderApprovalContext,
  signLighterApproveIntegratorWithBrowserWallet,
  submitLighterBuilderApprovalWithL1Signature,
} from './exchanges/lighter/lighter.builder-approval';

export {
  LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE,
  LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE,
} from './exchanges/lighter/lighter.account-tier';

export type { LighterAccountTier } from './exchanges/lighter/lighter.account-tier';

export type {
  LighterBuilderApprovalCheckResult,
  LighterBuilderApprovalContext,
  LighterBuilderApprovalFeeCaps,
  LighterBuilderApprovalResult,
} from './exchanges/lighter/lighter.builder-approval';

export {
  CandleCache,
  cleanCandles,
  createExchangeCandleFetcher,
  fetchOHLCVWithExchange,
  getCandleCacheKey,
  KeyValueCandleCacheBackend,
  MemoryCandleCacheBackend,
} from './candleCache';

export type {
  CandleCacheBackend,
  CandleCacheBackendReadMode,
  CandleCacheChunkAccessOptions,
  CandleCacheChunkRecord,
  CandleCacheExchangeFetchRequest,
  CandleCacheFetchRequest,
  CandleCacheKeyValueStore,
  CandleCacheOptions,
  CandleCacheRangeLookup,
  CandleCacheSparseChunk,
} from './candleCache';
