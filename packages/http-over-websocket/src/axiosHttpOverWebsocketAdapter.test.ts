// Pins the load-bearing contract that the HOW adapter's resolved response
// (and rejected error) carries the caller's original axios `config`. Without
// it, exchange interceptors that read request-scoped flags like
// `__suppressExchangeError` from `response.config` / `error.config` see
// `undefined` and per-call opt-outs silently no-op. The tunnel frame itself
// carries only `{data, status, headers}`, so the spread `{ ...data, config }`
// (and the direct `error.config = config` on the reject path) is what every
// built-in axios adapter does and what we have to reproduce here.

import Emitter from 'tiny-emitter';
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach } from 'vitest';

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
    sendToken = vi.fn();
    connectAndSubscribe = vi.fn();
  }

  return { HOWWebSocket: MockHOWWebSocket };
});

import { configureHOW } from './config';
import { createAxiosHttpOverWebsocketAdapter, howWebSocketJA, howWebSocketSG } from './axiosHttpOverWebsocketAdapter';

const mockWs = howWebSocketSG as unknown as {
  emitter: Emitter.TinyEmitter;
  sentMessages: any[];
  replyTo: (sent: any) => void;
};
const mockWsJA = howWebSocketJA as unknown as {
  emitter: Emitter.TinyEmitter;
  sentMessages: any[];
  replyTo: (sent: any) => void;
};

describe('axiosHttpOverWebsocketAdapter — config preservation invariant', () => {
  beforeAll(() => {
    configureHOW({
      getProxyToken: async () => 'fake-token',
      isTokenExpired: () => false,
    });
  });

  beforeEach(() => {
    mockWs.replyTo = () => {};
    mockWs.sentMessages.length = 0;
    mockWsJA.replyTo = () => {};
    mockWsJA.sentMessages.length = 0;
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
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
      baseURL: 'https://www.bitmex.com',
      headers: { Authorization: 'Bearer secret-token' },
      params: { symbol: 'XBTUSD', signature: 'param-signature' },
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
      url: { origin: 'https://www.bitmex.com', pathname: '/api/v1/order', queryKeys: ['apiKey'] },
      params: { symbol: 'XBTUSD', signature: '[REDACTED]' },
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
