import { setAwsProxyMigrationEnabled } from '../../../packages/safe-cex/src/utils/transportResolution';

import { getProxyTokenFromConfig, getTealstreetApiBaseUrl, isLoggedIn } from './auth';

let syncPromise: Promise<void> | null = null;

/**
 * Resolve ENABLE_AWS_PROXY_MIGRATION for the logged-in CLI user and flip safe-cex's
 * AWS->Vultr CORS/read-fleet gate accordingly. The CLI is single-user per process, so
 * the module-global setter is correct (same as the browser worker).
 *
 * Memoized (fetched once per process) and best-effort: any failure — not logged in, no
 * token, network/HTTP error — leaves the gate OFF = legacy AWS, the safe pre-migration
 * baseline. MUST be awaited before the first createExchange so the host is set before
 * virtualClock / REST start.
 *
 * The bucketing is resolved SERVER-SIDE (by the proxy token's userId) — the CLI just
 * receives the booleans, identical to web. /api/feature-flags accepts the CLI proxy
 * token via verifyProxyToken (the same path /api/cli/checkin uses).
 */
export function ensureAwsProxyMigrationFlag(): Promise<void> {
  if (syncPromise) return syncPromise;
  syncPromise = (async () => {
    try {
      if (!isLoggedIn()) return;
      const token = getProxyTokenFromConfig();
      if (!token) return;
      // Awaited before exchange creation, so cap it — a stalled request (captive portal /
      // VPN / socket hang) must not block CLI startup. Timeout -> catch -> OFF (legacy AWS).
      const res = await fetch(`${getTealstreetApiBaseUrl()}/api/feature-flags`, {
        headers: { authorization: `Bearer ${token}` },
        signal: AbortSignal.timeout(3000),
      });
      if (!res.ok) return;
      const flags = (await res.json()) as Record<string, unknown> | null;
      setAwsProxyMigrationEnabled(Boolean(flags?.ENABLE_AWS_PROXY_MIGRATION));
    } catch {
      // best-effort; leave OFF (legacy AWS)
    }
  })();
  return syncPromise;
}

/** Test hook. Do not call from product code. */
export function __resetAwsProxyMigrationSyncForTest(): void {
  syncPromise = null;
}
