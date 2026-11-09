import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { PUBLIC_CORS_HOSTS, PUBLIC_PROXY_READ_HOSTS, readsViaPublicProxy } from './publicCorsAllowlist';

const PUBLIC_CORS_CONFIG = resolve(__dirname, '../../../../../terraform/exchange-proxy-haproxy-tokyo.cfg');
const VENUES = resolve(__dirname, '../../../../../sdk-rs/venues');
const CRATE: Record<string, string> = {
  bingxspot: 'bingx',
  bitgetinverse: 'bitget',
  bitgetusdc: 'bitget',
  bybitinverse: 'bybit',
  bybitspot: 'bybit',
  coinbasespot: 'coinbase',
};

const crateSource = (crate: string): string => {
  const dir = resolve(VENUES, crate, 'src');
  return readdirSync(dir, { recursive: true, encoding: 'utf8' })
    .filter((file) => file.endsWith('.rs'))
    .map((file) => readFileSync(resolve(dir, file), 'utf8'))
    .join('\n');
};

describe('public CORS proxy allowlist', () => {
  // The proxy config lives in tealstreet-next; the SDK mirror carries only this list.
  it.skipIf(!existsSync(PUBLIC_CORS_CONFIG))('equals the hosts public-cors.tf derives for PUBLIC_HOSTS', () => {
    const config = readFileSync(PUBLIC_CORS_CONFIG, 'utf8');
    const hosts = [
      ...config.matchAll(/^\s*acl\s+\S+\s+path_beg\s+-i\s+\/api\/exchange_proxy\/https:\/\/([^/\s]+)\//gm),
    ].map(([, host]) => host!.toLowerCase());
    expect([...PUBLIC_CORS_HOSTS].sort()).toEqual([...new Set(hosts)].sort());
  });

  it.each(Object.entries(PUBLIC_PROXY_READ_HOSTS))('%s sends only allowlisted hosts', (_venue, hosts) => {
    for (const host of hosts!) expect(PUBLIC_CORS_HOSTS, host).toContain(host);
  });

  it.skipIf(!existsSync(VENUES)).each(Object.entries(PUBLIC_PROXY_READ_HOSTS))(
    "%s's hosts are ones its venue crate reads",
    (venue, hosts) => {
      const source = crateSource(CRATE[venue] ?? venue);
      for (const host of hosts!) expect(source, host).toContain(`https://${host}`);
    },
  );

  it('leaves venues the public proxy refuses on the exchange proxy', () => {
    expect(readsViaPublicProxy('cryptocom', 'https://api.crypto.com/exchange/v1/public/get-candlestick')).toBe(false);
    expect(readsViaPublicProxy('blofin', 'https://demo-trading-openapi.blofin.com/api/v1/market/candles')).toBe(false);
    expect(readsViaPublicProxy('okx', 'https://www.okx.com/api/v5/market/candles')).toBe(false);
    expect(readsViaPublicProxy('bybit', 'https://api-demo.bybit.com/v5/market/kline')).toBe(false);
    expect(readsViaPublicProxy('bybitdemo', 'https://api-demo.bybit.com/v5/market/kline')).toBe(false);
    expect(readsViaPublicProxy('blofin', 'https://openapi.blofin.com/api/v1/market/candles')).toBe(true);
    expect(readsViaPublicProxy('bingx', 'https://open-api.bingx.com.evil.invalid/openApi/swap/v3/quote/klines')).toBe(
      false,
    );
    expect(readsViaPublicProxy('bingx', 'not a url')).toBe(false);
  });
});
