import type { ExchangeAccountConfig } from '../types';

import { getExchangeMetadata } from '../exchangeMetadata';
import { defaultProxyRegion } from '../transport/tealstreet/transportResolution';

export function normalizeExchangeAccountConfig(account: ExchangeAccountConfig): ExchangeAccountConfig {
  const metadata = getExchangeMetadata(account.exchange);
  // NOTE: this is persist-time shaping of a saved account and intentionally does
  // NOT match resolveAccountTransport's resolve-time formula. Here we never persist
  // proxy for a testnet account and do not fold in enforceWsTunnel (applied fresh at
  // resolve time). Only the region DEFAULT is shared, via defaultProxyRegion.
  const tealstreetProxy = metadata.requiresCors ? true : account.testnet ? false : Boolean(account.tealstreetProxy);
  const { proxyRegion: accountProxyRegion, ...accountWithoutProxyRegion } = account;

  let proxyRegion = accountProxyRegion;
  if (tealstreetProxy && !proxyRegion) {
    proxyRegion = defaultProxyRegion(metadata);
  }

  return {
    ...accountWithoutProxyRegion,
    tealstreetProxy,
    ...(tealstreetProxy && proxyRegion ? { proxyRegion } : {}),
  };
}
