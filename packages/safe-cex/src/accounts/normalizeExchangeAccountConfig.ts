import type { ExchangeAccountConfig } from '../types';

import { getExchangeMetadata } from '../exchangeMetadata';

export function normalizeExchangeAccountConfig(account: ExchangeAccountConfig): ExchangeAccountConfig {
  const metadata = getExchangeMetadata(account.exchange);
  const tealstreetProxy = metadata.requiresCors ? true : account.testnet ? false : Boolean(account.tealstreetProxy);
  const { proxyRegion: accountProxyRegion, ...accountWithoutProxyRegion } = account;

  let proxyRegion = accountProxyRegion;
  if (tealstreetProxy && !proxyRegion) {
    proxyRegion = Object.keys(metadata.privateProxyRegions || {})[0] || 'SG';
  }

  return {
    ...accountWithoutProxyRegion,
    tealstreetProxy,
    ...(tealstreetProxy && proxyRegion ? { proxyRegion } : {}),
  };
}
