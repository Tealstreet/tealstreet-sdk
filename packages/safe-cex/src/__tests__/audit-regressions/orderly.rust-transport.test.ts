vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { Transport as EngineHostTransport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { AxiosAdapter, AxiosResponse, InternalAxiosRequestConfig } from 'axios';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { TransportProvider } from '../../transport/types';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { createEngineTransport, resolveEngineRoutingPlan } from '../../transport/engine';
import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';
import { setDefaultTransport } from '../../transport/registry';
import { tealstreetTransport } from '../../transport/tealstreet/tealstreetTransport';
import { OrderStatus } from '../../types';
import { adoptEngineCapabilities, engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const origin = 'https://api-evm.orderly.org';
const query = '/v1/order?symbol=PERP_BTC_USDC&client_order_id=a%20b%2Fc';
const body = '{"order_id":9007199254740993,"order_quantity":1.000,"side":"BUY"}';
const signed = () => ({
  method: 'POST',
  url: `${origin}${query}`,
  body,
  headers: { 'orderly-signature': 'exact-signed-bytes', 'Content-Type': 'application/json' },
});
const createBase = () => {
  const stream = (async function* () {
    yield new Uint8Array([123, 125]);
  })();
  const response = { status: 200, headers: 'x-stream: yes', body: stream };
  const base = {
    webSocketHeaders: true,
    now: vi.fn(() => 42),
    request: vi.fn(async () => response),
    connect: vi.fn(() => ({ send: vi.fn(), close: vi.fn() })),
    setTimeout: vi.fn(() => 123),
    clearTimeout: vi.fn(),
  } satisfies EngineHostTransport;
  return { base, response };
};
const provider = (overrides: Partial<TransportProvider> = {}): TransportProvider => ({
  ...directTransport,
  name: 'custom-orderly-provider',
  rest: ({ config }) => ({ baseURL: config.baseURL, proxied: false }),
  ...overrides,
});
const raw = (config: InternalAxiosRequestConfig, data: unknown = body, status = 200): AxiosResponse => ({
  config,
  data,
  status,
  statusText: String(status),
  headers: { 'x-upstream': 'retained', 'retry-after': '1' },
});
const options = (transport?: TransportProvider, extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'unused-test-key',
  secret: 'unused-test-secret',
  transport,
  ...extra,
});

afterEach(() => {
  setDefaultTransport(directTransport);
  setRustEngineTransport(nativeTransport);
  setWasmEngineLoader(async (venue) => {
    const { loadEngine } = await import('@tealstreet/safe-cex-wasm/engine');
    return loadEngine(venue);
  });
});

describe('Orderly Rust transport boundary', () => {
  it('promptly rejects start and getAccount on disposal when the WASM loader never settles', async () => {
    const { base } = createBase();
    const load = vi.fn(() => new Promise<WasmEngineModule>(() => {}));
    setWasmEngineLoader(load);
    setRustEngineTransport(() => base);
    const exchange = new RustEngineExchange('mode', options(undefined, { publicDataOnly: true }), new DefaultStore());
    const startup = exchange.start();
    expect(exchange.start()).toBe(startup);
    const account = exchange.getAccount();
    const settled = Promise.allSettled([startup, account]);
    exchange.dispose();
    let deadline: ReturnType<typeof setTimeout> | undefined;
    try {
      const results = await Promise.race([
        settled,
        new Promise<never>((_, reject) => {
          deadline = setTimeout(() => reject(new Error('Disposed startup did not settle promptly')), 100);
        }),
      ]);
      expect(results).toEqual([
        { status: 'rejected', reason: expect.objectContaining({ message: 'Exchange disposed during startup' }) },
        { status: 'rejected', reason: expect.objectContaining({ message: 'Exchange disposed during startup' }) },
      ]);
      expect((exchange as unknown as { startupWaiters: Set<() => void> }).startupWaiters.size).toBe(0);
      expect(load).toHaveBeenCalledTimes(1);
      expect(base.connect).not.toHaveBeenCalled();
      expect(base.request).not.toHaveBeenCalled();
    } finally {
      clearTimeout(deadline);
      exchange.dispose();
    }
  });

  it('rejects startup disposed while the WASM loader is pending before constructing the engine', async () => {
    const { base } = createBase();
    const construct = vi.fn();
    let loaded!: (module: WasmEngineModule) => void;
    setWasmEngineLoader(
      () =>
        new Promise((resolve) => {
          loaded = resolve;
        }),
    );
    setRustEngineTransport(() => base);
    const exchange = new RustEngineExchange('mode', options(undefined, { publicDataOnly: true }), new DefaultStore());
    const startup = exchange.start();
    expect(exchange.start()).toBe(startup);
    const starting = expect(startup).rejects.toThrow('disposed during startup');
    const account = expect(exchange.getAccount()).rejects.toThrow('disposed during startup');
    exchange.dispose();
    loaded({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] });
    await Promise.all([starting, account]);
    expect(construct).not.toHaveBeenCalled();
    expect(base.connect).not.toHaveBeenCalled();
    expect(base.request).not.toHaveBeenCalled();
  });

  it('shares deferred startup and its sole host with concurrent start and getAccount calls', async () => {
    const { base } = createBase();
    let loaded!: (module: WasmEngineModule) => void;
    const load = vi.fn(
      () =>
        new Promise<WasmEngineModule>((resolve) => {
          loaded = resolve;
        }),
    );
    setWasmEngineLoader(load);
    setRustEngineTransport(() => base);
    const noActions = () => '';
    const engine: WasmEngineLike = {
      capabilities: () => engineCapabilities('mode'),
      start: vi.fn(() =>
        JSON.stringify([{ op: 'wsConnect', socket: 1, url: 'wss://ws-evm.orderly.org/ws/stream/account-id' }]),
      ),
      // The engine answers identity from its loaded private account.
      command: vi.fn((json: string) => {
        const command = JSON.parse(json);
        if (command.op !== 'fetchAccount') return noActions();
        const result = JSON.stringify({ userId: 'account-id', affiliateId: 'account-id' });
        return JSON.stringify([{ op: 'done', command: command.command, result }]);
      }),
      takeFrame: () => new Uint8Array(consumerFrame({
        loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
      })),
      snapshotFrame: () => new Uint8Array(),
      wsOpen: noActions,
      wsMessage: noActions,
      wsClose: noActions, wsRouteExhausted: noActions,
      httpStart: noActions,
      httpChunk: noActions,
      httpEnd: noActions,
      httpFailed: noActions,
      timer: noActions,
    };
    const construct = vi.fn(function () {
      return engine;
    });
    const store = new DefaultStore();
    installNativeStartupFrames(engine, store);
    store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
    const exchange = new RustEngineExchange(
      'mode',
      options(undefined, {
        key: '11111111111111111111111111111111',
        secret: '11111111111111111111111111111111',
        applicationId: 'account-id',
      }),
      store,
    );
    try {
      const startup = exchange.start();
      expect(exchange.start()).toBe(startup);
      const account = exchange.getAccount();
      expect(load).toHaveBeenCalledTimes(1);
      expect(construct).not.toHaveBeenCalled();
      loaded({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] });
      await startup;
      await expect(account).resolves.toEqual({ userId: 'account-id', affiliateId: 'account-id' });
      expect(exchange.start()).toBe(startup);
      expect(construct).toHaveBeenCalledTimes(1);
      expect(engine.start).toHaveBeenCalledTimes(1);
      expect(base.connect).toHaveBeenCalledTimes(1);
    } finally {
      exchange.dispose();
    }
    expect(base.connect.mock.results[0].value.close).toHaveBeenCalledTimes(1);
    await expect(exchange.start()).rejects.toThrow('disposed during startup');
    expect(construct).toHaveBeenCalledTimes(1);
  });

  it('retains the original failed startup promise without retrying the loader or constructing a host', async () => {
    const { base } = createBase();
    const failure = new Error('WASM load failed');
    const load = vi.fn(async (): Promise<WasmEngineModule> => {
      throw failure;
    });
    setWasmEngineLoader(load);
    setRustEngineTransport(() => base);
    const exchange = new RustEngineExchange('mode', options(undefined, { publicDataOnly: true }), new DefaultStore());
    try {
      const startup = exchange.start();
      expect(exchange.start()).toBe(startup);
      await expect(startup).rejects.toBe(failure);
      expect(exchange.start()).toBe(startup);
      await expect(exchange.getAccount()).rejects.toBe(failure);
      expect(load).toHaveBeenCalledTimes(1);
      expect(base.connect).not.toHaveBeenCalled();
      expect(base.request).not.toHaveBeenCalled();
    } finally {
      exchange.dispose();
    }
  });

  it.each<ExchangeName>(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'])(
    '%s refuses unsupported order history filters before dispatching a command',
    async (name) => {
      const exchange = new RustEngineExchange(name, options(undefined, { publicDataOnly: true }), new DefaultStore());
      adoptEngineCapabilities(exchange);
      // Supply the engine record without starting a host; the capability guard must prevent dispatch.
      for (const filter of [{ orderId: '42' }, { status: [OrderStatus.Closed] }, { status: [] }]) {
        await expect(exchange.fetchHistoricalOrders(filter)).rejects.toThrow(
          'does not support orderId or status filters',
        );
      }
      exchange.dispose();
    },
  );

  it('preserves history dispatch for supported Orderly options and other venues', async () => {
    for (const name of ['mode', 'binance'] as const) {
      const exchange = new RustEngineExchange(name, options(undefined, { publicDataOnly: true }), new DefaultStore());
      adoptEngineCapabilities(exchange);
      try {
        await expect(
          exchange.fetchHistoricalOrders({ symbol: 'BTCUSDC', from: 1000, to: 2000, limit: 5 }),
        ).rejects.toThrow('engine is not started');
        if (name === 'binance') {
          await expect(exchange.fetchHistoricalOrders({ orderId: '42', status: [OrderStatus.Closed] })).rejects.toThrow(
            'engine is not started',
          );
        }
      } finally {
        exchange.dispose();
      }
    }
  });

  it('keeps direct request bytes and streams through the generic host', async () => {
    const { base } = createBase();
    const transport = createEngineTransport('tealstreet-dex', options(), base);
    const result = await transport.request(signed());
    expect(base.request).toHaveBeenCalledExactlyOnceWith({ ...signed(), signal: expect.any(AbortSignal) });
    const chunks: number[] = [];
    for await (const chunk of result.body) chunks.push(...(typeof chunk === 'number' ? [chunk] : chunk));
    expect(chunks).toEqual([123, 125]);
    transport.dispose?.();
  });

  it.each<ExchangeName>(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'])(
    '%s uses shared direct/proxy routes and keeps escaped queries and signed body lexemes exact', async (name) => {
      const { base } = createBase();
      base.request.mockResolvedValue({ status: 200, headers: '', body: new Uint8Array() } as never);
      const authorize = vi.fn(async (config: InternalAxiosRequestConfig) => {
        config.headers.set('Authorization', 'Bearer intermediary');
      });
      const rest = vi.fn(({ config }: Parameters<TransportProvider['rest']>[0]) => ({
        baseURL: `https://proxy.test/${config.baseURL}`, proxied: true, authorize,
      }));
      const transport = createEngineTransport(name, options(provider({ rest })), base);
      await transport.request(signed());
      expect(base.request).toHaveBeenCalledExactlyOnceWith({
        ...signed(), url: `https://proxy.test/${origin}${query}`,
        headers: { ...signed().headers, Authorization: 'Bearer intermediary' }, signal: expect.any(AbortSignal),
      });
      expect(authorize).toHaveBeenCalledOnce();
      expect(rest.mock.calls[0][0].config.baseURL).toBe(origin);
      transport.dispose?.();
    },
  );

  it('honors per-account transport over the registered default and preserves testnet host', async () => {
    const { base } = createBase();
    base.request.mockResolvedValue({ status: 200, headers: '', body: new Uint8Array() } as never);
    const unused = vi.fn();
    setDefaultTransport(provider({ rest: unused }));
    const rest = vi.fn(({ config }: Parameters<TransportProvider['rest']>[0]) => ({ baseURL: config.baseURL, proxied: false }));
    const transport = createEngineTransport('mode', options(provider({ rest }), { testnet: true }), base);
    const request = { ...signed(), url: signed().url.replace('https://api-', 'https://testnet-api-') };
    await transport.request(request);
    expect(base.request).toHaveBeenCalledExactlyOnceWith({ ...request, signal: expect.any(AbortSignal) });
    expect(rest.mock.calls[0][0].config.baseURL).toBe('https://testnet-api-evm.orderly.org');
    expect(unused).not.toHaveBeenCalled();
    transport.dispose?.();
  });

  it('uses the raw adapter variant and preserves rejected HTTP status/body without resending', async () => {
    const { base } = createBase();
    const adapter = vi.fn<AxiosAdapter>(async (config) => raw(config, body, 429));
    const restAdapter = vi.fn(() => adapter);
    const transport = createEngineTransport('woofi', options(provider({ restAdapter }), { tealstreetTunnel: true }), base);
    const result = await transport.request(signed());
    expect(restAdapter).toHaveBeenCalledExactlyOnceWith({ exchange: 'woofi', options: expect.any(Object),
      variant: { region: 'SG', raw: true, WebSocket: undefined } });
    expect(adapter).toHaveBeenCalledOnce();
    const config = adapter.mock.calls[0][0];
    expect(config.url).toBe(`${origin}${query}`);
    expect(config.data).toBe(body);
    expect(config.headers.get('orderly-signature')).toBe('exact-signed-bytes');
    expect(result.status).toBe(429);
    expect(new TextDecoder().decode(result.body as Uint8Array)).toBe(body);
    expect(base.request).not.toHaveBeenCalled();
    transport.dispose?.();
  });

  it('rejects parsed tunnel bodies and never retries an ambiguous write', async () => {
    const { base } = createBase();
    for (const failure of ['parsed', 'lost'] as const) {
      const adapter = vi.fn<AxiosAdapter>(async (config) => {
        if (failure === 'lost') throw new Error('response lost after accepted write');
        return raw(config, { order_id: 9007199254740992 });
      });
      const transport = createEngineTransport('what', options(provider({ restAdapter: () => adapter }), { tealstreetTunnel: true }), base);
      await expect(transport.request(signed())).rejects.toThrow(failure === 'lost' ? 'response lost' : 'raw response body');
      expect(adapter).toHaveBeenCalledOnce();
      expect(base.request).not.toHaveBeenCalled();
      transport.dispose?.();
    }
  });

  it.each([false, true])('refuses signed packet mutations before any dispatch (HOW=%s)', async (tunnel) => {
    const mutations = [
      (config: InternalAxiosRequestConfig) => { config.data = '{}'; },
      (config: InternalAxiosRequestConfig) => { config.url = '/v1/other'; },
      (config: InternalAxiosRequestConfig) => { config.baseURL = 'https://other.test'; },
      (config: InternalAxiosRequestConfig) => { config.method = 'DELETE'; },
      (config: InternalAxiosRequestConfig) => { config.params = { added: 1 }; },
      (config: InternalAxiosRequestConfig) => { config.headers.set('orderly-signature', 'changed'); },
      (config: InternalAxiosRequestConfig) => { config.signal = new AbortController().signal; },
    ];
    for (const mutate of mutations) {
      const { base } = createBase();
      const adapter = vi.fn<AxiosAdapter>(async (config) => raw(config));
      const transport = createEngineTransport('mode', options(provider({
        restAdapter: () => adapter,
        rest: () => ({ baseURL: 'https://proxy.test', proxied: true, authorize: async (config) => mutate(config) }),
      }), { tealstreetTunnel: tunnel }), base);
      await expect(transport.request(signed())).rejects.toThrow('changed signed request');
      expect(base.request).not.toHaveBeenCalled();
      expect(adapter).not.toHaveBeenCalled();
      transport.dispose?.();
    }
  });

  it('protects venue headers from a provider mutating routing inputs', async () => {
    const { base } = createBase();
    const transport = createEngineTransport('mode', options(provider({ rest: ({ config }) => {
      (config.headers as Record<string, string>)['orderly-signature'] = 'changed';
      return { baseURL: config.baseURL, proxied: false };
    } })), base);
    await expect(transport.request(signed())).rejects.toThrow('changed signed request');
    expect(base.request).not.toHaveBeenCalled();
    transport.dispose?.();
  });

  it('disposes while intermediary authorization is pending and never dispatches the late write', async () => {
    const { base } = createBase();
    let authorized!: () => void;
    const transport = createEngineTransport('mode', options(provider({ rest: () => ({
      baseURL: 'https://proxy.test', proxied: true, authorize: () => new Promise((resolve) => { authorized = resolve; }),
    }) })), base);
    const result = transport.request(signed());
    const rejected = expect(result).rejects.toThrow('disposed');
    transport.dispose?.();
    await rejected;
    authorized();
    await Promise.resolve();
    expect(base.request).not.toHaveBeenCalled();
  });

  it.each([false, true])('routes declared public/funding/private sockets using exact venue URLs (testnet=%s)', (testnet) => {
    const { base } = createBase();
    const transport = createEngineTransport('mode', options(provider(), { testnet }), base);
    const prefix = testnet ? 'testnet-' : '';
    const publicUrl = `wss://${prefix}ws-evm.orderly.org/ws/stream/account-id`;
    const privateUrl = `wss://${prefix}ws-private-evm.orderly.org/v2/ws/private/stream/account-id`;
    for (const [url, channel, feed] of [[publicUrl, 'public', 'market'], [publicUrl, 'public', 'funding'], [privateUrl, 'private', 'account']] as const) {
      const handlers = { channel, context: { feed, recover: false }, onOpen: vi.fn(), onMessage: vi.fn(), onClose: vi.fn() };
      transport.connect(url, handlers);
      expect(base.connect).toHaveBeenLastCalledWith(url, { ...handlers, headers: {}, onMessage: expect.any(Function), onBinary: expect.any(Function) });
    }
    transport.dispose?.();
  });

  it.each([false, true].flatMap((testnet) => [false, true].map((privateRoute) => ({ testnet, privateRoute }))))(
    'preflights required custom upgrade headers before SDK construction: %j', async ({ testnet, privateRoute }) => {
      const { base } = createBase();
      const construct = vi.fn();
      setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
      setRustEngineTransport(() => ({ ...base, webSocketHeaders: false }));
      const wsConnection = vi.fn((url: string) => url.includes('ws-private-') === privateRoute
        ? { kind: 'headers' as const, headers: { Authorization: 'required-custom-auth' } } : undefined);
      const exchange = new RustEngineExchange('mode', options(provider({ wsConnection }), {
        key: '11111111111111111111111111111111', secret: '11111111111111111111111111111111',
        publicDataOnly: !privateRoute, testnet, ...(privateRoute ? { applicationId: 'account-id' } : {}),
      }), new DefaultStore());
      try {
        await expect(exchange.start()).rejects.toThrow('upgrade header support');
        const prefix = testnet ? 'testnet-' : '';
        expect(wsConnection).toHaveBeenLastCalledWith(privateRoute
          ? `wss://${prefix}ws-private-evm.orderly.org/v2/ws/private/stream/account-id`
          : `wss://${prefix}ws-evm.orderly.org/ws/stream/OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY`);
        expect(construct).not.toHaveBeenCalled();
        expect(base.connect).not.toHaveBeenCalled();
        expect(base.request).not.toHaveBeenCalled();
      } finally { exchange.dispose(); }
    },
  );

  it('preserves custom upgrade headers on capable hosts and rejects later route changes on incapable hosts', () => {
    const { base } = createBase();
    const url = 'wss://ws-evm.orderly.org/ws/stream/account-id';
    const handlers = { channel: 'public' as const, onOpen: vi.fn(), onMessage: vi.fn(), onClose: vi.fn() };
    let headers: Record<string, string> = {};
    const custom = provider({ wsConnection: () => ({ kind: 'headers', headers }) });
    const incapable = createEngineTransport('mode', options(custom), { ...base, webSocketHeaders: false });
    headers = { Authorization: 'required-custom-auth' };
    expect(() => incapable.connect(url, handlers)).toThrow('upgrade header support');
    expect(base.connect).not.toHaveBeenCalled();
    const capable = createEngineTransport('mode', options(custom), base);
    capable.connect(url, handlers);
    expect(base.connect).toHaveBeenLastCalledWith(url, { ...handlers, headers, onMessage: expect.any(Function), onBinary: expect.any(Function) });
    incapable.dispose?.();
    capable.dispose?.();
  });

  it.each<ExchangeName>(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'])(
    '%s native routing plan uses the same serializable HOW policy', (name) => {
      const plan = resolveEngineRoutingPlan(name, options(tealstreetTransport, { tealstreetTunnel: true }), origin);
      expect(plan.http).toMatchObject({ mode: 'how', prefix: '', authorize: false,
        tunnelUrl: 'wss://fsjnpy143pbz.asia.tealstreet.io/tunnel' });
      expect(plan.ws).toEqual({ public: {}, private: {}, business: {} });
      expect(JSON.stringify(plan)).not.toContain('unused-test-secret');
    },
  );
});
