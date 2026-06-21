import type { ExchangeMetadata } from '../exchangeMetadata';
import type { ExchangeAccountConfig } from '../types';

// Single source of truth for the proxy / tunnel / CORS transport resolution that
// was previously scattered across createExchangeFromAccount.ts (preference →
// option resolution), safeCexAxiosCreate.ts (getProxyForAxios) and
// getPrivateProxy.ts. Hoisted so routing is traceable in one place: the
// AWS→Vultr CORS migration and the secondary-key "is this Direct?" gate both
// build on it. Behavior is locked by transport-resolution.characterization.test.ts.

// --- Proxy endpoints -------------------------------------------------------

// AWS whitelisted private proxy pool. These IPs are whitelisted by users on the
// exchange side (embedded in exchange partnerships), so this traffic MUST stay
// on AWS — it cannot move to Vultr without breaking the allowlists.
export const PRIVATE_PROXY_GRVT_TINY = 'https://fsjnpy143pbz-tiny.jp.tealstreet.io/api/exchange_proxy';
export const PRIVATE_PROXY_JP = 'https://fsjnpy143pbz.jp.tealstreet.io/api/exchange_proxy';
export const PRIVATE_PROXY_ASIA = 'https://fsjnpy143pbz.asia.tealstreet.io/api/exchange_proxy';

// Vultr read-fleet: cheap commodity VPS HAProxy CORS proxies behind Cloudflare,
// region-split (jp/sg), unwhitelisted/tokenless. The single source of truth for the
// read-fleet hosts — getReadFleetProxy.ts (secondary-key reads) imports these.
export const READ_FLEET_JP = 'https://read-fleet-jp.tealstreet.io/api/exchange_proxy';
export const READ_FLEET_SG = 'https://read-fleet-sg.tealstreet.io/api/exchange_proxy';

// Legacy AWS public CORS proxy — the pre-migration host the read fleet replaced.
// This is the OFF / default value of the ENABLE_AWS_PROXY_MIGRATION rollout.
export const LEGACY_AWS_PUBLIC_CORS = 'https://exchange-proxy.jp.tealstreet.io/api/exchange_proxy';

// ENABLE_AWS_PROXY_MIGRATION rollout gate. The unwhitelisted/public CORS proxy and
// the secondary-key read host are the only transport that moved AWS -> Vultr; this
// flag gates that whole switch (+ the secondary-key UI, gated app-side). Default OFF
// = legacy AWS, the zero-impact pre-migration baseline. safe-cex NEVER reads feature
// flags — the host app (web worker) flips this once at startup from the per-user
// flag; non-web runtimes (CLI / exchange-server) leave it OFF and stay on AWS.
let awsProxyMigrationEnabled = false;
export const setAwsProxyMigrationEnabled = (enabled: boolean): void => {
  awsProxyMigrationEnabled = enabled;
};
export const isAwsProxyMigrationEnabled = (): boolean => awsProxyMigrationEnabled;

// Public CORS proxy (CORS-unblocks public/unwhitelisted browser requests). Resolved
// at CALL time so the startup flip is always reflected — never cache it into a
// module-eval string. ON -> read-fleet-jp; OFF -> legacy AWS exchange-proxy.jp.
export const getPublicCorsProxy = (): string => (awsProxyMigrationEnabled ? READ_FLEET_JP : LEGACY_AWS_PUBLIC_CORS);

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
