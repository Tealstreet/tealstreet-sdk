import type { Transport as EngineHostTransport } from '@tealstreet/safe-cex-wasm/host';
import type { AxiosAdapter, AxiosResponse, InternalAxiosRequestConfig } from 'axios';
import type { TransportProvider } from '../../transport/types';
import type { ExchangeOptions } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { orderlyTransport } from '../../exchanges/rust/transport';
import { directTransport } from '../../transport/direct';
import { setDefaultTransport } from '../../transport/registry';

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

afterEach(() => setDefaultTransport(directTransport));

describe('Orderly Rust transport boundary', () => {
  it('keeps the direct transport and its streaming response unchanged', async () => {
    setDefaultTransport(directTransport);
    const { base, response } = createBase();
    const transport = orderlyTransport('tealstreet-dex', options(), base);
    expect(transport).toBe(base);
    const request = signed();
    expect(await transport.request(request)).toBe(response);
    expect(base.request).toHaveBeenCalledExactlyOnceWith(request);
  });

  it('routes overrideProxyUrl while preserving the original escaped query, signed body and response stream', async () => {
    const { base, response } = createBase();
    const transport = orderlyTransport(
      'woofi',
      options(directTransport, { overrideProxyUrl: 'https://proxy.test' }),
      base,
    );
    expect(await transport.request(signed())).toBe(response);
    expect(base.request).toHaveBeenCalledExactlyOnceWith({
      ...signed(),
      url: `https://proxy.test/${origin}${query}`,
    });
  });

  it('uses the registered default provider and authorizes every proxied request without touching signed bytes', async () => {
    const { base, response } = createBase();
    const authorize = vi.fn(async (config: InternalAxiosRequestConfig) => {
      config.headers.set('Authorization', 'Bearer intermediary');
    });
    const rest = vi.fn(() => ({ baseURL: 'https://registered.test/orderly', proxied: true, authorize }));
    const registered = provider({ rest });
    setDefaultTransport(registered);
    const opts = options(undefined, { tealstreetProxy: true, proxyRegion: 'JA' });
    const transport = orderlyTransport('kodiakfi', opts, base);
    expect(rest).toHaveBeenCalledExactlyOnceWith({ exchange: 'kodiakfi', options: opts, config: { baseURL: origin } });
    expect(await transport.request(signed())).toBe(response);
    expect(authorize).toHaveBeenCalledTimes(1);
    expect(base.request).toHaveBeenCalledExactlyOnceWith({
      ...signed(),
      url: `https://registered.test/orderly${query}`,
      headers: { ...signed().headers, Authorization: 'Bearer intermediary' },
    });
  });

  it('honors a per-account provider over the registered default and passes testnet options intact', async () => {
    const { base } = createBase();
    const defaultRest = vi.fn();
    setDefaultTransport(provider({ rest: defaultRest }));
    const rest = vi.fn(({ config }: Parameters<TransportProvider['rest']>[0]) => ({
      baseURL: config.baseURL,
      proxied: false,
    }));
    const opts = options(provider({ rest }), { testnet: true });
    const transport = orderlyTransport('salsadex', opts, base);
    const request = signed();
    await transport.request(request);
    expect(base.request).toHaveBeenCalledExactlyOnceWith(request);
    expect(defaultRest).not.toHaveBeenCalled();
    expect(rest).toHaveBeenCalledExactlyOnceWith({
      exchange: 'salsadex',
      options: opts,
      config: { baseURL: 'https://testnet-api-evm.orderly.org' },
    });
  });

  it('calls a raw tunnel adapter once with the untouched body, query, headers and binary response contract', async () => {
    const { base } = createBase();
    const calls: InternalAxiosRequestConfig[] = [];
    const bytes = new Uint8Array([0, 123, 125, 0]);
    const adapter: AxiosAdapter = vi.fn(async (config) => {
      calls.push(config);
      return raw(config, bytes.subarray(1, 3), 429);
    });
    const restAdapter = vi.fn(() => adapter);
    const opts = options(provider({ restAdapter }), { tealstreetTunnel: true, extra: { recvWindow: 12345 } });
    const transport = orderlyTransport('mode', opts, base);
    const result = await transport.request(signed());
    expect(restAdapter).toHaveBeenCalledExactlyOnceWith({ exchange: 'mode', options: opts, variant: 'default' });
    expect(calls).toHaveLength(1);
    expect(calls[0]).toMatchObject({
      url: query,
      baseURL: origin,
      method: 'POST',
      data: body,
      timeout: 12345,
      responseType: 'arraybuffer',
    });
    expect(calls[0].params).toBeUndefined();
    expect(calls[0].headers.get('orderly-signature')).toBe('exact-signed-bytes');
    expect(result.status).toBe(429);
    expect(result.headers).toContain('retry-after: 1');
    expect([...(result.body as Uint8Array)]).toEqual([123, 125]);
    expect(base.request).not.toHaveBeenCalled();
  });

  it('returns raw rejected HTTP responses and never retries an accepted or ambiguous write', async () => {
    const { base } = createBase();
    for (const mode of ['http', 'network'] as const) {
      const adapter = vi.fn<AxiosAdapter>(async (config) => {
        if (mode === 'http') throw { response: raw(config, '{"success":false,"code":-1}', 503) };
        throw new Error('response lost after accepted write');
      });
      const transport = orderlyTransport(
        'what',
        options(provider({ restAdapter: () => adapter }), { tealstreetTunnel: true }),
        base,
      );
      if (mode === 'http') {
        const response = await transport.request(signed());
        expect(response.status).toBe(503);
        expect(new TextDecoder().decode(response.body as Uint8Array)).toBe('{"success":false,"code":-1}');
      } else await expect(transport.request(signed())).rejects.toThrow('response lost');
      expect(adapter).toHaveBeenCalledTimes(1);
      expect(base.request).not.toHaveBeenCalled();
    }
  });

  it('refuses parsed tunnel responses and detects authorization changing signed content before sending', async () => {
    const { base } = createBase();
    const adapter = vi.fn<AxiosAdapter>(async (config) => raw(config, { order_id: 9007199254740992 }));
    const transport = orderlyTransport(
      'woofi',
      options(provider({ restAdapter: () => adapter }), { tealstreetTunnel: true }),
      base,
    );
    await expect(transport.request(signed())).rejects.toThrow('unparsed response');
    expect(adapter).toHaveBeenCalledTimes(1);
    for (const mutate of [
      (config: InternalAxiosRequestConfig) => {
        config.data = '{}';
      },
      (config: InternalAxiosRequestConfig) => {
        config.url = '/v1/other';
      },
      (config: InternalAxiosRequestConfig) => {
        config.headers.set('orderly-signature', 'changed');
      },
    ]) {
      const changed = orderlyTransport(
        'woofi',
        options(
          provider({
            rest: () => ({
              baseURL: 'https://proxy.test',
              proxied: true,
              authorize: async (config) => {
                mutate(config);
              },
            }),
          }),
        ),
        base,
      );
      await expect(changed.request(signed())).rejects.toThrow('changed signed request');
    }
    expect(base.request).not.toHaveBeenCalled();
  });

  it('fails unsupported route and legacy tunnel options before opening a socket or sending HTTP', () => {
    const { base } = createBase();
    expect(() => orderlyTransport('mode', options(directTransport, { tealstreetProxy: true }), base)).toThrow(
      'direct transport',
    );
    expect(() => orderlyTransport('mode', options(provider(), { tealstreetTunnel: true }), base)).toThrow(
      'preserves signed',
    );
    expect(() =>
      orderlyTransport(
        'mode',
        options(provider({ name: 'tealstreet', restAdapter: () => vi.fn() }), { tealstreetTunnel: true }),
        base,
      ),
    ).toThrow('preserves signed');
    expect(() => orderlyTransport('mode', options(provider(), { tealstreetWsRoute: true }), base)).toThrow(
      'WebSocket routing',
    );
    const wsRouting = () => ({
      ...directTransport.wsRouting('mode', {}),
      wsPrivate: { tunnelled: true, host: 'wss://relay.test' },
    });
    expect(() => orderlyTransport('mode', options(provider({ wsRouting })), base)).toThrow('WebSocket routing');
    expect(base.request).not.toHaveBeenCalled();
    expect(base.connect).not.toHaveBeenCalled();
  });

  it('keeps direct sockets/timers bound to the supplied base and aborts delayed authorization before a write', async () => {
    const { base } = createBase();
    const controller = new AbortController();
    const transport = orderlyTransport(
      'mode',
      options(
        provider({
          rest: () => ({
            baseURL: 'https://proxy.test',
            proxied: true,
            authorize: async () => {
              controller.abort();
            },
          }),
        }),
      ),
      base,
    );
    const handlers = { onOpen: vi.fn(), onMessage: vi.fn(), onClose: vi.fn() };
    transport.connect('wss://ws-evm.orderly.org/ws/stream/public-id', handlers);
    expect(base.connect).toHaveBeenCalledExactlyOnceWith('wss://ws-evm.orderly.org/ws/stream/public-id', handlers);
    expect(transport.now()).toBe(42);
    expect(transport.setTimeout(() => {}, 1)).toBe(123);
    transport.clearTimeout(123);
    expect(base.clearTimeout).toHaveBeenCalledExactlyOnceWith(123);
    await expect(
      transport.request({ ...signed(), signal: controller.signal } as Parameters<EngineHostTransport['request']>[0]),
    ).rejects.toThrow();
    expect(base.request).not.toHaveBeenCalled();
  });

  it('applies provider upgrade headers and refuses a relay or an unsupported browser upgrade', () => {
    const { base } = createBase();
    const url = 'wss://ws-evm.orderly.org/ws/stream/common';
    const handlers = { onOpen: vi.fn(), onMessage: vi.fn(), onClose: vi.fn() };
    const headers = provider({
      wsConnection: () => ({ kind: 'headers', headers: { Origin: 'https://provider.test' } }),
    });
    orderlyTransport('mode', options(headers), base).connect(url, handlers);
    expect(base.connect).toHaveBeenCalledExactlyOnceWith(url, {
      ...handlers,
      headers: { Origin: 'https://provider.test' },
    });
    base.connect.mockClear();
    expect(() =>
      orderlyTransport('mode', options(headers), { ...base, webSocketHeaders: false }).connect(url, handlers),
    ).toThrow('upgrade headers');
    const relay = provider({ wsConnection: () => ({ kind: 'relay', headers: {} }) });
    expect(() => orderlyTransport('mode', options(relay), base).connect(url, handlers)).toThrow('relay');
    expect(() =>
      orderlyTransport('mode', options(headers), base).connect(url, {
        ...handlers,
        headers: { origin: 'https://original.test' },
      }),
    ).toThrow('changed an existing');
    expect(base.connect).not.toHaveBeenCalled();
  });
});
