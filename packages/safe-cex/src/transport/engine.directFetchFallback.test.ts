// REST for a plain direct account whose browser cannot reach the venue (CORS, network,
// region block, extension): unsigned public reads move to the HTTP proxy, signed ones stay.
import type { HttpResponse, Transport } from '@tealstreet/safe-cex-wasm/host';

import { describe, expect, it, vi } from 'vitest';

import { bindProxyTokenLookup } from '../accessToken';
import { createEngineTransport } from './engine';
import { tealstreetTransport } from './tealstreet/tealstreetTransport';

vi.mock('../accessToken', () => ({
  getProxyToken: vi.fn(async () => 'proxy-fixture'),
  isTokenExpired: () => false,
  captureProxyTokenProvider: () => null,
  withProxyTokenProvider: (_provider: unknown, fn: () => unknown) => fn(),
  bindProxyTokenLookup: vi.fn(() => async () => 'proxy-fixture'),
  ProxyTokenUnavailableError: class extends Error {},
  refreshProxyToken: vi.fn(async () => undefined),
}));

const PROXY = 'https://fsjnpy143pbz-tyo.tealstreet.io/api/exchange_proxy/';
const PUBLIC_PROXY = 'https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy/';
const TICKERS = 'https://api.bybit.com/v5/market/tickers?category=linear';
const BYBIT_DEMO_TICKERS = 'https://api-demo.bybit.com/v5/market/tickers?category=linear';
const ok: HttpResponse = { status: 200, headers: '', body: new TextEncoder().encode('{"retCode":0}') };

/** The browser's fetch: rejects every venue-direct URL, answers anything else. */
const browser = (directError: () => unknown = () => new TypeError('Failed to fetch'), proxyReply?: () => unknown) => {
  const dispatched: string[] = [];
  const send = async (url: string): Promise<HttpResponse> => {
    dispatched.push(url);
    if (!url.startsWith('https://fsjnpy143pbz-tyo.tealstreet.io/')) throw directError();
    if (proxyReply) throw proxyReply();
    return ok;
  };
  const raw: Transport = {
    now: () => 0,
    setTimeout,
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
    connect: vi.fn(() => ({ send: vi.fn(), close: vi.fn() })),
    request: vi.fn(async (request) => send(request.url)),
    prepareHttp: vi.fn((prepare) => ({
      bind: (network: any) => ({
        ready: () => true,
        dispatch: () => send(network.url ?? prepare.url),
        cancel: () => undefined,
      }),
      cancel: () => undefined,
    })) as never,
  };
  return { raw, dispatched };
};

const bybitDirect = (raw: Transport, failures: string[] = []) =>
  createEngineTransport(
    'bybit',
    { key: 'fixture-key', secret: 'fixture-secret', transport: tealstreetTransport },
    raw,
    undefined,
    (message) => failures.push(message),
  );

const read = (route: ReturnType<typeof createEngineTransport>, request: number, url = TICKERS) => {
  const packet = { request, method: 'GET', url, headers: {}, intent: 'public' as const };
  return Promise.resolve(route.prepareHttp!(packet)).then((prepared) => prepared.bind(packet).dispatch());
};

describe('REST when the browser cannot reach the venue directly', () => {
  it('moves the startup ticker read to the public proxy on the next attempt, without waiting', async () => {
    const { raw, dispatched } = browser();
    const route = bybitDirect(raw);
    try {
      await expect(read(route, 1)).rejects.toThrow(TypeError);
      const started = performance.now();
      const reply = await read(route, 2);
      expect(reply.status).toBe(200);
      expect(performance.now() - started).toBeLessThan(1_000);
      expect(dispatched).toEqual([TICKERS, `${PUBLIC_PROXY}${TICKERS}`]);
      // Sticky for the account's transport: no direct retry per read, so no failure loop.
      await read(route, 3);
      expect(dispatched.slice(2)).toEqual([`${PUBLIC_PROXY}${TICKERS}`]);
    } finally {
      route.dispose!();
    }
  });

  // TEALSTREET-NEXT-EH5: the signed-out `default-demo` account has no session, so
  // /api/proxykey answers 401 and the exchange proxy's token never arrives.
  it('needs no proxy token, so a signed-out keyless account reads at once', async () => {
    vi.mocked(bindProxyTokenLookup).mockImplementation(() => async () => '');
    const { raw, dispatched } = browser();
    const failures: string[] = [];
    const route = createEngineTransport(
      'bybit',
      { key: '', secret: '', publicDataOnly: true, transport: tealstreetTransport },
      raw,
      undefined,
      (message) => failures.push(message),
    );
    try {
      await expect(read(route, 1)).rejects.toThrow(TypeError);
      const started = performance.now();
      expect((await read(route, 2)).status).toBe(200);
      expect(performance.now() - started).toBeLessThan(1_000);
      expect(dispatched).toEqual([TICKERS, `${PUBLIC_PROXY}${TICKERS}`]);
      expect(failures).toEqual([]);
    } finally {
      route.dispose!();
      vi.mocked(bindProxyTokenLookup).mockImplementation(() => async () => 'proxy-fixture');
    }
  });

  it('keeps the exchange proxy for a host the public proxy does not forward', async () => {
    const { raw, dispatched } = browser();
    const route = bybitDirect(raw);
    try {
      await expect(read(route, 1, BYBIT_DEMO_TICKERS)).rejects.toThrow(TypeError);
      expect((await read(route, 2, BYBIT_DEMO_TICKERS)).status).toBe(200);
      expect(dispatched).toEqual([BYBIT_DEMO_TICKERS, `${PROXY}${BYBIT_DEMO_TICKERS}`]);
    } finally {
      route.dispose!();
    }
  });

  it('keeps signed requests on the route the user chose', async () => {
    const { raw, dispatched } = browser();
    const route = bybitDirect(raw);
    const wallet = 'https://api.bybit.com/v5/account/wallet-balance?accountType=UNIFIED';
    try {
      await expect(read(route, 1)).rejects.toThrow(TypeError);
      const packet = {
        request: 2,
        method: 'GET',
        url: wallet,
        headers: { 'X-BAPI-API-KEY': 'fixture-key' },
        intent: 'primary' as const,
      };
      await expect((await route.prepareHttp!(packet)).bind(packet).dispatch()).rejects.toThrow(TypeError);
      expect(dispatched).toEqual([TICKERS, wallet]);
    } finally {
      route.dispose!();
    }
  });

  it('does not fail over when the read was called off rather than failing', async () => {
    const abort = () => Object.assign(new Error('aborted'), { name: 'AbortError' });
    const { raw, dispatched } = browser(abort);
    const route = bybitDirect(raw);
    try {
      await expect(read(route, 1)).rejects.toThrow('aborted');
      await expect(read(route, 2)).rejects.toThrow('aborted');
      expect(dispatched).toEqual([TICKERS, TICKERS]);
    } finally {
      route.dispose!();
    }
  });

  it('says once, visibly, when the proxy fallback fails as well', async () => {
    const { raw } = browser(undefined, () => new TypeError('Failed to fetch'));
    const failures: string[] = [];
    const route = bybitDirect(raw, failures);
    try {
      await expect(read(route, 1)).rejects.toThrow(TypeError);
      expect(failures).toEqual([]);
      await expect(read(route, 2)).rejects.toThrow(TypeError);
      await expect(read(route, 3)).rejects.toThrow(TypeError);
      expect(failures).toEqual([
        '[bybit] The venue could not be reached directly or through the Tealstreet proxy (network error on both routes); public data keeps retrying with backoff',
      ]);
    } finally {
      route.dispose!();
    }
  });

  it('says visibly when there is no proxy route to fall back to', async () => {
    const { raw, dispatched } = browser();
    const failures: string[] = [];
    const route = createEngineTransport(
      'bybit',
      { key: 'fixture-key', secret: 'fixture-secret' },
      raw,
      undefined,
      (message) => failures.push(message),
    );
    try {
      await expect(read(route, 1)).rejects.toThrow(TypeError);
      await expect(read(route, 2)).rejects.toThrow(TypeError);
      expect(dispatched).toEqual([TICKERS, TICKERS]);
      expect(failures).toEqual([
        '[bybit] The venue could not be reached directly (network error) and this account has no proxy route to fall back to; public data keeps retrying with backoff',
      ]);
    } finally {
      route.dispose!();
    }
  });
});
