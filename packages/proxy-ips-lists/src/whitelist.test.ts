import { describe, expect, it } from 'vitest';

import {
  exchangesRequireWsIp,
  privateProxyIpsSingapore,
  privateProxyIpsTokyo,
  privateProxyIpsVultrTokyo,
  wsProxyIps,
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

it('puts the fleet first then copies the selected legacy SG or JA addresses once', () => {
  for (const region of ['SG', 'JA']) {
    const result = whitelistIpGroups('bybit', region);
    const legacy = region === 'SG' ? privateProxyIpsSingapore : privateProxyIpsTokyo;
    expect(result.fleet).toEqual(privateProxyIpsVultrTokyo);
    expect(result.legacy).toEqual(legacy);
    expect(result.copy).toEqual([...privateProxyIpsVultrTokyo, ...legacy]);
    expect(new Set(result.copy).size).toBe(result.copy.length);
  }
});

it('a venue with no AWS list copies only the fleet', () => {
  expect(whitelistIpGroups('polymarket').legacy).toEqual([]);
  expect(whitelistIpGroups('polymarket').copy).toEqual(privateProxyIpsVultrTokyo);
});

it('retains Bitunix and declared WS extras in the union, without duplicating IPs', () => {
  expect(whitelistIpGroups('bitunix').copy).toEqual([
    ...privateProxyIpsVultrTokyo,
    ...privateProxyIpsTokyo,
    '72.46.86.143',
    '202.182.126.32',
  ]);
  exchangesRequireWsIp.push('bybit');
  try {
    expect(whitelistIpGroups('bybit').copy).toEqual([
      ...privateProxyIpsVultrTokyo,
      ...privateProxyIpsSingapore,
      ...wsProxyIps,
    ]);
  } finally {
    exchangesRequireWsIp.pop();
  }
});

describe('the verified BloFin 20-IP cap', () => {
  it('copies the union at the boundary, fleet only above it, but keeps optional legacy addresses visible', () => {
    const original = privateProxyIpsSingapore.length;
    privateProxyIpsSingapore.push('192.0.2.1', '192.0.2.2', '192.0.2.3');
    try {
      expect(whitelistIpGroups('blofin').copy).toHaveLength(20);
      expect(whitelistIpGroups('blofin').legacyOptional).toBe(false);
      privateProxyIpsSingapore.push('192.0.2.5');
      const over = whitelistIpGroups('blofin');
      expect(over.copy).toEqual(privateProxyIpsVultrTokyo);
      expect(over.legacy).toHaveLength(11);
      expect(over.legacyOptional).toBe(true);
      expect(over.limit).toBe(20);
    } finally {
      privateProxyIpsSingapore.splice(original);
    }
  });
});

it('MEXC has a documented 10-IP cap: its 14-IP union stays visible but copy is fleet-only', () => {
  const groups = whitelistIpGroups('mexc');
  expect(groups.limit).toBe(10);
  expect(groups.legacyOptional).toBe(true);
  expect(groups.copy).toEqual(privateProxyIpsVultrTokyo);
  expect(groups.legacy).toEqual(privateProxyIpsTokyo);
});

it('retains legacy WS egress even while the current Rust WS list is empty', () => {
  expect(legacyExchangesRequireWsIp).toEqual(['coinbase', 'blofin', 'okx', 'okxeu', 'bitget']);
  for (const exchange of legacyExchangesRequireWsIp) {
    expect(whitelistIpGroups(exchange).extras).toEqual(wsProxyIps);
    expect(whitelistIpGroups(exchange).copy).toEqual(expect.arrayContaining(wsProxyIps));
  }
});
