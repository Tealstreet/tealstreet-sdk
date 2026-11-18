import { describe, expect, it } from 'vitest';

import {
  exchangesRequireWsIp,
  privateProxyIpsSingapore,
  privateProxyIpsTokyo,
  privateProxyIpsVultrTokyo,
} from './index';
import { legacyExchangesRequireWsIp, legacyWhitelistIps } from './legacyWhitelist';
import { whitelistIpGroups } from './whitelist';

it('uses legacy metadata region/default mappings and existing shared arrays, never routing metadata', () => {
  expect(legacyWhitelistIps('bybit', 'SG')).toBe(privateProxyIpsSingapore);
  expect(legacyWhitelistIps('bybit', 'JA')).toBe(privateProxyIpsTokyo);
  expect(legacyWhitelistIps('bybit', 'UNKNOWN')).toBe(privateProxyIpsSingapore);
  expect(legacyWhitelistIps('cryptocom')).toBe(privateProxyIpsTokyo);
  expect(legacyWhitelistIps('cryptocom', 'SG')).toBe(privateProxyIpsSingapore);
  expect(legacyWhitelistIps('__proto__')).toEqual([]);
});

it('copies only the current fleet even when a legacy region is selected', () => {
  for (const region of ['SG', 'JA']) {
    const result = whitelistIpGroups('bybit', region);
    expect(result.fleet).toEqual(privateProxyIpsVultrTokyo);
    expect(result.legacy).toEqual([]);
    expect(result.extras).toEqual([]);
    expect(result.copy).toEqual(privateProxyIpsVultrTokyo);
    expect(result.legacyOptional).toBe(false);
  }
});

it('a venue with no AWS list copies only the fleet', () => {
  expect(whitelistIpGroups('polymarket').legacy).toEqual([]);
  expect(whitelistIpGroups('polymarket').copy).toEqual(privateProxyIpsVultrTokyo);
});

it('keeps the live Bitunix passthrough extras, but no retired WS proxy extras', () => {
  expect(whitelistIpGroups('bitunix').copy).toEqual([
    ...privateProxyIpsVultrTokyo,
    '72.46.86.143',
    '202.182.126.32',
  ]);
  expect(whitelistIpGroups('bitunix').extras).toEqual(['72.46.86.143', '202.182.126.32']);
  exchangesRequireWsIp.push('bybit');
  try {
    expect(whitelistIpGroups('bybit').copy).toEqual(privateProxyIpsVultrTokyo);
    expect(whitelistIpGroups('bybit').extras).toEqual([]);
  } finally {
    exchangesRequireWsIp.pop();
  }
});

it('MEXC copies the same fleet-only list; retired AWS addresses no longer make caps relevant', () => {
  const groups = whitelistIpGroups('mexc');
  expect(privateProxyIpsVultrTokyo.length).toBeLessThanOrEqual(10);
  expect(groups.limit).toBeUndefined();
  expect(groups.legacyOptional).toBe(false);
  expect(groups.copy).toEqual(privateProxyIpsVultrTokyo);
  expect(groups.legacy).toEqual([]);
});

it('keeps historical legacy WS knowledge out of current whitelist copy', () => {
  expect(legacyExchangesRequireWsIp).toEqual(['coinbase', 'blofin', 'okx', 'okxeu', 'bitget']);
  for (const exchange of legacyExchangesRequireWsIp) {
    expect(whitelistIpGroups(exchange).extras).toEqual([]);
    expect(whitelistIpGroups(exchange).copy).toEqual(privateProxyIpsVultrTokyo);
  }
});
