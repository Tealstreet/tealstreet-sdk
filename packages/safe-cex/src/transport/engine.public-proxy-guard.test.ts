import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { EngineHost } from '@tealstreet/safe-cex-wasm/host';
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

  it('refuses Lighter authorization added only after managed preparation, before binding or dispatch', async () => {
    const raw = makeRaw();
    const cancel = vi.fn();
    const dispatch = vi.fn(async () => ({ status: 200, body: new Uint8Array() }));
    const bind = vi.fn(() => ({ ready: () => true, dispatch }));
    const prepareHttp = vi.fn(async () => ({ bind, cancel }));
    let publicProxy = getPublicCorsProxy();
    const routed = createEngineTransport(
      'lighter',
      options({
        tealstreetProxy: false,
        transport: { ...tealstreetTransport, publicCorsProxy: () => publicProxy },
      }),
      { ...raw, prepareHttp },
    );
    const original = {
      request: 77,
      method: 'GET' as const,
      intent: 'primary' as const,
      route: 'primary' as const,
      url: 'https://mainnet.zklighter.elliot.ai/api/v1/accountInactiveOrders?account_index=1&limit=100',
      headers: {},
      timeoutMs: 20_000,
    };
    try {
      const prepared = await routed.prepareHttp!(original);
      expect(prepareHttp).toHaveBeenCalledOnce();
      expect(dispatch).not.toHaveBeenCalled();
      publicProxy = 'https://replacement.invalid/public_proxy';
      expect(() => prepared.bind({ ...original, headers: { authorization: 'signed-lighter-token' } })).toThrow(
        SignedPublicProxyRefusal,
      );
      expect(bind).not.toHaveBeenCalled();
      expect(dispatch).not.toHaveBeenCalled();
      expect(raw.request).not.toHaveBeenCalled();
      expect(cancel).toHaveBeenCalledOnce();
    } finally {
      routed.dispose?.();
    }
  });

  it.each([
    ['venue key', { 'X-MBX-APIKEY': 'venue-key' }, ''],
    ['authorization', { Authorization: 'venue-token' }, ''],
    ['signature query', {}, '&signature=signed'],
    ['sign query', {}, '&sign=signed'],
  ] as const)('rechecks a public packet with a signer-added %s', async (_name, headers, suffix) => {
    const raw = makeRaw();
    const cancel = vi.fn();
    const bind = vi.fn(() => ({ ready: () => true, dispatch: vi.fn() }));
    const routed = createEngineTransport('binance', options(), {
      ...raw,
      prepareHttp: async () => ({ bind, cancel }),
    });
    const original = {
      request: 78,
      method: 'GET',
      route: 'publicCors' as const,
      intent: 'public' as const,
      url: 'https://fapi.binance.com/fapi/v1/klines?symbol=BTCUSDT&interval=1m',
      headers: {},
      timeoutMs: 20_000,
    };
    try {
      const prepared = await routed.prepareHttp!(original);
      expect(() => prepared.bind({ ...original, url: original.url + suffix, headers })).toThrow(
        SignedPublicProxyRefusal,
      );
      expect(bind).not.toHaveBeenCalled();
      expect(raw.request).not.toHaveBeenCalled();
      expect(cancel).toHaveBeenCalledOnce();
    } finally {
      routed.dispose?.();
    }
  });

  it('reports a signer-added refusal as invalid before the real managed host can claim or send', async () => {
    const raw = makeRaw();
    const dispatch = vi.fn(async () => ({ status: 200, body: new Uint8Array() }));
    const rawBind = vi.fn(() => ({ ready: () => true, dispatch }));
    const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), {
      ...raw,
      prepareHttp: async () => ({ bind: rawBind }),
    });
    const packet = {
      request: 79,
      method: 'GET',
      route: 'primary',
      url: 'https://mainnet.zklighter.elliot.ai/api/v1/accountInactiveOrders?account_index=1&limit=100',
      headers: [],
      timeoutMs: 20_000,
    };
    const failed = vi.fn(() => '');
    const claim = vi.fn(() => JSON.stringify({ status: 'released' }));
    const engine = {
      start: () =>
        JSON.stringify([{ op: 'managedHttp', phase: 'prepare', preparation: 'p1', intent: 'primary', packet }]),
      httpPreparationPending: () => true,
      httpPrepared: () =>
        JSON.stringify([
          {
            op: 'managedHttp',
            phase: 'bind',
            preparation: 'p1',
            permit: 'r1',
            packet: { ...packet, headers: [['authorization', 'signed-lighter-token']] },
          },
        ]),
      httpPreparationFailed: failed,
      httpClaim: claim,
      prepareDispatch: () => '',
    };
    const host = new EngineHost(engine as unknown as WasmEngineLike, routed);
    try {
      host.start();
      await vi.waitFor(() => expect(failed).toHaveBeenCalledWith('p1', 'invalid', 42));
      expect(failed).toHaveBeenCalledOnce();
      expect(claim).not.toHaveBeenCalled();
      expect(rawBind).not.toHaveBeenCalled();
      expect(dispatch).not.toHaveBeenCalled();
      expect(raw.request).not.toHaveBeenCalled();
    } finally {
      host.dispose();
      routed.dispose?.();
    }
  });

  it('still binds and dispatches a managed Lighter payload-signed placement exactly once', async () => {
    const raw = makeRaw();
    const dispatch = vi.fn(async () => ({ status: 200, body: new Uint8Array() }));
    const bind = vi.fn((_packet: Parameters<Transport['request']>[0]) => ({ ready: () => true, dispatch }));
    const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), {
      ...raw,
      prepareHttp: async () => ({ bind }),
    });
    const original = {
      request: 80,
      method: 'POST',
      intent: 'primary' as const,
      url: 'https://mainnet.zklighter.elliot.ai/api/v1/sendTx',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      timeoutMs: 20_000,
    };
    const body = 'tx_type=14&tx_info=%7B%22Sig%22%3A%22signed%22%7D';
    try {
      const prepared = await routed.prepareHttp!(original);
      const bound = prepared.bind({ ...original, body });
      expect(bind.mock.calls[0]?.[0]).toMatchObject({
        url: `${getPublicCorsProxy()}/${original.url}`,
        body,
      });
      expect(dispatch).not.toHaveBeenCalled();
      await bound.dispatch();
      expect(dispatch).toHaveBeenCalledOnce();
      expect(() => bound.dispatch()).toThrow();
    } finally {
      routed.dispose?.();
    }
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
