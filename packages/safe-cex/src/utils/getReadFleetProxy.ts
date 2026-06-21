import type { ExchangeName } from '../types';

// Read-fleet endpoints: cheap commodity VPS (Vultr HAProxy CORS proxies) fronted
// by Cloudflare, one pool per region. Reads from opted-in (secondary-key) accounts
// egress from these many IPs for rate-limit spreading; writes stay on the tuned
// primary/whitelisted transport.
const READ_FLEET_TOKYO = 'https://read-fleet-jp.tealstreet.io/api/exchange_proxy';
const READ_FLEET_SINGAPORE = 'https://read-fleet-sg.tealstreet.io/api/exchange_proxy';

// Resolves the proxy URL for the secondary (read-only) key's transport, split by
// the exchange's region the same way getPrivateProxy splits Tokyo/Singapore.
// Only an explicit 'SG' region routes to Singapore; 'JA' and an unset region both
// route to Tokyo, matching the pre-fleet jp default so opting in stays
// region-consistent. _exchange is kept for future per-exchange routing. Always
// returns a URL today; revert to the exchange-proxy.jp default by returning
// undefined here (call sites already treat undefined as "use the default").
export const getReadFleetProxyUrl = (_exchange: ExchangeName, proxyRegion?: string): string => {
  return proxyRegion === 'SG' ? READ_FLEET_SINGAPORE : READ_FLEET_TOKYO;
};
