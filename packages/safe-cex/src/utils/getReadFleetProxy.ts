import type { ExchangeName } from '../types';

import { restEgressRegion } from '../routing/resolveExchangeRouting';
import {
  isAwsProxyMigrationEnabled,
  LEGACY_AWS_PUBLIC_CORS,
  READ_FLEET_JP,
  READ_FLEET_SG,
} from './transportResolution';

// Resolves the proxy URL for the secondary (read-only) key's transport. Reads from
// opted-in accounts egress the cheap commodity read fleet (rate-limit spreading) while
// writes stay on the tuned primary/whitelisted transport. The fleet egresses from the
// SAME region the PRIMARY REST pool uses for this exchange — `restEgressRegion` reuses
// the per-exchange routing table + the `JA` override (jp/okx/bitget & grvt's 'tiny' pool
// -> Tokyo; every 'asia'-pool exchange -> Singapore). So a secondary-key read lands in
// the IDENTICAL region AWS picks (true parity with getPrivateProxy) — e.g. binance+'SG'
// stays Tokyo (its declared jp pool), bybit with no region goes Singapore (its asia pool).
//
// Gated by ENABLE_AWS_PROXY_MIGRATION: OFF -> legacy AWS (pre-migration baseline; the
// secondary-key UI is also hidden when off so this path is rarely hit), ON -> read fleet.
export const getReadFleetProxyUrl = (exchange: ExchangeName, proxyRegion?: string): string => {
  if (!isAwsProxyMigrationEnabled()) return LEGACY_AWS_PUBLIC_CORS;
  return restEgressRegion(exchange, proxyRegion) === 'sg' ? READ_FLEET_SG : READ_FLEET_JP;
};
