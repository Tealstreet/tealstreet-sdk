// Pins the load-bearing contract that the HOW adapter's resolved response
// (and rejected error) carries the caller's original axios `config`. Without
// it, exchange interceptors that read request-scoped flags like
// `__suppressExchangeError` from `response.config` / `error.config` see
// `undefined` and per-call opt-outs silently no-op. The tunnel frame itself
// carries only `{data, status, headers}`, so the spread `{ ...data, config }`
// (and the direct `error.config = config` on the reject path) is what every
// built-in axios adapter does and what we have to reproduce here.

import Emitter from 'tiny-emitter';
import { describe, it, expect, vi, beforeAll, beforeEach } from 'vitest';

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
import { createAxiosHttpOverWebsocketAdapter, howWebSocketSG } from './axiosHttpOverWebsocketAdapter';

const mockWs = howWebSocketSG as unknown as {
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
});
