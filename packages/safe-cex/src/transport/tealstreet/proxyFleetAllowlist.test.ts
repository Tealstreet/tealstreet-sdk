import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { PROXY_FLEET_REST_PREFIXES, proxyFleetForwards } from './proxyFleetAllowlist';

const FLEET_CONFIG = resolve(__dirname, '../../../../../terraform/private-exchange-proxy/haproxy-sg-running.cfg');

describe('proxy fleet allowlist', () => {
  it('equals the https prefixes the fleet HAProxy forwards', () => {
    const config = readFileSync(FLEET_CONFIG, 'utf8');
    const prefixes = [...config.matchAll(/path_beg -i \/api\/exchange_proxy\/(https:\/\/\S+)/g)].map(
      ([, prefix]) => prefix,
    );
    expect([...PROXY_FLEET_REST_PREFIXES].sort()).toEqual([...new Set(prefixes)].sort());
  });

  it('matches a venue url by prefix only', () => {
    expect(proxyFleetForwards('https://api.bybit.com/v5/order/create')).toBe(true);
    expect(proxyFleetForwards('https://api.kraken.com/0/private/Balance')).toBe(false);
    expect(proxyFleetForwards('https://api.crypto.com/v2/private/get-order')).toBe(false);
  });
});
