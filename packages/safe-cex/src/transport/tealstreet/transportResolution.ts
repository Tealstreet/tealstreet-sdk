import type { ExchangeMetadata } from '../../exchangeMetadata';
import type { ExchangeAccountConfig } from '../../types';

// Single source of truth for the proxy / tunnel / CORS transport resolution that
// was previously scattered across createExchangeFromAccount.ts (preference →
// option resolution), safeCexAxiosCreate.ts (getProxyForAxios) and
// getPrivateProxy.ts. Hoisted so routing is traceable in one place: the
// secondary-key "is this Direct?" gate builds on it. Behavior is locked by
// transport-resolution.characterization.test.ts.

// --- Proxy endpoints -------------------------------------------------------

// AWS whitelisted private proxy pool. These IPs are whitelisted by users on the
// exchange side (embedded in exchange partnerships), so this traffic MUST stay
// on AWS — it cannot move to another provider without breaking the allowlists.
export const PRIVATE_PROXY_GRVT_TINY = 'https://fsjnpy143pbz-tiny.jp.tealstreet.io/api/exchange_proxy';
export const PRIVATE_PROXY_JP = 'https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy';
export const PRIVATE_PROXY_ASIA = 'https://fsjnpy143pbz.asia.tealstreet.io/api/exchange_proxy';

// AWS public CORS proxy. The ONLY host for unwhitelisted public browser reads and for
// a secondary read-only key's reads. A Vultr read fleet was built as a cheaper
// alternative in June 2026 and abandoned 2026-08-20 having never carried traffic.
export const AWS_PUBLIC_CORS_PROXY = 'https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy';

// Public CORS proxy (CORS-unblocks public/unwhitelisted browser requests). A getter,
// not an inlined const, because every call site resolves at CALL time — that is the
// seam any future host rollout would need, and callers must not cache the string.
export const getPublicCorsProxy = (): string => AWS_PUBLIC_CORS_PROXY;

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
