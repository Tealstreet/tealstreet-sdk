// Pins the load-bearing contract that the HOW adapter's resolved response
// (and rejected error) carries the caller's original axios `config`. Without
// it, exchange interceptors that read request-scoped flags like
// `__suppressExchangeError` from `response.config` / `error.config` see
// `undefined` and per-call opt-outs silently no-op. The tunnel frame itself
// carries only `{data, status, headers}`, so the spread `{ ...data, config }`
// (and the direct `error.config = config` on the reject path) is what every
// built-in axios adapter does and what we have to reproduce here.

import Emitter from 'tiny-emitter';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

import { bigIntAsStringJsonParse } from '@tealstreet/json-parse/src/jsonParse';

// MUST mock before importing the adapter — the adapter constructs
// `howWebSocketSG` / `howWebSocketJA` singletons at module load, so the
// mock class has to be in place by then.
vi.mock('./HOWWebSocket', () => {
  class MockHOWWebSocket {
    isOpen = true;
    isInitialized = true;
    isTimeSynced = true;
    isTokenSent = true;
    authState = 'Good';
    epoch = 0;
    tunnelLatency: number | undefined = undefined;

    emitter: Emitter.TinyEmitter = new (Emitter as any)();
    sentMessages: any[] = [];
    replyTo: (sent: any) => void = () => {};

    constructor(_url: string) {}

    once = (event: string, listener: (...args: any[]) => void) => {
      this.emitter.once(event, listener);
      return this;
    };
    off = (event: string, listener?: (...args: any[]) => void) => {
      this.emitter.off(event, listener as any);
      return this;
    };
    send = (data: any) => {
      this.sentMessages.push(data);
      // Microtask so the awaiting Promise sees the listener registered first.
      queueMicrotask(() => this.replyTo(data));
    };
    sendPrepared = (message: string) => this.send(JSON.parse(message));
    sendToken = vi.fn();
    connectAndSubscribe = vi.fn();
  }

  return { HOWWebSocket: MockHOWWebSocket };
});

import { configureHOW } from './config';
import {
  TRADE_TUNNEL_RPC_VERSION,
  createAxiosHttpOverWebsocketAdapter,
  howWebSocketJA,
  howWebSocketSG,
} from './axiosHttpOverWebsocketAdapter';

const mockWs = howWebSocketSG as unknown as {
  authState: string;
  epoch: number;
  isOpen: boolean;
  isTimeSynced: boolean;
  isTokenSent: boolean;
  emitter: Emitter.TinyEmitter;
  sentMessages: any[];
  sendToken: ReturnType<typeof vi.fn>;
  replyTo: (sent: any) => void;
};
const mockWsJA = howWebSocketJA as unknown as {
  emitter: Emitter.TinyEmitter;
  sentMessages: any[];
  replyTo: (sent: any) => void;
};

describe('axiosHttpOverWebsocketAdapter — config preservation invariant', () => {
  beforeEach(() => {
    configureHOW({
      getProxyToken: async () => 'fake-token',
      isTokenExpired: () => false,
    });
    mockWs.isTokenSent = true;
    mockWs.isOpen = true;
    mockWs.isTimeSynced = true;
    mockWs.authState = 'Good';
    mockWs.epoch = 0;
    mockWs.replyTo = () => {};
    mockWs.sentMessages.length = 0;
    mockWsJA.replyTo = () => {};
    mockWsJA.sentMessages.length = 0;
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
    mockWs.sendToken.mockReset();
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  it('prepares and binds without HTTP, then hands exact signed bytes off synchronously once', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter({ customParser: (body) => body });
    const controller = new AbortController();
    const initial: any = {
      url: 'https://example.com/private',
      baseURL: 'https://example.com',
      method: 'POST',
      headers: {},
      signal: controller.signal,
      timeout: 1000,
    };
    const prepared = await adapter.prepare(initial);
    expect(mockWs.sentMessages).toHaveLength(0);
    const final = {
      ...initial,
      url: 'https://example.com/private?stamp=123&signature=exact',
      headers: { 'X-Test': 'exact' },
      data: '{"value":"001.20"}',
    };
    const bound = prepared.bind(final);
    expect(bound.ready()).toBe(true);
    expect(mockWs.sentMessages).toHaveLength(0);
    mockWs.replyTo = (sent) => mockWs.emitter.emit('sub:' + sent.nonce, { data_raw: '[]', status: 200, headers: {} });
    const response = bound.dispatch();
    expect(mockWs.sentMessages).toHaveLength(1);
    expect(mockWs.sentMessages[0].arg).toEqual({
      headers: { 'X-Test': 'exact' },
      method: 'POST',
      url: final.url,
      params: {},
      data: final.data,
    });
    expect(() => bound.dispatch()).toThrow();
    await expect(response).resolves.toMatchObject({ data: '[]', config: final });
    expect(mockWs.sentMessages).toHaveLength(1);
    adapter.dispose();
  });

  it('managed default adapters preserve large integer and decimal request and response bytes', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const config: any = {
      url: 'https://example.com/read?x=%2B+%20',
      method: 'POST',
      headers: {},
      data: ' {"id":18446744073709551615,"n":1.000}\n',
    };
    const prepared = await adapter.prepare(config);
    mockWs.replyTo = (sent) =>
      mockWs.emitter.emit('sub:' + sent.nonce, { data_raw: config.data, status: 200, headers: {} });
    const response = await prepared.bind(config).dispatch();
    expect(mockWs.sentMessages[0].arg.data).toBe(config.data);
    expect(response.data).toBe(config.data);
    expect(mockWs.sentMessages[0]).toMatchObject({ format: 'raw', accept_encoding: ['gzip'], accept_binary: true });
    adapter.dispose();
  });

  it('invalidates a prepared tunnel across authentication/connection epochs without sending', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const config: any = { url: 'https://example.com/read', method: 'GET', headers: {} };
    const first = await adapter.prepare(config);
    mockWs.epoch++;
    expect(() => first.bind(config)).toThrow(expect.objectContaining({ code: 'HTTP_PREPARATION_STALE' }));
    const second = await adapter.prepare(config);
    const bound = second.bind(config);
    mockWs.epoch++;
    expect(() => bound.ready()).toThrow(expect.objectContaining({ code: 'HTTP_PREPARATION_STALE' }));
    expect(() => bound.dispatch()).toThrow(expect.objectContaining({ code: 'HTTP_PREPARATION_STALE' }));
    expect(mockWs.sentMessages).toHaveLength(0);
    first.cancel();
    second.cancel();
    adapter.dispose();
  });

  it('prepared cancellation removes only its response listener and leaves a shared peer usable', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const config: any = { url: 'https://example.com/cancel', method: 'GET', headers: {} };
    const prepared = await adapter.prepare(config);
    const bound = prepared.bind(config);
    const response = bound.dispatch();
    const rejected = expect(response).rejects.toMatchObject({ name: 'TradeTunnelCanceledError' });
    const nonce = mockWs.sentMessages[0].nonce;
    prepared.cancel();
    await rejected;
    expect((mockWs.emitter as any).e?.['sub:' + nonce]).toBeUndefined();
    expect(mockWs.isOpen).toBe(true);
    mockWs.replyTo = (sent) =>
      mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
    await expect(adapter({ ...config, url: 'https://example.com/peer' })).resolves.toMatchObject({
      data: { ok: true },
    });
    adapter.dispose();
  });

  it('readiness and binding do not renew the original complete-request deadline', async () => {
    vi.useFakeTimers();
    const adapter = createAxiosHttpOverWebsocketAdapter();
    try {
      mockWs.isOpen = false;
      const config: any = { url: 'https://example.com/read', method: 'GET', headers: {}, timeout: 100 };
      const pending = adapter.prepare(config);
      await vi.advanceTimersByTimeAsync(74);
      mockWs.isOpen = true;
      await vi.advanceTimersByTimeAsync(1);
      const prepared = await pending;
      const bound = prepared.bind(config);
      expect(bound.ready()).toBe(true);
      await vi.advanceTimersByTimeAsync(25);
      expect(bound.ready()).toBe(false);
      expect(() => bound.dispatch()).toThrow(expect.objectContaining({ name: 'TradeTunnelTimeoutError' }));
      expect(mockWs.sentMessages).toHaveLength(0);
      prepared.cancel();
    } finally {
      adapter.dispose();
      vi.useRealTimers();
    }
  });

  it('attaches caller config (with custom keys + headers) to the resolved tunnel response', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const callerConfig: any = {
      url: '/api/path',
      method: 'GET',
      baseURL: 'https://example.com',
      headers: { 'X-Custom-Header': 'custom-value' },
      __suppressExchangeError: true,
    };

    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, {
        data: { ok: true },
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    };

    const response: any = await adapter(callerConfig);

    expect(response.data).toEqual({ ok: true });
    expect(response.status).toBe(200);
    expect(response.config).toBe(callerConfig);
    expect(response.config.__suppressExchangeError).toBe(true);
    expect(response.config.headers['X-Custom-Header']).toBe('custom-value');
  });

  it.each([
    ['Binance', '{"orderId":9007199254740993}', 'orderId', '9007199254740993'],
    ['OKX', '{"data":[{"ordId":1234567890123456789}]}', 'ordId', '1234567890123456789'],
    ['Bitrue', '{"orderId":2334151155877155281}', 'orderId', '2334151155877155281'],
  ])('uses a versioned raw-only response for %s large integers', async (_exchange, rawBody, key, expected) => {
    const adapter = createAxiosHttpOverWebsocketAdapter({ customParser: bigIntAsStringJsonParse });
    const callerConfig: any = {
      url: '/api/order',
      method: 'GET',
      baseURL: 'https://example.com',
      headers: {},
    };

    mockWs.replyTo = (sent) => {
      expect(sent).toMatchObject({
        type: 'rpc.http',
        rpc_version: TRADE_TUNNEL_RPC_VERSION,
        format: 'raw',
        accept_encoding: ['gzip'],
        accept_binary: true,
      });
      // New tunnel servers omit `data`; the adapter must reconstruct it from
      // the one transmitted body without losing integer precision.
      mockWs.emitter.emit('sub:' + sent.nonce, {
        data_raw: rawBody,
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    };

    const response: any = await adapter(callerConfig);
    const parsed = Array.isArray(response.data?.data) ? response.data.data[0] : response.data;
    expect(parsed[key]).toBe(expected);
  });

  it.each([
    [
      'Binance',
      'H4sIAAAAAAAEA6tWyi9KSS3yTFGysjQwMDe0tDQyNTE3MbC0NK4FAERmNdIcAAAA',
      'orderId',
      '9007199254740993',
    ],
    [
      'OKX',
      'H4sIAAAAAAAEA6tWSkksSVSyiq5Wyi9K8UxRsjI0MjYxNTO3sDSAs2pjawE+xw12KAAAAA==',
      'ordId',
      '1234567890123456789',
    ],
    [
      'Bitrue',
      'H4sIAAAAAAAEA6tWyi9KSS3yTFGyMjI2NjE0NTQ0NbUwNzc0NTWyMKwFAKiI3scfAAAA',
      'orderId',
      '2334151155877155281',
    ],
  ])('decodes a negotiated gzip response for %s large integers', async (_exchange, encodedBody, key, expected) => {
    const adapter = createAxiosHttpOverWebsocketAdapter({ customParser: bigIntAsStringJsonParse });
    const callerConfig: any = {
      url: '/api/order',
      method: 'GET',
      baseURL: 'https://example.com',
      headers: {},
    };

    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, {
        data_encoded: encodedBody,
        data_encoding: 'gzip+base64',
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    };

    const response: any = await adapter(callerConfig);
    const parsed = Array.isArray(response.data?.data) ? response.data.data[0] : response.data;
    expect(parsed[key]).toBe(expected);
  });

  it.each([
    [
      'Binance',
      'H4sIAAAAAAAEA6tWyi9KSS3yTFGysjQwMDe0tDQyNTE3MbC0NK4FAERmNdIcAAAA',
      'orderId',
      '9007199254740993',
    ],
    [
      'OKX',
      'H4sIAAAAAAAEA6tWSkksSVSyiq5Wyi9K8UxRsjI0MjYxNTO3sDSAs2pjawE+xw12KAAAAA==',
      'ordId',
      '1234567890123456789',
    ],
    [
      'Bitrue',
      'H4sIAAAAAAAEA6tWyi9KSS3yTFGyMjI2NjE0NTQ0NbUwNzc0NTWyMKwFAKiI3scfAAAA',
      'orderId',
      '2334151155877155281',
    ],
  ])('decodes a negotiated binary gzip response for %s large integers', async (_exchange, encodedBody, key, expected) => {
    const adapter = createAxiosHttpOverWebsocketAdapter({ customParser: bigIntAsStringJsonParse });
    const callerConfig: any = {
      url: '/api/order',
      method: 'GET',
      baseURL: 'https://example.com',
      headers: {},
    };

    mockWs.replyTo = (sent) => {
      const binary = Uint8Array.from(globalThis.atob(encodedBody), (character) => character.charCodeAt(0));
      mockWs.emitter.emit('sub:' + sent.nonce, {
        data_binary: binary.buffer,
        data_encoding: 'gzip',
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    };

    const response: any = await adapter(callerConfig);
    const parsed = Array.isArray(response.data?.data) ? response.data.data[0] : response.data;
    expect(parsed[key]).toBe(expected);
  });

  it('rejects a corrupt negotiated gzip response without waiting for the request timeout', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter({ customParser: bigIntAsStringJsonParse });
    const callerConfig: any = {
      url: '/api/order',
      method: 'GET',
      baseURL: 'https://example.com',
      headers: {},
    };

    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, {
        data_encoded: 'bm90LWd6aXA=',
        data_encoding: 'gzip+base64',
        status: 200,
        headers: {},
      });
    };

    await expect(adapter(callerConfig)).rejects.toMatchObject({
      name: 'TradeTunnelResponseDecodeError',
      config: callerConfig,
    });
  });

  it('keeps compatibility with legacy tunnel responses that include both data fields', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter({ customParser: bigIntAsStringJsonParse });
    const callerConfig: any = {
      url: '/api/order',
      method: 'GET',
      baseURL: 'https://example.com',
      headers: {},
    };

    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, {
        data: { orderId: 9007199254740992 },
        data_raw: '{"orderId":9007199254740993}',
        status: 200,
        headers: {},
      });
    };

    const response: any = await adapter(callerConfig);
    expect(response.data.orderId).toBe('9007199254740993');
  });

  it('attaches caller config to the rejected tunnel error', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const callerConfig: any = {
      url: '/api/path',
      method: 'POST',
      baseURL: 'https://example.com',
      headers: {},
      __suppressExchangeError: true,
    };

    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, { error: 'simulated tunnel failure' });
    };

    await expect(adapter(callerConfig)).rejects.toMatchObject({
      name: 'TradeTunnelError',
      config: callerConfig,
    });
  });

  it('rejects when asynchronous pre-send authentication fails', async () => {
    configureHOW({
      getProxyToken: async () => {
        throw new Error('proxy token unavailable');
      },
      isTokenExpired: () => false,
    });
    mockWs.isTokenSent = false;
    mockWs.authState = 'Unknown';

    const adapter = createAxiosHttpOverWebsocketAdapter();
    const callerConfig: any = {
      url: '/api/path',
      method: 'POST',
      baseURL: 'https://example.com',
      headers: {},
    };

    await expect(adapter(callerConfig)).rejects.toThrow('proxy token unavailable');
  });

  it('reads the token bound at request start after waiting for a cold socket', async () => {
    let context = 'account-a';
    configureHOW({
      getProxyToken: async () => context,
      bindProxyToken: () => {
        const bound = context;
        return async () => bound;
      },
      isTokenExpired: () => false,
    });
    mockWs.isOpen = false;
    mockWs.isTokenSent = false;
    mockWs.authState = 'Unknown';
    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
    };
    const adapter = createAxiosHttpOverWebsocketAdapter();
    try {
      const pending = adapter({ url: '/api/path', method: 'GET', baseURL: 'https://example.com', headers: {} } as any);
      // Another account's work runs while this request waits for the socket.
      context = 'account-b';
      await new Promise((resolve) => setTimeout(resolve, 30));
      mockWs.isOpen = true;
      await vi.waitFor(() => expect(mockWs.sendToken).toHaveBeenCalled());
      expect(mockWs.sendToken.mock.calls[0][0]).toMatchObject({ type: 'rpc.auth', token: 'account-a' });
      mockWs.authState = 'Good';
      await expect(pending).resolves.toMatchObject({ data: { ok: true } });
    } finally {
      configureHOW({ bindProxyToken: undefined });
      mockWs.isOpen = true;
      adapter.dispose();
    }
  });

  it('uses TradeTunnel IP auto-auth without fetching or sending a token', async () => {
    const getProxyToken = vi.fn(async () => {
      throw new Error('proxy token should not be requested');
    });
    configureHOW({ getProxyToken, isTokenExpired: () => false });
    mockWs.isTokenSent = false;
    mockWs.authState = 'Good';
    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
    };

    const adapter = createAxiosHttpOverWebsocketAdapter();
    await expect(
      adapter({
        url: '/api/path',
        method: 'GET',
        baseURL: 'https://example.com',
        headers: {},
      } as any),
    ).resolves.toMatchObject({ data: { ok: true } });

    expect(getProxyToken).not.toHaveBeenCalled();
    expect(mockWs.sendToken).not.toHaveBeenCalled();
  });

  it('request cancellation during token acquisition sends neither credentials nor a stale write', async () => {
    let resolveToken!: (token: string) => void;
    const token = new Promise<string>((resolve) => { resolveToken = resolve; });
    const getProxyToken = vi.fn(() => token);
    configureHOW({ getProxyToken, isTokenExpired: () => false });
    mockWs.authState = 'Unknown';
    mockWs.isTokenSent = false;
    const controller = new AbortController();
    const adapter = createAxiosHttpOverWebsocketAdapter();
    try {
      const callerConfig: any = { url: '/cancelled', method: 'POST', baseURL: 'https://example.com', headers: {}, signal: controller.signal };
      const pending = adapter(callerConfig);
      const canceled = expect(pending).rejects.toMatchObject({ name: 'TradeTunnelCanceledError', config: callerConfig });
      await vi.waitFor(() => expect(getProxyToken).toHaveBeenCalledOnce());
      controller.abort();
      await canceled;
      resolveToken('arrived-after-cancellation');
      await token;
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(mockWs.sendToken).not.toHaveBeenCalled();
      expect(mockWs.sentMessages).toEqual([]);
    } finally { adapter.dispose(); }
  });

  it('a request-specific abort leaves another request on the same adapter able to refresh shared authentication', async () => {
    vi.useFakeTimers();
    const adapter = createAxiosHttpOverWebsocketAdapter();
    try {
      const staleToken = new Promise<string>(() => {});
      const getProxyToken = vi.fn(() => {
        if (getProxyToken.mock.calls.length <= 2) return Promise.resolve('initial-token');
        if (getProxyToken.mock.calls.length === 3) return staleToken;
        return Promise.resolve('survivor-token');
      });
      configureHOW({ getProxyToken, isTokenExpired: () => false });
      mockWs.authState = 'Bad';
      mockWs.isTokenSent = true;
      mockWs.sendToken.mockImplementation(() => { mockWs.authState = 'Good'; });
      mockWs.replyTo = (sent) => {
        mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
      };
      const controller = new AbortController();
      const firstConfig: any = { url: '/cancelled', method: 'POST', baseURL: 'https://example.com', headers: {}, timeout: 5_000, signal: controller.signal };
      const secondConfig: any = { url: '/surviving', method: 'GET', baseURL: 'https://example.com', headers: {}, timeout: 5_000 };
      const first = adapter(firstConfig).catch((error) => error);
      const second = adapter(secondConfig);
      await vi.advanceTimersByTimeAsync(600);
      expect(getProxyToken).toHaveBeenCalledTimes(3);
      controller.abort();
      await expect(first).resolves.toMatchObject({ name: 'TradeTunnelCanceledError', config: firstConfig });
      await vi.advanceTimersByTimeAsync(600);
      await expect(second).resolves.toMatchObject({ data: { ok: true }, config: secondConfig });
      expect(mockWs.sendToken).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ token: 'survivor-token' }));
      expect(mockWs.sentMessages.filter((message) => message.type === 'rpc.http')).toHaveLength(1);
    } finally {
      adapter.dispose();
      vi.useRealTimers();
    }
  });

  it('cancels a response wait without closing a peer request on the same connection', async () => {
    const controller = new AbortController();
    const adapter = createAxiosHttpOverWebsocketAdapter();
    try {
      mockWs.replyTo = (sent) => {
        if (sent.arg.url.endsWith('/peer')) mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
      };
      const pending = adapter({ url: '/cancelled', method: 'GET', baseURL: 'https://example.com', headers: {}, signal: controller.signal } as any);
      const canceled = expect(pending).rejects.toMatchObject({ name: 'TradeTunnelCanceledError' });
      controller.abort();
      await canceled;
      await expect(adapter({ url: '/peer', method: 'GET', baseURL: 'https://example.com', headers: {} } as any)).resolves.toMatchObject({ data: { ok: true } });
    } finally { adapter.dispose(); }
  });

  it('coalesces concurrent Bad-auth refreshes into one token fetch and rpc.auth frame', async () => {
    vi.useFakeTimers();
    try {
      let resolveRefreshToken!: (token: string) => void;
      const refreshToken = new Promise<string>((resolve) => {
        resolveRefreshToken = resolve;
      });
      const getProxyToken = vi.fn(() => {
        if (getProxyToken.mock.calls.length <= 2) return Promise.resolve('initial-token');
        return refreshToken;
      });
      configureHOW({ getProxyToken, isTokenExpired: () => false });
      mockWs.authState = 'Bad';
      mockWs.isTokenSent = true;
      mockWs.sendToken.mockImplementation(() => {
        mockWs.authState = 'Good';
      });
      mockWs.replyTo = (sent) => {
        mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
      };

      const adapter = createAxiosHttpOverWebsocketAdapter();
      const first = adapter({
        url: '/first',
        method: 'GET',
        baseURL: 'https://example.com',
        headers: {},
        timeout: 5_000,
      } as any);
      const second = adapter({
        url: '/second',
        method: 'GET',
        baseURL: 'https://example.com',
        headers: {},
        timeout: 5_000,
      } as any);

      await vi.advanceTimersByTimeAsync(600);
      expect(getProxyToken).toHaveBeenCalledTimes(3);

      resolveRefreshToken('refreshed-token');
      await Promise.all([first, second]);

      expect(mockWs.sendToken).toHaveBeenCalledTimes(1);
      expect(mockWs.sendToken).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'rpc.auth', token: 'refreshed-token' }),
      );
      expect(mockWs.sentMessages.filter((message) => message.type === 'rpc.http')).toHaveLength(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it('keeps each joined Bad-auth request on its own typed timeout deadline', async () => {
    vi.useFakeTimers();
    try {
      const neverResolvingToken = new Promise<string>(() => {});
      const getProxyToken = vi.fn(() => {
        if (getProxyToken.mock.calls.length <= 2) return Promise.resolve('initial-token');
        if (getProxyToken.mock.calls.length === 3) return neverResolvingToken;
        return Promise.resolve('retry-token');
      });
      configureHOW({ getProxyToken, isTokenExpired: () => false });
      mockWs.authState = 'Bad';
      mockWs.isTokenSent = true;
      mockWs.sendToken.mockImplementation(() => {
        mockWs.authState = 'Good';
      });
      mockWs.replyTo = (sent) => {
        mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
      };

      const adapter = createAxiosHttpOverWebsocketAdapter();
      const shortConfig: any = {
        url: '/short',
        method: 'GET',
        baseURL: 'https://example.com',
        headers: {},
        timeout: 550,
      };
      const longConfig: any = {
        url: '/long',
        method: 'GET',
        baseURL: 'https://example.com',
        headers: {},
        timeout: 2_000,
      };
      const shortResult = adapter(shortConfig).catch((error) => error);
      const longResult = adapter(longConfig);

      await vi.advanceTimersByTimeAsync(600);
      await expect(shortResult).resolves.toMatchObject({
        name: 'TradeTunnelTimeoutError',
        config: shortConfig,
      });

      await vi.advanceTimersByTimeAsync(600);
      await expect(longResult).resolves.toMatchObject({ data: { ok: true }, config: longConfig });
      expect(mockWs.sendToken).toHaveBeenCalledTimes(1);
      expect(mockWs.sendToken).toHaveBeenCalledWith(expect.objectContaining({ token: 'retry-token' }));
    } finally {
      vi.useRealTimers();
    }
  });

  it('disposal of a shared Bad-auth refresh owner leaves another adapter free to reauthenticate', async () => {
    vi.useFakeTimers();
    const firstAdapter = createAxiosHttpOverWebsocketAdapter();
    const secondAdapter = createAxiosHttpOverWebsocketAdapter();
    try {
      let resolveStaleToken!: (token: string) => void;
      const staleToken = new Promise<string>((resolve) => { resolveStaleToken = resolve; });
      const getProxyToken = vi.fn(() => {
        if (getProxyToken.mock.calls.length <= 2) return Promise.resolve('initial-token');
        if (getProxyToken.mock.calls.length === 3) return staleToken;
        return Promise.resolve('survivor-token');
      });
      configureHOW({ getProxyToken, isTokenExpired: () => false });
      mockWs.authState = 'Bad';
      mockWs.isTokenSent = true;
      mockWs.sendToken.mockImplementation(() => { mockWs.authState = 'Good'; });
      mockWs.replyTo = (sent) => {
        mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
      };
      const config = (url: string): any => ({ url, method: 'GET', baseURL: 'https://example.com', headers: {}, timeout: 5_000 });
      const firstConfig = config('/disposed-owner');
      const secondConfig = config('/surviving-account');
      const first = firstAdapter(firstConfig).catch((error) => error);
      const second = secondAdapter(secondConfig);
      await vi.advanceTimersByTimeAsync(600);
      expect(getProxyToken).toHaveBeenCalledTimes(3);
      firstAdapter.dispose();
      await expect(first).resolves.toMatchObject({ name: 'TradeTunnelDisposedError', config: firstConfig });
      await vi.advanceTimersByTimeAsync(600);
      await expect(second).resolves.toMatchObject({ data: { ok: true }, config: secondConfig });
      expect(mockWs.sendToken).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ token: 'survivor-token' }));
      resolveStaleToken('arrived-after-disposal');
      await vi.advanceTimersByTimeAsync(0);
      expect(mockWs.sendToken).toHaveBeenCalledOnce();
      expect(mockWs.sentMessages.filter((message) => message.type === 'rpc.http')).toHaveLength(1);
    } finally {
      firstAdapter.dispose();
      secondAdapter.dispose();
      vi.useRealTimers();
    }
  });

  it('shares Bad-auth refresh failures and clears the single-flight for a retry', async () => {
    vi.useFakeTimers();
    try {
      const refreshFailure = new Error('shared token refresh failed');
      let rejectRefreshToken!: (error: Error) => void;
      const refreshToken = new Promise<string>((_resolve, reject) => {
        rejectRefreshToken = reject;
      });
      const getProxyToken = vi.fn(() => {
        if (getProxyToken.mock.calls.length <= 2) return Promise.resolve('initial-token');
        return refreshToken;
      });
      configureHOW({ getProxyToken, isTokenExpired: () => false });
      mockWs.authState = 'Bad';
      mockWs.isTokenSent = true;

      const adapter = createAxiosHttpOverWebsocketAdapter();
      const first = adapter({
        url: '/first',
        method: 'GET',
        baseURL: 'https://example.com',
        headers: {},
        timeout: 5_000,
      } as any);
      const second = adapter({
        url: '/second',
        method: 'GET',
        baseURL: 'https://example.com',
        headers: {},
        timeout: 5_000,
      } as any);
      const outcomes = Promise.allSettled([first, second]);

      await vi.advanceTimersByTimeAsync(600);
      expect(getProxyToken).toHaveBeenCalledTimes(3);
      rejectRefreshToken(refreshFailure);
      const failed = await outcomes;

      expect(failed).toEqual([
        expect.objectContaining({ status: 'rejected', reason: refreshFailure }),
        expect.objectContaining({ status: 'rejected', reason: refreshFailure }),
      ]);
      expect(mockWs.sendToken).not.toHaveBeenCalled();

      const retryGetProxyToken = vi.fn().mockResolvedValue('retry-token');
      configureHOW({ getProxyToken: retryGetProxyToken, isTokenExpired: () => false });
      mockWs.authState = 'Bad';
      mockWs.sendToken.mockImplementation(() => {
        mockWs.authState = 'Good';
      });
      mockWs.replyTo = (sent) => {
        mockWs.emitter.emit('sub:' + sent.nonce, { data: { ok: true }, status: 200, headers: {} });
      };

      const retry = adapter({
        url: '/retry',
        method: 'GET',
        baseURL: 'https://example.com',
        headers: {},
        timeout: 5_000,
      } as any);
      await vi.advanceTimersByTimeAsync(600);
      await expect(retry).resolves.toMatchObject({ data: { ok: true } });
      expect(mockWs.sendToken).toHaveBeenCalledTimes(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it('applies the caller timeout to the pre-send connection wait', async () => {
    mockWs.isOpen = false;

    const adapter = createAxiosHttpOverWebsocketAdapter();
    const callerConfig: any = {
      url: '/api/path',
      method: 'POST',
      baseURL: 'https://example.com',
      headers: {},
      timeout: 10,
    };

    await expect(adapter(callerConfig)).rejects.toMatchObject({
      name: 'TradeTunnelTimeoutError',
      config: callerConfig,
      method: 'POST',
      url: 'https://example.com/api/path',
    });
    expect(mockWs.sentMessages).toHaveLength(0);
  });

  it('applies the remaining caller timeout to the response wait', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const callerConfig: any = {
      url: '/api/path',
      method: 'GET',
      baseURL: 'https://example.com',
      headers: {},
      timeout: 10,
    };

    await expect(adapter(callerConfig)).rejects.toMatchObject({
      name: 'TradeTunnelTimeoutError',
      config: callerConfig,
    });
    expect(mockWs.sentMessages).toHaveLength(1);
  });

  it('preserves object-shaped tunnel error messages', async () => {
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const callerConfig: any = {
      url: '/api/path',
      method: 'POST',
      baseURL: 'https://example.com',
      headers: {},
    };

    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, {
        error: { message: 'This request has expired - `expires` is in the past.' },
      });
    };

    await expect(adapter(callerConfig)).rejects.toMatchObject({
      name: 'TradeTunnelError',
      message: 'This request has expired - `expires` is in the past. (URL: https://example.com/api/path)',
      originalError: { message: 'This request has expired - `expires` is in the past.' },
    });
  });

  it('emits sanitized request and response summaries only in DEBUG mode', async () => {
    vi.stubEnv('DEBUG', '1');
    const logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    const adapter = createAxiosHttpOverWebsocketAdapter({ region: 'JA' });
    const callerConfig: any = {
      url: '/api/v1/order?apiKey=query-key',
      method: 'POST',
      baseURL: 'https://api.bybit.com',
      headers: { Authorization: 'Bearer secret-token' },
      params: { symbol: 'BTCUSDT', signature: 'param-signature' },
      data: JSON.stringify({
        price: 100,
        apiKey: 'body-key',
        nested: { token: 'nested-token', visible: 'ok' },
      }),
    };

    mockWsJA.replyTo = (sent) => {
      mockWsJA.emitter.emit('sub:' + sent.nonce, {
        data: { ok: true, secret: 'response-secret' },
        status: 200,
        headers: {},
      });
    };

    await adapter(callerConfig);

    expect(logSpy).toHaveBeenCalledTimes(2);

    const requestLog = logSpy.mock.calls.find(([message]) => message === '[TradeTunnel] REQUEST:')?.[1] as any;
    const responseLog = logSpy.mock.calls.find(([message]) => message === '[TradeTunnel] RESPONSE:')?.[1] as any;

    expect(requestLog).toMatchObject({
      method: 'POST',
      route: { selectedRegion: 'JA', tunnelHost: 'tokyo', forcedRegion: 'JA', bypassProxy: false },
      url: { origin: 'https://api.bybit.com', pathname: '/api/v1/order', queryKeys: ['apiKey'] },
      params: { symbol: 'BTCUSDT', signature: '[REDACTED]' },
      data: { price: 100, apiKey: '[REDACTED]', nested: { token: '[REDACTED]', visible: 'ok' } },
    });
    expect(responseLog).toMatchObject({
      data: { ok: true, secret: '[REDACTED]' },
    });

    const serializedLogs = JSON.stringify(logSpy.mock.calls);
    expect(serializedLogs).not.toContain('query-key');
    expect(serializedLogs).not.toContain('param-signature');
    expect(serializedLogs).not.toContain('body-key');
    expect(serializedLogs).not.toContain('nested-token');
    expect(serializedLogs).not.toContain('response-secret');
    expect(serializedLogs).not.toContain('secret-token');
  });

  it('caps sanitizer edge cases in verbose TradeTunnel summaries', async () => {
    vi.stubEnv('DEBUG', '1');
    const logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const circularParams: Record<string, unknown> = { visible: 'ok' };
    circularParams.self = circularParams;
    const callerConfig: any = {
      url: '/api/path?secret=value',
      method: 'POST',
      baseURL: 'https://example.com',
      headers: {},
      params: circularParams,
      data: {
        long: 'x'.repeat(600),
        array: Array.from({ length: 25 }, (_, index) => index),
        deep: { a: { b: { c: { d: { e: 'hidden' } } } } },
        manyKeys: Object.fromEntries(Array.from({ length: 55 }, (_, index) => [`key${index}`, index])),
      },
    };

    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, {
        data: { ok: true },
        status: 200,
        headers: {},
      });
    };

    await adapter(callerConfig);

    const requestLog = logSpy.mock.calls.find(([message]) => message === '[TradeTunnel] REQUEST:')?.[1] as any;
    expect(requestLog.params.self).toBe('[CIRCULAR]');
    expect(requestLog.data.long).toContain('[TRUNCATED]');
    expect(requestLog.data.array).toHaveLength(21);
    expect(requestLog.data.array[20]).toBe('[TRUNCATED] 5 items');
    expect(requestLog.data.deep.a.b.c).toBe('[TRUNCATED]');
    expect(requestLog.data.manyKeys.__truncatedKeys).toBe(5);
    expect(JSON.stringify(logSpy.mock.calls)).not.toContain('secret=value');
  });

  it('does not emit trade tunnel summaries when verbose logging is off', async () => {
    const debugSpy = vi.spyOn(console, 'debug').mockImplementation(() => {});
    const logSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
    const adapter = createAxiosHttpOverWebsocketAdapter();
    const callerConfig: any = {
      url: '/api/path',
      method: 'GET',
      baseURL: 'https://example.com',
      headers: {},
    };

    mockWs.replyTo = (sent) => {
      mockWs.emitter.emit('sub:' + sent.nonce, {
        data: { ok: true },
        status: 200,
        headers: {},
      });
    };

    await adapter(callerConfig);

    expect(debugSpy).not.toHaveBeenCalled();
    expect(logSpy).not.toHaveBeenCalledWith(expect.stringContaining('[TradeTunnel]'), expect.anything());
  });
});
