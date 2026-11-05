import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { directTransport } from './direct';
import { createEngineTransport, resolveEngineRoutingPlan } from './engine';
import { SignedPublicProxyRefusal } from './publicProxyGuard';
import { setDefaultTransport } from './registry';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';
import { getPublicCorsProxy } from './tealstreet/transportResolution';

const mocks = vi.hoisted(() => ({ token: vi.fn(async () => 'proxy-jwt') }));
vi.mock('../accessToken', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../accessToken')>()),
  getProxyToken: mocks.token,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: () => () => mocks.token(),
}));
vi.mock('../utils/runtime', () => ({ isNodeOrBunRuntime: () => false }));

const makeRaw = () => ({
  webSocketHeaders: true,
  now: vi.fn(() => 42),
  request: vi.fn<Transport['request']>().mockResolvedValue({ status: 200, body: new Uint8Array([123, 125]) }),
  connect: vi.fn<Transport['connect']>(() => ({ send: vi.fn(), close: vi.fn() })),
  setTimeout: vi.fn<Transport['setTimeout']>(),
  clearTimeout: vi.fn<Transport['clearTimeout']>(),
});

const options = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'venue-key',
  secret: 'venue-secret',
  transport: tealstreetTransport,
  tealstreetProxy: true,
  ...extra,
});

const DUAL = 'https://fapi.binance.com/fapi/v1/positionSide/dual?timestamp=1&signature=abc';

afterEach(() => {
  setDefaultTransport(directTransport);
  mocks.token.mockClear();
});

describe('a signed request is never sent toward the public proxy', () => {
  it('refuses a secondary-key signed read before any I/O (secondary keys fail closed)', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport(
      'binance',
      options({ secondary_key: 'read-key', secondary_secret: 'read-secret' }),
      raw,
    );
    await expect(
      routed.request({ method: 'GET', url: DUAL, headers: { 'X-MBX-APIKEY': 'read-key' } }),
    ).rejects.toBeInstanceOf(SignedPublicProxyRefusal);
    expect(raw.request).not.toHaveBeenCalled();
  });

  it('refuses a credential header or a signature on the explicit public route', async () => {
    for (const request of [
      {
        url: 'https://fapi.binance.com/fapi/v2/account',
        headers: { 'X-MBX-APIKEY': 'venue-key' } as Record<string, string>,
      },
      { url: 'https://fapi.binance.com/fapi/v2/account', headers: { Authorization: 'Bearer x' } },
      { url: DUAL, headers: {} },
    ]) {
      const raw = makeRaw();
      const routed = createEngineTransport('binance', options(), raw);
      await expect(routed.request({ route: 'publicCors', method: 'GET', ...request })).rejects.toThrow(
        'refused: signed request to the public proxy',
      );
      expect(raw.request).not.toHaveBeenCalled();
    }
  });

  it('lets an unsigned public read through the public proxy', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('binance', options(), raw);
    await routed.request({
      route: 'publicCors',
      method: 'GET',
      url: 'https://fapi.binance.com/fapi/v1/exchangeInfo',
      headers: {},
    });
    expect(raw.request.mock.calls[0][0].url).toBe(
      `${getPublicCorsProxy()}/https://fapi.binance.com/fapi/v1/exchangeInfo`,
    );
  });

  it('keeps payload-signed Lighter on the public proxy, but never with a credential header', async () => {
    const base = 'https://mainnet.zklighter.elliot.ai';
    const raw = makeRaw();
    const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), raw);
    await routed.request({
      method: 'POST',
      url: `${base}/api/v1/sendTx`,
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: 'tx_type=14&tx_info=%7B%7D',
    });
    expect(raw.request.mock.calls[0][0].url).toBe(`${getPublicCorsProxy()}/${base}/api/v1/sendTx`);
    await expect(
      routed.request({ method: 'GET', url: `${base}/api/v1/account`, headers: { Authorization: 'token' } }),
    ).rejects.toBeInstanceOf(SignedPublicProxyRefusal);
    expect(raw.request).toHaveBeenCalledTimes(1);
  });

  it('gives the native host no secondary read route on the public proxy, so it fails closed', () => {
    const plan = resolveEngineRoutingPlan(
      'binance',
      options({ secondary_key: 'read-key', secondary_secret: 'read-secret' }),
      'https://fapi.binance.com',
    );
    expect(plan.http.prefix).not.toBe(getPublicCorsProxy());
    expect(plan).not.toHaveProperty('readHttp');
  });
});
