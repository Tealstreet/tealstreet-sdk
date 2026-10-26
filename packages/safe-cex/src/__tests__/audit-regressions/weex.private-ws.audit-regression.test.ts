import { createRequire } from 'node:module';

import { describe, expect, it, vi } from 'vitest';
import { WebSocketServer } from 'ws';

import { ReconnectingWebSocket } from '../../../../../thirdparty/reconnecting-websocket/src/ReconnectingWebsocket';
import { unitTransportGuard } from '../../../config/test/transportGuard';
import { getExchangeMetadata } from '../../exchangeMetadata';
import { BaseWebSocket } from '../../exchanges/base.ws';
import { signWeexMessage } from '../reference/weex/weex.api';
import { WeexExchange } from '../reference/weex/weex.exchange';
import { BASE_WS_URL } from '../reference/weex/weex.types';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

class HeaderCapturingWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  static instances: HeaderCapturingWebSocket[] = [];

  readyState = HeaderCapturingWebSocket.OPEN;
  private listeners = new Map<string, Set<(event: any) => void>>();

  constructor(
    readonly url: string,
    readonly protocols: string[],
    readonly options: { headers?: Record<string, string> },
  ) {
    HeaderCapturingWebSocket.instances.push(this);
  }

  addEventListener(type: string, listener: (event: any) => void) {
    const listeners = this.listeners.get(type) || new Set();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: (event: any) => void) {
    this.listeners.get(type)?.delete(listener);
  }

  send = vi.fn();

  close(code = 1006, reason = '') {
    this.readyState = HeaderCapturingWebSocket.CLOSED;
    this.emit('close', { code, reason });
  }

  emit(type: string, event: any) {
    for (const listener of this.listeners.get(type) || []) listener(event);
  }
}

class BrowserWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  static instances: BrowserWebSocket[] = [];

  readyState = BrowserWebSocket.OPEN;
  onerror: ((event: Event) => unknown) | null = null;
  private listeners = new Map<string, Set<(event: any) => void>>();

  constructor(
    readonly url: string,
    readonly protocols?: string | string[],
  ) {
    BrowserWebSocket.instances.push(this);
  }

  addEventListener(type: string, listener: (event: any) => void) {
    const listeners = this.listeners.get(type) || new Set();
    listeners.add(listener);
    this.listeners.set(type, listeners);
  }

  removeEventListener(type: string, listener: (event: any) => void) {
    this.listeners.get(type)?.delete(listener);
  }

  send = vi.fn();

  close() {
    this.readyState = BrowserWebSocket.CLOSED;
  }
}

const createExchange = () =>
  new WeexExchange(
    'weex',
    { key: 'test-access-key', secret: 'test-secret', passphrase: 'test-passphrase' } as any,
    new DefaultStore(),
  );

const nodeRequire = createRequire(import.meta.url);
const stringHeader = (value: string | string[] | undefined): string | undefined =>
  typeof value === 'string' ? value : undefined;

describe('audit regression: WEEX private websocket authentication', () => {
  it('regenerates signed private headers for each reconnecting Node transport attempt', async () => {
    vi.useFakeTimers();
    HeaderCapturingWebSocket.instances = [];
    let attempt = 0;

    try {
      const socket = new ReconnectingWebSocket('wss://ws-contract.weex.com/v3/ws/private', undefined, {
        WebSocket: HeaderCapturingWebSocket,
        headers: () => ({
          'ACCESS-TIMESTAMP': `${++attempt}`,
          'ACCESS-SIGN': `signature-${attempt}`,
        }),
        minReconnectionDelay: 0,
        maxReconnectionDelay: 0,
        minUptime: 0,
        connectionTimeout: 1_000,
      });

      await vi.advanceTimersByTimeAsync(0);
      expect(HeaderCapturingWebSocket.instances).toHaveLength(1);
      expect(HeaderCapturingWebSocket.instances[0].options.headers).toEqual({
        'ACCESS-TIMESTAMP': '1',
        'ACCESS-SIGN': 'signature-1',
      });

      HeaderCapturingWebSocket.instances[0].close();
      await vi.advanceTimersByTimeAsync(0);
      expect(HeaderCapturingWebSocket.instances).toHaveLength(2);
      expect(HeaderCapturingWebSocket.instances[1].options.headers).toEqual({
        'ACCESS-TIMESTAMP': '2',
        'ACCESS-SIGN': 'signature-2',
      });

      socket.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it('preserves static headers for header-capable WebSocket constructors', async () => {
    vi.useFakeTimers();
    HeaderCapturingWebSocket.instances = [];

    try {
      const socket = new ReconnectingWebSocket('wss://ws-contract.weex.com/v3/ws/private', undefined, {
        WebSocket: HeaderCapturingWebSocket,
        headers: { 'X-Static-Header': 'static-value' },
        minReconnectionDelay: 0,
        maxReconnectionDelay: 0,
        minUptime: 0,
        connectionTimeout: 1_000,
      });

      await vi.advanceTimersByTimeAsync(0);
      expect(HeaderCapturingWebSocket.instances[0].options.headers).toEqual({ 'X-Static-Header': 'static-value' });
      socket.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it('reports a throwing fresh-header provider without credential text, then retries through the normal delay', async () => {
    vi.useFakeTimers();
    HeaderCapturingWebSocket.instances = [];
    const errors: string[] = [];
    let providerCalls = 0;

    try {
      const socket = new ReconnectingWebSocket('wss://ws-contract.weex.com/v3/ws/private', undefined, {
        WebSocket: HeaderCapturingWebSocket,
        headers: () => {
          providerCalls++;
          if (providerCalls === 1) throw new Error('test-secret-must-not-leak');
          return { 'ACCESS-TIMESTAMP': `${providerCalls}`, 'ACCESS-SIGN': `signature-${providerCalls}` };
        },
        minReconnectionDelay: 10,
        maxReconnectionDelay: 10,
        minUptime: 0,
        connectionTimeout: 1_000,
      });
      socket.addEventListener('error', (event) => {
        errors.push(event.message);
      });

      await vi.advanceTimersByTimeAsync(0);
      await vi.advanceTimersByTimeAsync(10);

      // This kills the mutation that clears the lock but omits the retry.
      expect(providerCalls).toBe(2);
      expect(HeaderCapturingWebSocket.instances).toHaveLength(1);
      expect(errors).toEqual(['WebSocket header provider failed']);
      expect(JSON.stringify(errors)).not.toContain('test-secret-must-not-leak');

      socket.close();
    } finally {
      vi.useRealTimers();
    }
  });

  it('builds all required WEEX private headers from a fresh timestamp without logging credentials', () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket;
    const createWebsocket = vi.fn().mockReturnValue({ addEventListener: vi.fn() });
    vi.spyOn(websocket, 'createWebsocket').mockImplementation(createWebsocket as any);
    vi.spyOn(virtualClock, 'getCurrentTimeMs')
      .mockReturnValueOnce(1_700_000_000_001)
      .mockReturnValueOnce(1_700_000_000_002);

    websocket.connectAndSubscribe();
    const headerProvider = createWebsocket.mock.calls[0][2].headers as () => Record<string, string>;
    const first = headerProvider();
    const second = headerProvider();

    expect(first).toEqual({
      'User-Agent': 'Tealstreet',
      'ACCESS-KEY': 'test-access-key',
      'ACCESS-PASSPHRASE': 'test-passphrase',
      'ACCESS-TIMESTAMP': '1700000000001',
      'ACCESS-SIGN': signWeexMessage('test-secret', '1700000000001/v3/ws/private'),
    });
    expect(second['ACCESS-TIMESTAMP']).toBe('1700000000002');
    expect(second['ACCESS-SIGN']).toBe(signWeexMessage('test-secret', '1700000000002/v3/ws/private'));
    expect(JSON.stringify(createWebsocket.mock.calls)).not.toContain('test-secret');
  });

  it('fails closed in browser runtimes, keeps the REST safety cadence, and never leaks credentials', () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket;
    const createWebsocket = vi.spyOn(websocket, 'createWebsocket');
    const log = vi.spyOn(exchange, 'log');
    const originalProcess = globalThis.process;

    vi.stubGlobal('process', undefined);
    try {
      websocket.connectAndSubscribe();
    } finally {
      vi.stubGlobal('process', originalProcess);
    }

    const logs = JSON.stringify(log.mock.calls);
    expect(createWebsocket).not.toHaveBeenCalled();
    expect(getExchangeMetadata('weex').tickInterval).toBe(2_000);
    expect(logs).toContain('WEEX private WebSocket disabled');
    expect(logs).not.toContain('test-access-key');
    expect(logs).not.toContain('test-secret');
    expect(logs).not.toContain('test-passphrase');
  });

  it('fails closed through BaseWebSocket dispatch for a Node-marked renderer without ws', () => {
    const exchange = createExchange();
    const websocket = exchange.privateWebsocket;
    const log = vi.spyOn(exchange, 'log');
    const Module = nodeRequire('module') as { _load: (...args: any[]) => unknown };
    const originalLoad = Module._load;
    Module._load = (request: string, ...args: any[]) => {
      if (request === 'ws') throw new Error('ws is unavailable in this renderer');
      return originalLoad(request, ...args);
    };
    try {
      websocket.connectAndSubscribe();
    } finally {
      Module._load = originalLoad;
    }

    // This kills the mutation that lets a Node-marked renderer use native WebSocket.
    const logs = JSON.stringify(log.mock.calls);
    expect(websocket.ws).toBeUndefined();
    expect(getExchangeMetadata('weex').tickInterval).toBe(2_000);
    expect(logs).toContain('header-capable Node/Bun WebSocket transport is unavailable');
    expect(logs).not.toContain('test-access-key');
    expect(logs).not.toContain('test-secret');
    expect(logs).not.toContain('test-passphrase');
  });

  it("prefers Node 22's synchronous ESM-capable resolver for required headers", () => {
    const exchange = createExchange();
    const getBuiltinModule = vi.spyOn(process, 'getBuiltinModule');
    const websocket = new BaseWebSocket(exchange);

    const socket = websocket.createWebsocket('wss://ws-contract.weex.com/v3/ws/private', undefined, {
      headers: { 'ACCESS-KEY': 'test-access-key' },
      headersRequired: true,
      startClosed: true,
    });

    expect(getBuiltinModule).toHaveBeenCalledWith('module');
    expect((socket as any)._options).toEqual(
      expect.objectContaining({ WebSocket: expect.any(Function), headers: { 'ACCESS-KEY': 'test-access-key' } }),
    );
    socket.close();
  });

  it('keeps static and implicit Tealstreet routing headers on the browser-native best-effort path', async () => {
    vi.useFakeTimers();
    BrowserWebSocket.instances = [];
    const exchange = createExchange();
    const websocket = new BaseWebSocket(exchange);
    const originalProcess = globalThis.process;
    const originalWebSocket = globalThis.WebSocket;
    vi.stubGlobal('process', undefined);
    vi.stubGlobal('WebSocket', BrowserWebSocket);

    try {
      const staticHeaderSocket = websocket.createWebsocket('wss://ws-contract.weex.com/v3/ws/public', undefined, {
        headers: { 'User-Agent': 'Tealstreet' },
      });
      const tealstreetSocket = websocket.createWebsocket('wss://fsjnpy143pbz.asia.tealstreet.io/weex/public');
      await vi.advanceTimersByTimeAsync(0);

      expect(staticHeaderSocket).toBeInstanceOf(BrowserWebSocket);
      expect(BrowserWebSocket.instances.map((socket) => socket.url)).toEqual([
        'wss://ws-contract.weex.com/v3/ws/public',
        'wss://fsjnpy143pbz.asia.tealstreet.io/weex/public',
      ]);
      tealstreetSocket.close();
    } finally {
      vi.stubGlobal('process', originalProcess);
      vi.stubGlobal('WebSocket', originalWebSocket);
      vi.clearAllTimers();
      vi.useRealTimers();
    }
  });

  it('keeps browser WEEX startup on public native WebSocket while private auth fails closed and the 2s REST loop runs', async () => {
    vi.useFakeTimers();
    BrowserWebSocket.instances = [];
    const exchange = createExchange();
    const log = vi.spyOn(exchange, 'log');
    const fetchBalanceAndPositions = vi
      .spyOn(exchange, 'fetchBalanceAndPositions')
      .mockResolvedValue({ balance: { used: 0, free: 0, total: 0, upnl: 0 }, positions: [] } as never);
    const fetchOrders = vi.spyOn(exchange, 'fetchOrders').mockResolvedValue([]);

    vi.spyOn(exchange, 'fetchMarkets').mockResolvedValue([]);
    vi.spyOn(exchange, 'fetchTickers').mockResolvedValue([]);
    vi.spyOn(exchange as any, 'fetchSymbolConfigs').mockResolvedValue(new Map());
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: { dualSidePosition: false } } as never);

    const originalProcess = globalThis.process;
    const originalWebSocket = globalThis.WebSocket;
    vi.stubGlobal('process', undefined);
    vi.stubGlobal('WebSocket', BrowserWebSocket);

    try {
      await exchange.start();

      expect(BrowserWebSocket.instances).toHaveLength(1);
      expect(BrowserWebSocket.instances[0].url).toBe('wss://ws-contract.weex.com/v3/ws/public');
      expect(exchange.privateWebsocket.ws).toBeUndefined();
      expect(JSON.stringify(log.mock.calls)).toContain('WEEX private WebSocket disabled');
      expect(getExchangeMetadata('weex').tickInterval).toBe(2_000);
      expect(fetchBalanceAndPositions).toHaveBeenCalledTimes(2);
      expect(fetchOrders).toHaveBeenCalledTimes(2);

      await vi.advanceTimersByTimeAsync(2_000);
      // In a visible browser document loop() schedules the next tick through
      // requestAnimationFrame after the two-second REST safety delay.
      await vi.advanceTimersByTimeAsync(20);
      expect(fetchBalanceAndPositions).toHaveBeenCalledTimes(3);
      expect(fetchOrders).toHaveBeenCalledTimes(3);
    } finally {
      exchange.dispose();
      vi.stubGlobal('process', originalProcess);
      vi.stubGlobal('WebSocket', originalWebSocket);
      vi.clearAllTimers();
      vi.useRealTimers();
    }
  });

  it('uses BaseWebSocket dispatch to send refreshed WEEX private headers on reconnect', async () => {
    const server = new WebSocketServer({ host: '127.0.0.1', port: 0 });
    let revokeLoopback: (() => void) | undefined;
    const previousPrivateUrl = BASE_WS_URL.private.livenet;
    const capturedHeaders: Array<Record<string, string | undefined>> = [];
    let resolveSecondConnection: (() => void) | undefined;
    const secondConnection = new Promise<void>((resolve) => {
      resolveSecondConnection = resolve;
    });

    try {
      await new Promise<void>((resolve, reject) => {
        server.once('listening', resolve);
        server.once('error', reject);
      });
      const address = server.address();
      if (!address || typeof address === 'string') throw new Error('WEEX test server did not bind a TCP port');
      revokeLoopback = unitTransportGuard.allowLoopbackPort(address.port);
      BASE_WS_URL.private.livenet = `ws://127.0.0.1:${address.port}/v3/ws/private`;

      server.on('connection', (connection, request) => {
        capturedHeaders.push({
          key: stringHeader(request.headers['access-key']),
          passphrase: stringHeader(request.headers['access-passphrase']),
          timestamp: stringHeader(request.headers['access-timestamp']),
          signature: stringHeader(request.headers['access-sign']),
        });
        if (capturedHeaders.length === 1) {
          connection.close();
        } else {
          resolveSecondConnection?.();
        }
      });

      const exchange = createExchange();
      vi.spyOn(exchange, 'fetchAndSetPrivateSnapshot').mockResolvedValue(undefined as never);
      vi.spyOn(virtualClock, 'getCurrentTimeMs')
        .mockReturnValueOnce(1_700_000_000_001)
        .mockReturnValueOnce(1_700_000_000_002);
      exchange.privateWebsocket.connectAndSubscribe();
      expect((exchange.privateWebsocket.ws as any)._options).toEqual(
        expect.objectContaining({ headers: expect.any(Function), WebSocket: expect.any(Function) }),
      );

      let timeout: NodeJS.Timeout | undefined;
      await Promise.race([
        secondConnection,
        new Promise<void>((_, reject) => {
          timeout = setTimeout(() => reject(new Error('WEEX reconnect did not occur')), 8_000);
        }),
      ]);
      clearTimeout(timeout);

      // Real BaseWebSocket dispatch selected RWS + ws, and RWS refreshed the
      // header provider rather than reusing an expired private-auth timestamp.
      expect(capturedHeaders).toEqual([
        {
          key: 'test-access-key',
          passphrase: 'test-passphrase',
          timestamp: '1700000000001',
          signature: signWeexMessage('test-secret', '1700000000001/v3/ws/private'),
        },
        {
          key: 'test-access-key',
          passphrase: 'test-passphrase',
          timestamp: '1700000000002',
          signature: signWeexMessage('test-secret', '1700000000002/v3/ws/private'),
        },
      ]);
      // RWS.close() clears retry timers without calling ws.terminate() during
      // the server's close handshake; Base disposal can then clean timers.
      (exchange.privateWebsocket.ws as ReconnectingWebSocket).close();
      exchange.privateWebsocket.ws = undefined;
      exchange.privateWebsocket.dispose();
    } finally {
      revokeLoopback?.();
      BASE_WS_URL.private.livenet = previousPrivateUrl;
      server.clients.forEach((client) => client.terminate());
      await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
    }
  }, 10_000);

  it('answers the documented private type ping immediately and cleans up its heartbeat timer', () => {
    vi.useFakeTimers();
    try {
      const exchange = createExchange();
      const websocket = exchange.privateWebsocket;
      const frames: string[] = [];
      websocket.ws = { readyState: WebSocket.OPEN, send: (frame: string) => frames.push(frame) } as any;
      const ping = vi.spyOn(websocket, 'ping').mockImplementation(() => {});

      websocket.onMessage({ data: JSON.stringify({ type: 'ping', time: '1700000000000' }) } as MessageEvent);

      expect(frames).toEqual([JSON.stringify({ method: 'PONG', id: 1 })]);
      websocket.dispose();
      vi.runOnlyPendingTimers();
      expect(ping).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});
