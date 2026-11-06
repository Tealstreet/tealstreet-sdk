// A public-data-only engine (keyless, or a refused key) keeps the account's tunnel
// flag but does not use the tunnel. Its REST must still take the proxy: venues whose
// public REST refuses browser origins (requiresCors) are dead browser-direct.
import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { describe, expect, it, vi } from 'vitest';

import { exchangeMetadata, getExchangeMetadata } from '../exchangeMetadata';
import { createEngineTransport } from './engine';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { getPublicCorsProxy } from './tealstreet/transportResolution';

vi.mock('../accessToken', () => ({
  getProxyToken: vi.fn(async () => 'proxy-fixture'),
  isTokenExpired: () => false,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => async () => 'proxy-fixture',
  ProxyTokenUnavailableError: class extends Error {},
  refreshProxyToken: vi.fn(async () => undefined),
}));

const CONTRACTS = 'https://open-api.bingx.com/openApi/swap/v2/quote/contracts';
const ok = { status: 200, headers: '', body: new TextEncoder().encode('{"code":0,"data":[]}') };

const rawTransport = () => {
  const dispatched: Array<{ url: string; headers: Record<string, string> }> = [];
  const raw: Transport = {
    now: () => 0,
    setTimeout,
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
    connect: vi.fn(() => ({ send: vi.fn(), close: vi.fn() })),
    request: vi.fn(async (request) => {
      dispatched.push(request);
      return ok;
    }),
  };
  return { raw, dispatched };
};

// As the web factory builds them: requiresCors forces the proxy, the account keeps its tunnel flag.
const publicBingx = (key?: string): ExchangeOptions => ({
  key: key ?? '',
  secret: key ? 'fixture-secret' : '',
  transport: tealstreetTransport,
  publicDataOnly: true,
  tealstreetProxy: true,
  tealstreetTunnel: true,
  proxyRegion: 'SG',
});

describe('public-data-only REST for a venue that needs CORS', () => {
  it('bingx needs CORS', () => {
    expect(getExchangeMetadata('bingx').requiresCors).toBe(true);
  });

  it('a keyless bingx engine reads contracts through the public CORS proxy, off the trading fleet', async () => {
    const { raw, dispatched } = rawTransport();
    const route = createEngineTransport('bingx', publicBingx(), raw);
    await route.request({ method: 'GET', url: CONTRACTS, headers: {} });
    expect(dispatched).toHaveLength(1);
    expect(dispatched[0].url).toBe(`${getPublicCorsProxy()}/${CONTRACTS}`);
    expect(Object.values(dispatched[0].headers)).not.toContain('Bearer proxy-fixture');
    route.dispose!();
  });

  for (const [name, key] of [['refused key', 'refused-key']] as const) {
    it(`a ${name} bingx engine reads contracts through the proxy, never browser-direct`, async () => {
      const { raw, dispatched } = rawTransport();
      const route = createEngineTransport('bingx', publicBingx(key), raw);
      await route.request({ method: 'GET', url: CONTRACTS, headers: {} });
      expect(dispatched).toHaveLength(1);
      expect(dispatched[0].url).not.toBe(CONTRACTS);
      expect(dispatched[0].url).toMatch(/^https:\/\/[^/]+\.tealstreet\.io\/.+\/https:\/\/open-api\.bingx\.com\//);
      expect(dispatched[0].url.endsWith(CONTRACTS)).toBe(true);
      expect(Object.values(dispatched[0].headers)).toContain('Bearer proxy-fixture');
      route.dispose!();
    });
  }

  it('a keyed bingx engine still takes the tunnel', () => {
    const { raw, dispatched } = rawTransport();
    const route = createEngineTransport('bingx', { ...publicBingx('fixture-key'), publicDataOnly: false }, raw);
    expect(route.httpMode).toBe('how');
    expect(dispatched).toHaveLength(0);
    route.dispose!();
  });
});

// Every venue that needs CORS, keyless with the account's tunnel flag: its public REST
// must leave the browser through a Tealstreet host, never straight to the venue.
describe('every requiresCors venue on public data only', () => {
  const venues = (Object.keys(exchangeMetadata) as ExchangeName[]).filter(
    (exchange) => getExchangeMetadata(exchange).requiresCors,
  );
  it.each(venues)('%s leaves through a Tealstreet host', async (exchange) => {
    const { raw, dispatched } = rawTransport();
    const route = createEngineTransport(exchange, { ...publicBingx(), proxyRegion: undefined }, raw);
    // The tunnel egresses from the fleet too; anything else must be a proxied URL.
    if (route.httpMode !== 'how') {
      const url = 'https://venue.invalid/public/markets';
      await route.request({ method: 'GET', url, headers: {} }).catch(() => undefined);
      expect(dispatched).toHaveLength(1);
      expect(dispatched[0].url).not.toBe(url);
    }
    route.dispose!();
  });
});
