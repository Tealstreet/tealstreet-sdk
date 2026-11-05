import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { PROXY_FLEET_REST_PREFIXES, proxyFleetForwards } from './proxyFleetAllowlist';

const FLEET_CONFIG = resolve(__dirname, '../../../../../terraform/private-exchange-proxy/haproxy-sg-running.cfg');

describe('proxy fleet allowlist', () => {
  // The fleet config lives in tealstreet-next; the SDK mirror carries only this list.
  it.skipIf(!existsSync(FLEET_CONFIG))('equals the https prefixes the fleet HAProxy forwards', () => {
    const config = readFileSync(FLEET_CONFIG, 'utf8');
    const prefixes = [...config.matchAll(/path_beg -i \/api\/exchange_proxy\/(https:\/\/\S+)/g)].map(
      ([, prefix]) => prefix,
    );
    expect([...PROXY_FLEET_REST_PREFIXES].sort()).toEqual([...new Set(prefixes)].sort());
  });

  it.each([
    'https://clob.polymarket.com/data/orders?next_cursor=MA==',
    'https://gamma-api.polymarket.com/markets?active=true',
    'https://data-api.polymarket.com/positions?user=fixture',
  ])('forwards the Polymarket REST endpoint %s', (url) => {
    expect(proxyFleetForwards(url)).toBe(true);
    expect(proxyFleetForwards(url.replace('.polymarket.com/', '.polymarket.com.evil.invalid/'))).toBe(false);
    expect(proxyFleetForwards(url.replace('.polymarket.com/', '.polymarket.com@evil.invalid/'))).toBe(false);
  });

  it('matches a venue url by prefix only', () => {
    expect(proxyFleetForwards('https://api.bybit.com/v5/order/create')).toBe(true);
    expect(proxyFleetForwards('https://api.kraken.com/0/private/Balance')).toBe(true);
    expect(proxyFleetForwards('https://api.gemini.com/v1/balances')).toBe(false);
    expect(proxyFleetForwards('https://api.crypto.com/v2/private/get-order')).toBe(false);
  });
});
