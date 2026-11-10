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
const bitunixPassthroughIps = ['72.46.86.143', '202.182.126.32'] as const;

/** Display/copy policy only: never changes a venue's routing metadata. */
export function whitelistIpGroups(exchange = '', _region?: string): WhitelistIpGroups {
  const fleet = unique(privateProxyIpsVultrTokyo);
  // Not legacy AWS: Bitunix passthrough retirement is still unproved
  // (infra/tofu/athena/athena.tf), so restricted keys must allow these too.
  const extras = exchange === 'bitunix' ? bitunixPassthroughIps : [];
  const copy = unique([...fleet, ...extras]);
  return { fleet, legacy: [], extras, copy, legacyOptional: false };
}
