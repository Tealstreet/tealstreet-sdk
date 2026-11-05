import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { directTransport } from './direct';
import { createEngineTransport, resolveEngineRoutingPlan } from './engine';
import { setDefaultTransport } from './registry';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { PUBLIC_CORS_PROXY } from './tealstreet/transportResolution';

const mocks = vi.hoisted(() => ({ token: vi.fn(async () => 'proxy-jwt') }));
vi.mock('../accessToken', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../accessToken')>()),
  getProxyToken: mocks.token,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => () => mocks.token(),
}));
vi.mock('../utils/runtime', () => ({ isNodeOrBunRuntime: () => false }));

afterEach(() => {
  setDefaultTransport(directTransport);
  mocks.token.mockClear();
});

// Venues that accepted a saved read key (engine.ts secondaryReadHeaders, plus Crypto.com).
// Coinbase drops its read egress at the bridge (rust.secondary-read-keys-bypass.test.ts).
const READ_KEY_VENUES: ExchangeName[] = [
  'binance',
  'binancepm',
  'binancespot',
  'binanceinverse',
  'asterdex',
  'bybit',
  'bybitinverse',
  'bybitspot',
  'bybitdemo',
  'hyrotrader',
  'okx',
  'okxeu',
  'okxdemo',
  'okxeudemo',
  'phemex',
  'backpack',
  'kraken',
  'krakenspot',
  'bitget',
  'bitgetusdc',
  'bitgetinverse',
  'kucoin',
  'kucoinspot',
  'mexc',
  'bitunix',
  'bingx',
  'bitrue',
  'weex',
  'blofin',
  'woo',
  'cryptocom',
];
// Their public reads already had their own route, never the read egress.
const OWN_PUBLIC_ROUTE = new Set<ExchangeName>(['binance', 'binancepm', 'binanceinverse', 'asterdex']);
// Private clients that always install HOW: covered by the native plan, not the raw TS probe below.
const HOW_PRIVATE = new Set<ExchangeName>(['kucoin', 'kucoinspot', 'bingx']);

const primaryOnly = (): ExchangeOptions => ({
  key: 'primary-key',
  secret: 'primary-secret',
  passphrase: 'primary-pass',
  transport: tealstreetTransport,
  tealstreetProxy: true,
  tealstreetTunnel: false,
});
// What createExchangeFromAccount builds for an account with a saved read key.
const withSavedReadKey = (): ExchangeOptions => ({
  ...primaryOnly(),
  secondary_key: 'read-key',
  secondary_secret: 'read-secret',
  secondary_passphrase: 'read-pass',
  secondaryProxyUrl: PUBLIC_CORS_PROXY,
});

type Request = Parameters<Transport['request']>[0];

const probe = () => {
  const dispatch = vi.fn(async () => ({ status: 200, body: new Uint8Array([49]) }));
  const bind = vi.fn((_packet: Request) => ({ ready: vi.fn(() => true), dispatch }));
  const prepareHttp = vi.fn<NonNullable<Transport['prepareHttp']>>(async () => ({ bind, cancel: vi.fn() }));
  const raw: Transport = {
    now: () => 42,
    setTimeout: () => 0,
    clearTimeout: () => {},
    request: vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array([123, 125]) }),
    connect: () => ({ send() {}, close() {} }),
    prepareHttp,
  };
  return { raw, prepareHttp };
};

const preparedUrl = async (
  exchange: ExchangeName,
  options: ExchangeOptions,
  intent: 'public' | 'primary' | 'secondary',
): Promise<string> => {
  const { raw, prepareHttp } = probe();
  const routed = createEngineTransport(exchange, options, raw);
  try {
    const url = `https://${exchange}.invalid/fixture`;
    await routed.prepareHttp!({ request: 1, method: 'GET', url, headers: {}, timeoutMs: 1000, intent });
    return prepareHttp.mock.lastCall![0].url;
  } finally {
    routed.dispose!();
  }
};

describe('secondary read keys bypassed (SECONDARY_READ_KEYS_ENABLED = false)', () => {
  it.each(READ_KEY_VENUES)('%s native plan: no read route; unsigned public reads keep the read egress', (exchange) => {
    const baseURL = `https://${exchange}.invalid`;
    const plan = resolveEngineRoutingPlan(exchange, withSavedReadKey(), baseURL);
    const primary = resolveEngineRoutingPlan(exchange, primaryOnly(), baseURL);
    expect(plan).not.toHaveProperty('readHttp');
    expect(plan).not.toHaveProperty('readHeader');
    expect(plan.http).toEqual(primary.http);
    expect(JSON.stringify(plan)).not.toMatch(/read-key|read-secret|read-pass/);
    if (OWN_PUBLIC_ROUTE.has(exchange)) {
      expect(plan).toEqual(primary);
    } else {
      expect(plan.readPublic).toBe(true);
      expect(plan.readPublicHttp?.prefix).toBe(PUBLIC_CORS_PROXY);
    }
  });

  it.each(READ_KEY_VENUES.filter((exchange) => !HOW_PRIVATE.has(exchange)))(
    '%s: signed reads take the primary route; unsigned public reads keep the read egress',
    async (exchange) => {
      const url = `https://${exchange}.invalid/fixture`;
      expect(await preparedUrl(exchange, withSavedReadKey(), 'primary')).toBe(
        await preparedUrl(exchange, primaryOnly(), 'primary'),
      );
      await expect((async () => preparedUrl(exchange, withSavedReadKey(), 'secondary'))()).rejects.toMatchObject({
        code: 'HTTP_PREPARATION_UNSUPPORTED',
      });
      expect(await preparedUrl(exchange, withSavedReadKey(), 'public')).toBe(
        OWN_PUBLIC_ROUTE.has(exchange)
          ? await preparedUrl(exchange, primaryOnly(), 'public')
          : `${PUBLIC_CORS_PROXY}/${url}`,
      );
    },
  );

  it.each(READ_KEY_VENUES.filter((exchange) => !HOW_PRIVATE.has(exchange)))(
    '%s: a header-inferred signed GET keeps the account route and is never refused',
    async (exchange) => {
      const url = `https://${exchange}.invalid/fixture?signature=abc`;
      const send = async (options: ExchangeOptions) => {
        const { raw } = probe();
        const routed = createEngineTransport(exchange, options, raw);
        await routed.request({ method: 'GET', url, headers: { 'X-MBX-APIKEY': 'primary-key' } });
        routed.dispose!();
        return vi.mocked(raw.request).mock.lastCall![0].url;
      };
      expect(await send(withSavedReadKey())).toBe(await send(primaryOnly()));
    },
  );

  it('a partial saved read key no longer refuses construction', () => {
    expect(() =>
      createEngineTransport('okx', { ...primaryOnly(), secondary_key: 'read-key' }, probe().raw),
    ).not.toThrow();
  });
});
