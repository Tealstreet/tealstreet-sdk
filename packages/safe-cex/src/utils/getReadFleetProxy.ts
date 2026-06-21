import type { ExchangeName } from '../types';

import {
  isAwsProxyMigrationEnabled,
  LEGACY_AWS_PUBLIC_CORS,
  READ_FLEET_JP,
  READ_FLEET_SG,
} from './transportResolution';

// Resolves the proxy URL for the secondary (read-only) key's transport. Reads from
// opted-in accounts egress from the read fleet's many IPs (rate-limit spreading);
// writes stay on the tuned primary/whitelisted transport. Split by the exchange's
// region the same way getPrivateProxy splits Tokyo/Singapore: only an explicit 'SG'
// routes to Singapore; 'JA' and an unset region both route to Tokyo (the pre-fleet
// jp default). _exchange is kept for future per-exchange routing. The host constants
// live in transportResolution.ts (single source of truth).
//
// Gated by ENABLE_AWS_PROXY_MIGRATION: OFF -> legacy AWS (pre-migration baseline; the
// secondary-key UI is also hidden when off so this path is rarely hit), ON -> read fleet.
export const getReadFleetProxyUrl = (_exchange: ExchangeName, proxyRegion?: string): string => {
  if (!isAwsProxyMigrationEnabled()) return LEGACY_AWS_PUBLIC_CORS;
  return proxyRegion === 'SG' ? READ_FLEET_SG : READ_FLEET_JP;
};
