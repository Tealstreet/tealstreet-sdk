import { privateProxyIpsVultrTokyo } from './index';

export type WhitelistIpGroups = {
  fleet: readonly string[];
  legacy: readonly string[];
  extras: readonly string[];
  copy: readonly string[];
  legacyOptional: boolean;
  limit?: number;
};

const unique = (ips: readonly string[]) => [...new Set(ips)];

/** Display/copy policy only: never changes a venue's routing metadata. */
export function whitelistIpGroups(_exchange = '', _region?: string): WhitelistIpGroups {
  const fleet = unique(privateProxyIpsVultrTokyo);
  return { fleet, legacy: [], extras: [], copy: fleet, legacyOptional: false };
}
