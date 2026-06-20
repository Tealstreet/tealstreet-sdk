import type { ExchangeName } from '../types';

// Resolves the proxy URL for the secondary (read-only) key's transport. Reads
// route through a cheap, non-whitelisted commodity fleet, split by the exchange's
// region (Tokyo vs Singapore) the same way getPrivateProxy splits .jp/.asia.
//
// Returns undefined until the fleet endpoints are provisioned and wired (a later
// epic). While undefined, the secondary client keeps its existing transport, so
// enabling a secondary key changes nothing about routing — this stays additive
// and default-off.
export const getReadFleetProxyUrl = (_exchange: ExchangeName, _proxyRegion?: string): string | undefined => {
  return undefined;
};
