import { exchangesRequireWsIp, privateProxyIpsVultrTokyo, wsProxyIps } from './index';
import { legacyExchangesRequireWsIp, legacyWhitelistIps } from './legacyWhitelist';

export const LEGACY_WHITELIST_NOTE = 'Also allow these for the legacy app (rollback and whitelabel).';
export const OPTIONAL_LEGACY_WHITELIST_NOTE = 'Optional for the legacy app (rollback and whitelabel).';
export type WhitelistIpGroups = {
  fleet: readonly string[];
  legacy: readonly string[];
  extras: readonly string[];
  copy: readonly string[];
  legacyOptional: boolean;
  limit?: number;
};

// https://www.okx.com/docs-v5/en/#overview-api-key-creation-api-key-security
// https://docs.blofin.com/index.html#api-key-creation
// https://www.mexc.com/api-docs/spot-v3/error-code (730703: 10 bound IPs per key)
const verifiedLimits: Record<string, number> = { okx: 20, okxdemo: 20, okxeu: 20, okxeudemo: 20, blofin: 20, mexc: 10 };
const unique = (ips: readonly string[]) => [...new Set(ips)];

/** Display/copy policy only: never changes a venue's routing metadata. */
export function whitelistIpGroups(exchange = '', region?: string): WhitelistIpGroups {
  const fleet = unique(privateProxyIpsVultrTokyo);
  const legacy = unique(legacyWhitelistIps(exchange, region)).filter((ip) => !fleet.includes(ip));
  const extras = unique([
    ...(exchangesRequireWsIp.includes(exchange) || legacyExchangesRequireWsIp.includes(exchange) ? wsProxyIps : []),
    // Bitunix passthrough retirement is unproved (infra/tofu/athena/athena.tf).
    ...(exchange === 'bitunix' ? ['72.46.86.143', '202.182.126.32'] : []),
  ]).filter((ip) => !fleet.includes(ip) && !legacy.includes(ip));
  const union = unique([...fleet, ...legacy, ...extras]);
  const limit = Object.hasOwn(verifiedLimits, exchange) ? verifiedLimits[exchange] : undefined;
  const legacyOptional = limit !== undefined && union.length > limit;
  return { fleet, legacy, extras, copy: legacyOptional ? fleet : union, legacyOptional, limit };
}
