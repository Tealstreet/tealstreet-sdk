import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { PROXY_FLEET_REST_PREFIXES, proxyFleetForwards } from './proxyFleetAllowlist';

const FLEET_CONFIG = resolve(__dirname, '../../../../../terraform/private-exchange-proxy/haproxy-sg-running.cfg');

describe('proxy fleet allowlist', () => {
  // The fleet config lives in tealstreet-next; the SDK mirror carries only this list.
  it.skipIf(!existsSync(FLEET_CONFIG))('equals the https prefixes the fleet HAProxy forwards', () => {
    const config = readFileSync(FLEET_CONFIG, 'utf8');
    // Commented ACLs (notably testnet.okex.com) are not forwarding routes.
    const routes = [...config.matchAll(/^\s*acl (\S+) path_beg -i \/api\/exchange_proxy\/(https:\/\/\S+)/gm)];
    const backends = new Set([...config.matchAll(/^backend (\S+)[ \t]*$/gm)].map(([, backend]) => backend));
    const uses = new Map(
      [...config.matchAll(/^\s*use_backend (\S+) if (\S+)[ \t]*$/gm)].map(([, backend, acl]) => [acl, backend]),
    );
    // GRVT also declares helper ACLs for header rewrites; only use_backend routes forward.
    const forwarded = routes.filter(([, acl]) => uses.has(acl));
    const prefixes = forwarded.map(([, , prefix]) => prefix);
    expect([...PROXY_FLEET_REST_PREFIXES].sort()).toEqual([...new Set(prefixes)].sort());
    for (const [, acl, prefix] of forwarded) {
      expect(backends.has(uses.get(acl) ?? ''), prefix).toBe(true);
    }
  });

  it.each([
    'https://testnet.binance.vision/api/v3/account',
    'https://testnet.binancefuture.com/eapi/v1/account',
    'https://fapi.asterdex-testnet.com/fapi/v3/account',
    'https://api.staging.woox.io/v3/accountinfo',
    'https://testnet-api-evm.orderly.org/v1/client/holding',
    'https://api.rh-testnet.lighter.xyz/api/v1/account',
    // WEEX still never installs HOW; its explicit HTTP proxy needs a route too.
    'https://api-contract.weex.com/capi/v2/account/assets',
    'https://api.kraken.com/0/private/Balance',
    'https://openapi.okx.com/api/v5/account/balance',
    'https://eapi.binance.com/eapi/v1/account',
    'https://demo-fapi.binance.com/fapi/v3/account',
    'https://demo-dapi.binance.com/dapi/v1/account',
    'https://demo-trading-openapi.blofin.com/api/v1/account/balance',
  ])('forwards the engine REST endpoint %s', (url) => {
    expect(proxyFleetForwards(url)).toBe(true);
    const { origin } = new URL(url);
    expect(proxyFleetForwards(`${origin}.evil.invalid/`)).toBe(false);
    expect(proxyFleetForwards(`${origin}@evil.invalid/`)).toBe(false);
  });

  // DNS-verified retired routes must not advertise an unusable fallback.
  it.each([
    'api.aster.fi',
    'testnet-api.aster.fi',
    'stream.aster.fi',
    'testnet-stream.aster.fi',
    'ws.prod.paradex.trade',
    'ws.testnet.paradex.trade',
    'wsaws.okx.com',
    'direct.bitmex.com',
    'ws.bitmex.com',
    'api.direct.bitmex.com',
    'www.bitmex.com',
    'testnet.bitmex.com',
    'api.coincatch.com',
    'api-swap-rest.bingx.com',
    'api-pub.woo.network',
    'testnet.okex.com',
  ])('refuses the retired or disabled host %s', (host) => {
    expect(proxyFleetForwards(`https://${host}/account`)).toBe(false);
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
