import type { ExchangeAccountConfig, ExchangeOptions } from './types';

import { getExchangeMetadata } from './exchangeMetadata';
import { createExchange } from './index';
import { getReadFleetProxyUrl } from './utils/getReadFleetProxy';

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
  const metadata = getExchangeMetadata(account.exchange);
  const shouldForwardApplicationId =
    metadata.requiresApplicationId || account.exchange === 'lighter' || account.exchange === 'asterdex';
  const shouldForwardApiKeyIndex = metadata.requiresApiKeyIndex || account.exchange === 'lighter';

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

  // Determine proxy region - if exchange requires CORS and no region set, use first available
  let proxyRegion = account.proxyRegion;
  if (metadata.requiresCors && !proxyRegion) {
    proxyRegion = Object.keys(metadata?.privateProxyRegions || {})[0] || 'SG';
  }

  const tealstreetProxy = account.tealstreetProxy || metadata.requiresCors || metadata.enforceWsTunnel;

  // When a secondary (read-only) key is configured, route its reads through the
  // cheap read fleet. Undefined until the fleet is wired, so this is a no-op today.
  const secondaryProxyUrl = account.secondary_apiKey ? getReadFleetProxyUrl(account.exchange, proxyRegion) : undefined;

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

    // Proxy settings
    tealstreetProxy: tealstreetProxy,
    tealstreetTunnel:
      tealstreetProxy && (metadata.supportsTunnel !== false || metadata.enforceWsTunnel)
        ? true
        : account.tealstreetTunnel,
    tealstreetWsRoute: account.tealstreetWsRoute,
    ...(proxyRegion ? { proxyRegion } : {}),

    // Testnet
    ...(account.testnet ? { testnet: account.testnet } : {}),

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

    // dYdX
    ...(account.exchange === 'dydx' && account.dydxAddress ? { dydxAddress: account.dydxAddress } : {}),

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

    // Apply any overrides
    ...overrides,
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
  const options = accountToExchangeOptions(account, overrides);
  return createExchange(account.exchange, options);
}
