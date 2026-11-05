import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

// These tests pin today's secondary read-key behaviour (secondaryReadKeys.ts bypasses it).
vi.mock('../secondaryReadKeys', () => ({ SECONDARY_READ_KEYS_ENABLED: true }));

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

const LIGHTER_READ_TOKEN = `1700000060:42:3:${'ab'.repeat(80)}`;

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

  it.each([
    ['GET', '/api/v1/trades'],
    ['GET', '/api/v1/accountInactiveOrders'],
    ['GET', '/api/v1/positionFunding'],
    ['GET', '/api/v1/accountLimits'],
    ['POST', '/api/v1/changeAccountTier'],
  ])('preserves Lighter read-token Authorization for %s %s', async (method, path) => {
    const raw = makeRaw();
    const dispatch = vi.fn(async () => ({ status: 200, body: new Uint8Array() }));
    const bind = vi.fn((_packet: Parameters<Transport['request']>[0]) => ({ ready: () => true, dispatch }));
    const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), {
      ...raw,
      prepareHttp: async () => ({ bind }),
    });
    const original = {
      request: 81,
      method,
      intent: 'primary' as const,
      url: `https://mainnet.zklighter.elliot.ai${path}?account_index=42`,
      headers: {},
      timeoutMs: 20_000,
    };
    try {
      const prepared = await routed.prepareHttp!(original);
      const bound = prepared.bind({
        ...original,
        headers: { authorization: LIGHTER_READ_TOKEN },
        ...(method === 'POST' ? { body: 'account_index=42&new_tier=premium' } : {}),
      });
      expect(bind.mock.calls[0]![0]).toMatchObject({
        url: `${getPublicCorsProxy()}/${original.url}`,
        headers: { authorization: LIGHTER_READ_TOKEN },
      });
      expect(dispatch).not.toHaveBeenCalled();
      await bound.dispatch();
      expect(dispatch).toHaveBeenCalledOnce();
    } finally {
      routed.dispose?.();
    }
  });

  it.each([
    ['GET', '/api/v1/account'],
    ['POST', '/api/v1/accountInactiveOrders'],
    ['GET', '/api/v1/changeAccountTier'],
    ['DELETE', '/api/v1/accountLimits'],
    ['GET', '/api/v1/accountLimits/extra'],
  ])('refuses Lighter Authorization outside the exact method/path set: %s %s', async (method, path) => {
    const raw = makeRaw();
    const bind = vi.fn(() => ({ ready: () => true, dispatch: vi.fn() }));
    const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), {
      ...raw,
      prepareHttp: async () => ({ bind }),
    });
    const original = {
      request: 82,
      method,
      intent: 'primary' as const,
      url: `https://mainnet.zklighter.elliot.ai${path}`,
      headers: {},
      timeoutMs: 20_000,
    };
    try {
      const prepared = await routed.prepareHttp!(original);
      expect(() => prepared.bind({ ...original, headers: { Authorization: LIGHTER_READ_TOKEN } })).toThrow(
        SignedPublicProxyRefusal,
      );
      expect(bind).not.toHaveBeenCalled();
      expect(raw.request).not.toHaveBeenCalled();
    } finally {
      routed.dispose?.();
    }
  });

  it.each(['signature=abc', 'sign=abc', '%73ignature=abc'])(
    'refuses %s even with the Lighter payload capability',
    async (query) => {
      const raw = makeRaw();
      const bind = vi.fn(() => ({ ready: () => true, dispatch: vi.fn() }));
      const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), {
        ...raw,
        prepareHttp: async () => ({ bind }),
      });
      const original = {
        request: 83,
        method: 'GET',
        intent: 'primary' as const,
        url: 'https://mainnet.zklighter.elliot.ai/api/v1/trades',
        headers: {},
        timeoutMs: 20_000,
      };
      try {
        const prepared = await routed.prepareHttp!(original);
        expect(() => prepared.bind({ ...original, url: `${original.url}?${query}`, headers: {} })).toThrow(
          SignedPublicProxyRefusal,
        );
        expect(bind).not.toHaveBeenCalled();
      } finally {
        routed.dispose?.();
      }
    },
  );

  it.each(['X-MBX-APIKEY', 'APIKey', 'ACCESS-KEY', 'X-API-Key'])(
    'still refuses other credentials on a Lighter token path: %s',
    async (header) => {
      const raw = makeRaw();
      const bind = vi.fn(() => ({ ready: () => true, dispatch: vi.fn() }));
      const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), {
        ...raw,
        prepareHttp: async () => ({ bind }),
      });
      const original = {
        request: 84,
        method: 'GET',
        intent: 'primary' as const,
        url: 'https://mainnet.zklighter.elliot.ai/api/v1/trades',
        headers: {},
        timeoutMs: 20_000,
      };
      try {
        const prepared = await routed.prepareHttp!(original);
        expect(() =>
          prepared.bind({ ...original, headers: { authorization: LIGHTER_READ_TOKEN, [header]: 'api-secret' } }),
        ).toThrow(SignedPublicProxyRefusal);
        expect(bind).not.toHaveBeenCalled();
      } finally {
        routed.dispose?.();
      }
    },
  );

  it('does not grant the same token to another venue, origin, or opaque secret value', async () => {
    for (const [exchange, origin, token] of [
      ['binance', 'https://mainnet.zklighter.elliot.ai', LIGHTER_READ_TOKEN],
      ['lighter', 'https://other.invalid', LIGHTER_READ_TOKEN],
      ['lighter', 'https://mainnet.zklighter.elliot.ai', 'api-secret'],
    ] as const) {
      const raw = makeRaw();
      const bind = vi.fn(() => ({ ready: () => true, dispatch: vi.fn() }));
      const routed = createEngineTransport(exchange, options({ tealstreetProxy: false }), {
        ...raw,
        prepareHttp: async () => ({ bind }),
      });
      const original = {
        request: 85,
        method: 'GET',
        route: 'publicCors' as const,
        intent: 'public' as const,
        url: `${origin}/api/v1/trades`,
        headers: {},
        timeoutMs: 20_000,
      };
      try {
        const prepared = await routed.prepareHttp!(original);
        expect(() => prepared.bind({ ...original, headers: { Authorization: token } })).toThrow(
          SignedPublicProxyRefusal,
        );
        expect(bind).not.toHaveBeenCalled();
      } finally {
        routed.dispose?.();
      }
    }
  });

  it.each([
    ['lighter', false, 'https://mainnet.zklighter.elliot.ai'],
    ['lighter', true, 'https://testnet-api.lighter.xyz'],
    ['lighter-robinhood', false, 'https://api.rh.lighter.xyz'],
  ] as const)('permits the same token in legacy and managed %s testnet=%s reads', async (exchange, testnet, origin) => {
    const raw = makeRaw();
    const routed = createEngineTransport(exchange, options({ tealstreetProxy: false, testnet }), raw);
    try {
      await routed.request({
        method: 'GET',
        url: `${origin}/api/v1/trades`,
        headers: { Authorization: LIGHTER_READ_TOKEN },
      });
      expect(raw.request.mock.calls[0]![0]).toMatchObject({
        url: `${getPublicCorsProxy()}/${origin}/api/v1/trades`,
        headers: { Authorization: LIGHTER_READ_TOKEN },
      });
    } finally {
      routed.dispose?.();
    }
  });

  it('refuses tokens for another deployment and for a profile without the capability', async () => {
    for (const [exchange, origin] of [
      ['lighter', 'https://mainnet.zklighter.elliot.ai'],
      ['lighter-robinhood', 'https://api.rh-testnet.lighter.xyz'],
    ] as const) {
      const raw = makeRaw();
      const routed = createEngineTransport(exchange, options({ tealstreetProxy: false, testnet: true }), raw);
      try {
        await expect(
          routed.request({
            route: 'publicCors',
            method: 'GET',
            url: `${origin}/api/v1/trades`,
            headers: { Authorization: LIGHTER_READ_TOKEN },
          }),
        ).rejects.toThrow(SignedPublicProxyRefusal);
        expect(raw.request).not.toHaveBeenCalled();
      } finally {
        routed.dispose?.();
      }
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

describe('review3092 query and legacy binding controls', () => {
  const lighterRouted = (
    raw: ReturnType<typeof makeRaw>,
    bind = vi.fn((_packet: Parameters<Transport['request']>[0]) => ({
      ready: () => true,
      dispatch: vi.fn(async () => ({ status: 200, body: new Uint8Array() })),
    })),
  ) => {
    const cancel = vi.fn();
    const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), {
      ...raw,
      prepareHttp: async () => ({ bind, cancel }),
    });
    return { routed, bind, cancel };
  };
  // sign.rs AuthPlacement::Query: account.rs:1092 accountActiveOrders appends auth=<token>.
  it.each(['/api/v1/account?by=index&value=42', '/api/v1/sendTx'])(
    'refuses a query-placed auth token on the unlisted path %s',
    async (path) => {
      const raw = makeRaw();
      const { routed, bind } = lighterRouted(raw);
      const url = `https://mainnet.zklighter.elliot.ai${path}`;
      const original = {
        request: 90,
        method: 'GET',
        intent: 'primary' as const,
        route: 'primary' as const,
        url,
        headers: {},
        timeoutMs: 20_000,
      };
      try {
        const prepared = await routed.prepareHttp!(original);
        const sep = url.includes('?') ? '&' : '?';
        let refused = false;
        try {
          prepared.bind({ ...original, url: `${url}${sep}auth=${LIGHTER_READ_TOKEN}` });
        } catch (e) {
          refused = e instanceof SignedPublicProxyRefusal;
        }
        // Query tokens cannot extend the operation allowlist.
        expect({ path, refused, bound: bind.mock.calls.length }).toEqual({ path, refused: true, bound: 0 });
      } finally {
        routed.dispose?.();
      }
    },
  );
  it.each([
    ['GET', '/api/v1/trades'],
    ['GET', '/api/v1/accountInactiveOrders'],
    ['GET', '/api/v1/accountActiveOrders'],
    ['GET', '/api/v1/positionFunding'],
    ['GET', '/api/v1/accountLimits'],
    ['POST', '/api/v1/changeAccountTier'],
  ])('preserves the native query token for %s %s', async (method, path) => {
    const raw = makeRaw();
    const { routed, bind } = lighterRouted(raw);
    const original = {
      request: 92,
      method,
      intent: 'primary' as const,
      route: 'primary' as const,
      url: `https://mainnet.zklighter.elliot.ai${path}?account_index=42`,
      headers: {},
      timeoutMs: 20_000,
    };
    const signed = { ...original, url: `${original.url}&auth=${LIGHTER_READ_TOKEN}` };
    try {
      const prepared = await routed.prepareHttp!(original);
      const bound = prepared.bind(signed);
      expect(bind).toHaveBeenCalledOnce();
      expect(bind.mock.lastCall![0].url).toBe(`${getPublicCorsProxy()}/${signed.url}`);
      expect(raw.request).not.toHaveBeenCalled();
      expect(bind.mock.results[0]!.value.dispatch).not.toHaveBeenCalled();
      await bound.dispatch();
      expect(bind.mock.results[0]!.value.dispatch).toHaveBeenCalledOnce();
      expect(() => bound.dispatch()).toThrow();
    } finally {
      routed.dispose?.();
    }
  });

  it.each([
    ['binance', 'GET', 'https://mainnet.zklighter.elliot.ai/api/v1/accountActiveOrders', LIGHTER_READ_TOKEN, 'auth'],
    ['lighter', 'GET', 'https://other.invalid/api/v1/accountActiveOrders', LIGHTER_READ_TOKEN, 'auth'],
    ['lighter', 'POST', 'https://mainnet.zklighter.elliot.ai/api/v1/sendTx', LIGHTER_READ_TOKEN, 'auth'],
    ['lighter', 'GET', 'https://mainnet.zklighter.elliot.ai/api/v1/account', LIGHTER_READ_TOKEN, 'a%75th'],
    ['lighter', 'GET', 'https://mainnet.zklighter.elliot.ai/api/v1/account', LIGHTER_READ_TOKEN, 'AUTH'],
    ['lighter', 'GET', 'https://mainnet.zklighter.elliot.ai/api/v1/accountActiveOrders', 'api-secret', 'auth'],
    ['lighter', 'GET', 'https://mainnet.zklighter.elliot.ai/api/v1/accountActiveOrders', '', 'auth'],
  ] as const)('refuses unapproved query credentials: %s %s %s %s %s', async (exchange, method, url, token, name) => {
    const raw = makeRaw();
    const bind = vi.fn((_packet: Parameters<Transport['request']>[0]) => ({ ready: () => true, dispatch: vi.fn() }));
    const cancel = vi.fn();
    const routed = createEngineTransport(exchange, options({ tealstreetProxy: false }), {
      ...raw,
      prepareHttp: async () => ({ bind, cancel }),
    });
    const original = {
      request: 93,
      method,
      intent: 'public' as const,
      route: 'publicCors' as const,
      url,
      headers: {},
      timeoutMs: 20_000,
    };
    try {
      const prepared = await routed.prepareHttp!(original);
      expect(() => prepared.bind({ ...original, url: `${url}?${name}=${token}` })).toThrow(SignedPublicProxyRefusal);
      expect(bind).not.toHaveBeenCalled();
      expect(raw.request).not.toHaveBeenCalled();
      expect(cancel).toHaveBeenCalledOnce();
    } finally {
      routed.dispose?.();
    }
  });

  it('refuses an invalid duplicate auth value beside an otherwise valid token', async () => {
    const raw = makeRaw();
    const { routed, bind } = lighterRouted(raw);
    const original = {
      request: 94,
      method: 'GET',
      intent: 'primary' as const,
      url: 'https://mainnet.zklighter.elliot.ai/api/v1/accountActiveOrders',
      headers: {},
      timeoutMs: 20_000,
    };
    try {
      const prepared = await routed.prepareHttp!(original);
      expect(() =>
        prepared.bind({ ...original, url: `${original.url}?auth=${LIGHTER_READ_TOKEN}&auth=api-secret` }),
      ).toThrow(SignedPublicProxyRefusal);
      expect(bind).not.toHaveBeenCalled();
    } finally {
      routed.dispose?.();
    }
  });

  it('legacy query tokens work only when the prepared URL already contains the token', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), raw);
    const url = `https://mainnet.zklighter.elliot.ai/api/v1/accountActiveOrders?auth=${LIGHTER_READ_TOKEN}`;
    try {
      await routed.request({ method: 'GET', url, headers: {} });
      expect(raw.request).toHaveBeenCalledOnce();
      expect(raw.request.mock.lastCall![0].url).toBe(`${getPublicCorsProxy()}/${url}`);
    } finally {
      routed.dispose?.();
    }
  });

  it.each([false, true])(
    'legacy URL changes are refused even without credentials or with a mutated input: %s',
    async (mutateInput) => {
      const raw = makeRaw();
      const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), raw);
      const original = {
        request: 95,
        method: 'GET',
        url: 'https://mainnet.zklighter.elliot.ai/api/v1/account',
        headers: {},
      };
      try {
        const prepared = await routed.prepareHttp!(original as never);
        const replacement = 'https://mainnet.zklighter.elliot.ai/api/v1/accountLimits';
        if (mutateInput) original.url = replacement;
        expect(() => prepared.bind({ ...original, url: replacement } as never)).toThrow();
        expect(raw.request).not.toHaveBeenCalled();
      } finally {
        routed.dispose?.();
      }
    },
  );

  it('legacy (no intent) bind checks the URL actually sent', async () => {
    const raw = makeRaw();
    const routed = createEngineTransport('lighter', options({ tealstreetProxy: false }), raw);
    const original = {
      request: 91,
      method: 'GET',
      url: 'https://mainnet.zklighter.elliot.ai/api/v1/account?by=index&value=42',
      headers: {},
    };
    try {
      const prepared = await routed.prepareHttp!(original as never);
      let refused = false;
      try {
        const bound = prepared.bind({
          ...original,
          url: 'https://mainnet.zklighter.elliot.ai/api/v1/accountLimits?account_index=42',
          headers: { Authorization: LIGHTER_READ_TOKEN },
        } as never);
        await bound.dispatch();
      } catch (e) {
        refused = e instanceof SignedPublicProxyRefusal;
      }
      const sent = raw.request.mock.calls.map(([r]) => ({
        url: r.url,
        auth: r.headers.Authorization ?? r.headers.authorization,
      }));
      expect({ refused, sent }).toEqual({ refused: true, sent: [] });
    } finally {
      routed.dispose?.();
    }
  });
});
