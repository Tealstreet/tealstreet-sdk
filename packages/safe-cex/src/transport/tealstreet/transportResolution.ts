import type { ExchangeMetadata } from '../../exchangeMetadata';
import type { ExchangeAccountConfig } from '../../types';
import type { RestPool } from './routing/types';

// Single source of truth for the proxy / tunnel / CORS transport resolution that
// was previously scattered across createExchangeFromAccount.ts (preference →
// option resolution), safeCexAxiosCreate.ts (getProxyForAxios) and
// getPrivateProxy.ts. Hoisted so routing is traceable in one place: the
// secondary-key "is this Direct?" gate builds on it. Behavior is locked by
// transport-resolution.characterization.test.ts.

// --- Proxy endpoints -------------------------------------------------------

// Whitelisted private proxy pools: the exchange-proxy-edge Worker on the Vultr
// tunnel host, which checks the user's proxy JWT and forwards to the 9-IP Tokyo
// fleet (docs/plans/AWS_EXIT.md step 3e). The whitelist is Tokyo-only, so the jp,
// asia and GRVT pools are one host; the names stay because the routing table and
// the exchange server's pinned hosts still tell them apart.
export const PRIVATE_PROXY_JP = 'https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy';
export const PRIVATE_PROXY_ASIA = PRIVATE_PROXY_JP;
export const PRIVATE_PROXY_GRVT_TINY = PRIVATE_PROXY_JP;

// Public CORS proxy. The ONLY host for unwhitelisted public browser reads and for a
// secondary read-only key's reads: the same edge Worker's /api/public_proxy/, which
// needs no JWT and always egresses from the tunnel hosts' own IPs, never the
// whitelisted fleet, so public-data weight stays off the IPs users trade through
// (AWS_EXIT.md step 3d; it replaced AWS's exchange-proxy.jp). Same `<base>/<https url>` shape.
// ROLLOUT GUARD: do not release the rust lanes until infra/tofu proxy-fleet's
// `public_cors_enabled` is on and BOTH tunnel hosts carry the public CORS container;
// before that this host 403s every public read (Binance market data, clocks, ticks).
export const PUBLIC_CORS_PROXY = 'https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy';

export interface ProxyHosts {
  privateProxy: Record<RestPool, string>;
  publicCorsProxy: string;
}

let proxyHosts: ProxyHosts = {
  privateProxy: { jp: PRIVATE_PROXY_JP, asia: PRIVATE_PROXY_ASIA, tiny: PRIVATE_PROXY_GRVT_TINY },
  publicCorsProxy: PUBLIC_CORS_PROXY,
};

/** Pins this runtime's proxy hosts (the hosted exchange server); see `pinTealstreetEgressHosts`. */
export const setProxyHosts = (hosts: ProxyHosts): void => {
  proxyHosts = { privateProxy: { ...hosts.privateProxy }, publicCorsProxy: hosts.publicCorsProxy };
};

export const getPrivateProxyHost = (pool: RestPool): string => proxyHosts.privateProxy[pool];

// Public CORS proxy (CORS-unblocks public/unwhitelisted browser requests). A getter,
// not an inlined const, because every call site resolves at CALL time — that is the
// seam any future host rollout would need, and callers must not cache the string.
export const getPublicCorsProxy = (): string => proxyHosts.publicCorsProxy;

// --- Preference resolution -------------------------------------------------

export interface ResolvedTransport {
  // Mirrors ExchangeOptions: the `||` of optional flags can be undefined (treated as false everywhere downstream).
  tealstreetProxy: boolean | undefined;
  tealstreetTunnel: boolean | undefined;
  proxyRegion?: string;
}

type TransportAccount = Pick<ExchangeAccountConfig, 'tealstreetProxy' | 'tealstreetTunnel' | 'proxyRegion'>;
type TransportMetadata = Pick<
  ExchangeMetadata,
  'requiresCors' | 'enforceWsTunnel' | 'supportsTunnel' | 'privateProxyRegions'
>;

// The default region for an account that needs a region but hasn't picked one:
// the exchange's first declared private-proxy region, else Singapore. Shared by
// resolveAccountTransport and normalizeExchangeAccountConfig (whose GUARDS differ
// — requiresCors vs tealstreetProxy — but whose default VALUE is identical).
export const defaultProxyRegion = (metadata: Pick<ExchangeMetadata, 'privateProxyRegions'>): string =>
  Object.keys(metadata?.privateProxyRegions || {})[0] || 'SG';

// Resolve the effective proxy / tunnel / region for a saved account from its
// transport preferences plus exchange metadata. Extracted verbatim from
// accountToExchangeOptions — `requiresCors`/`enforceWsTunnel` force the proxy on,
// and a forced (or supported) tunnel rides on top.
export const resolveAccountTransport = (account: TransportAccount, metadata: TransportMetadata): ResolvedTransport => {
  let proxyRegion = account.proxyRegion;
  if (metadata.requiresCors && !proxyRegion) {
    proxyRegion = defaultProxyRegion(metadata);
  }

  const tealstreetProxy = account.tealstreetProxy || metadata.requiresCors || metadata.enforceWsTunnel;
  const tealstreetTunnel =
    tealstreetProxy && (metadata.supportsTunnel !== false || metadata.enforceWsTunnel)
      ? true
      : account.tealstreetTunnel;

  return { tealstreetProxy, tealstreetTunnel, ...(proxyRegion ? { proxyRegion } : {}) };
};

// The user's order leaves their own machine — no Tealstreet proxy and no tunnel.
// A secondary read-only key provides no value in this state (the primary key is
// already unwhitelisted), so the UI must not offer it.
export const isDirectTransport = (
  transport: Pick<ResolvedTransport, 'tealstreetProxy' | 'tealstreetTunnel'>,
): boolean => !transport.tealstreetProxy && !transport.tealstreetTunnel;
