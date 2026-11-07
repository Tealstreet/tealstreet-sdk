import type { ExchangeAccountConfig, ExchangeOptions } from './types';

import { getExchangeMetadata } from './exchangeMetadata';
import { normalizeBybitSiteId } from './exchanges/bybit/bybit.site';
import { assertBetaVenue } from './exchanges/rust/betaVenues';
import { createExchange } from './index';
import { getTransport } from './transport/registry';
import { resolveAccountTransport } from './transport/tealstreet/transportResolution';

type KucoinBrokerConfig = NonNullable<NonNullable<ExchangeOptions['extra']>['kucoinBroker']>;

const KUCOIN_BROKER_ENV_PREFIX_BY_EXCHANGE = {
  kucoin: 'NEXT_PUBLIC_KUCOIN_FUTURES_BROKER',
  kucoinspot: 'NEXT_PUBLIC_KUCOIN_SPOT_BROKER',
} as const;

const getPublicEnv = (name: string): string | undefined => {
  const value = typeof process === 'undefined' ? undefined : process.env?.[name];
  if (typeof value !== 'string') {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed || undefined;
};

const parseKucoinBrokerVerify = (value: string | undefined): boolean | undefined => {
  if (!value) {
    return undefined;
  }

  const normalized = value.trim().toLowerCase();
  if (normalized === 'true' || normalized === '1' || normalized === 'yes') {
    return true;
  }
  if (normalized === 'false' || normalized === '0' || normalized === 'no') {
    return false;
  }

  return undefined;
};

const isLighterExchange = (exchangeName: ExchangeAccountConfig['exchange']): boolean =>
  exchangeName === 'lighter' || exchangeName === 'lighter-robinhood';

const getKucoinBrokerEnv = (prefix: string, suffix: 'PARTNER' | 'KEY' | 'NAME' | 'VERIFY'): string | undefined =>
  getPublicEnv(`${prefix}_${suffix}`) ?? getPublicEnv(`NEXT_PUBLIC_KUCOIN_BROKER_${suffix}`);

const resolvePublicKucoinBrokerConfig = (
  exchange: ExchangeAccountConfig['exchange'],
): KucoinBrokerConfig | undefined => {
  if (exchange !== 'kucoin' && exchange !== 'kucoinspot') {
    return undefined;
  }

  const prefix = KUCOIN_BROKER_ENV_PREFIX_BY_EXCHANGE[exchange];
  const partner = getKucoinBrokerEnv(prefix, 'PARTNER');
  const brokerKey = getKucoinBrokerEnv(prefix, 'KEY');
  const brokerName = getKucoinBrokerEnv(prefix, 'NAME');
  if (!partner || !brokerKey || !brokerName) {
    return undefined;
  }

  const verify = parseKucoinBrokerVerify(getKucoinBrokerEnv(prefix, 'VERIFY'));

  return {
    partner,
    brokerKey,
    brokerName,
    ...(verify !== undefined ? { verify } : {}),
  };
};

/**
 * Convert an ExchangeAccountConfig to ExchangeOptions.
 * This is a pure function that maps account fields to the options format
 * expected by createExchange, using exchange metadata to determine which
 * fields are required.
 */
export function accountToExchangeOptions(
  account: ExchangeAccountConfig,
  overrides?: Partial<ExchangeOptions>,
): ExchangeOptions {
  const { extra: overrideExtra, ...optionOverrides } = overrides ?? {};
  const metadata = getExchangeMetadata(account.exchange);
  const shouldForwardApplicationId =
    metadata.requiresApplicationId || isLighterExchange(account.exchange) || account.exchange === 'asterdex';
  const shouldForwardApiKeyIndex = metadata.requiresApiKeyIndex || isLighterExchange(account.exchange);
  const publicKucoinBrokerConfig = resolvePublicKucoinBrokerConfig(account.exchange);
  const hyrotraderMode =
    account.exchange === 'hyrotrader'
      ? account.hyrotraderDemo
        ? ('demo' as const)
        : ('production' as const)
      : undefined;
  const extra = {
    ...(publicKucoinBrokerConfig ? { kucoinBroker: publicKucoinBrokerConfig } : {}),
    ...(hyrotraderMode ? { hyrotraderMode } : {}),
    ...overrideExtra,
  };
  const hasExtra = Object.keys(extra).length > 0;

  // Runtime sanity checks for impossible metadata configurations
  if (metadata.requiresCors && metadata.forbidCors) {
    throw new Error(`[${account.exchange}] Invalid metadata: requiresCors and forbidCors are both true`);
  }
  if (metadata.enforceWsTunnel && metadata.supportsTunnel === false) {
    console.warn(
      `[${account.exchange}] Metadata warning: enforceWsTunnel is true but supportsTunnel is false. ` +
        `enforceWsTunnel takes precedence — tunnel will be enabled.`,
    );
  }

  // Resolve proxy / tunnel / region from preferences + metadata (single source
  // of truth — see transport/tealstreet/transportResolution.ts).
  const { tealstreetProxy, tealstreetTunnel, proxyRegion } = resolveAccountTransport(account, metadata);

  // When a secondary (read-only) key is configured, its reads egress the public AWS
  // CORS proxy while writes stay on the primary/whitelisted transport.
  const secondaryProxyUrl = account.secondary_apiKey ? getTransport(overrides ?? {}).publicCorsProxy() : undefined;
  const bybitSiteId = normalizeBybitSiteId(account.exchange, account.bybitSiteId);

  const options: ExchangeOptions = {
    key: account.apiKey,
    secret: account.apiSecret,

    // Passphrase (OKX, Bitget, BloFin, KuCoin, etc.)
    ...(metadata.requiresPassphrase && account.passphrase ? { passphrase: account.passphrase } : {}),

    // Secondary API keys
    ...(account.secondary_apiKey ? { secondary_key: account.secondary_apiKey } : {}),
    ...(account.secondary_apiSecret ? { secondary_secret: account.secondary_apiSecret } : {}),
    ...(account.secondary_passphrase ? { secondary_passphrase: account.secondary_passphrase } : {}),
    ...(secondaryProxyUrl ? { secondaryProxyUrl } : {}),

    // Proxy settings (resolved above via resolveAccountTransport)
    tealstreetProxy: tealstreetProxy,
    tealstreetTunnel: tealstreetTunnel,
    tealstreetWsRoute: account.tealstreetWsRoute,
    ...(proxyRegion ? { proxyRegion } : {}),

    // Testnet
    ...(account.testnet ? { testnet: account.testnet } : {}),
    ...(bybitSiteId ? { bybitSiteId } : {}),

    // Spot opt-in, for venues that serve spot + derivatives from one
    // connection. Only forwarded when the venue actually offers the toggle,
    // so a stale flag on an unrelated account can't turn anything on.
    ...(metadata.supportsSpotToggle && account.enableSpotMarkets ? { enableSpotMarkets: true } : {}),
    ...(metadata.supportsOptionsToggle && account.enableOptionMarkets ? { enableOptionMarkets: true } : {}),
    ...(metadata.supportsInverseToggle && account.enableInverseMarkets ? { enableInverseMarkets: true } : {}),

    // Exchange-specific fields based on metadata
    ...(shouldForwardApplicationId && account.applicationId ? { applicationId: account.applicationId } : {}),
    ...(shouldForwardApiKeyIndex && account.apiKeyIndex ? { apiKeyIndex: account.apiKeyIndex } : {}),
    ...(metadata.supportsVault && account.vault ? { vault: account.vault } : {}),
    ...(metadata.requiresOmniKeySeed && account.omniKeySeed ? { omniKeySeed: account.omniKeySeed } : {}),

    // Paradex
    ...(metadata.requiresParadexAddress
      ? {
          paradexAddress: account.paradexAddress,
          subkeyAddress: account.subkeyAddress,
        }
      : {}),

    // GRVT - pass each field independently if set
    ...(account.exchange === 'grvt' && account.grvtSignerAddress
      ? { grvtSignerAddress: account.grvtSignerAddress }
      : {}),
    ...(account.exchange === 'grvt' && account.grvtTradingAccountId
      ? { grvtTradingAccountId: account.grvtTradingAccountId }
      : {}),

    // Subaccount
    ...(metadata.supportsSubaccount && account.subaccount ? { subaccount: account.subaccount } : {}),

    // Orderly exchange ID
    ...(account.orderlyExchangeId ? { orderlyExchangeId: account.orderlyExchangeId } : {}),
    ...(hasExtra ? { extra } : {}),

    // Apply any overrides
    ...optionOverrides,
  };

  return options;
}

/**
 * Create an exchange instance from an ExchangeAccountConfig.
 * This is the main entry point for creating exchanges from saved account configs.
 *
 * @param account - The saved exchange account configuration
 * @param overrides - Optional ExchangeOptions to override/extend the defaults
 * @returns Promise resolving to the exchange instance
 */
export async function createExchangeFromAccount(account: ExchangeAccountConfig, overrides?: Partial<ExchangeOptions>) {
  assertBetaVenue(account.exchange);
  const options = accountToExchangeOptions(account, overrides);
  return createExchange(account.exchange, options);
}
