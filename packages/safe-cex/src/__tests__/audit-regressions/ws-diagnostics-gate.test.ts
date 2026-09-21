import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BaseWebSocket, WsDiagnosticReceiveOptions, WsDiagnosticSendOptions } from '../../exchanges/base.ws';
import { setWsDiagnosticsEnabled } from '../../utils/wsDiagnostics';

type EmittedFrame = { event: string; message: string };

class TestWebSocket extends BaseWebSocket<any> {
  connectAndSubscribe = () => {};
  onOpen = () => {};
  onMessage = () => {};
  ping = () => {};

  emitReceiveForTest(data: unknown, options?: WsDiagnosticReceiveOptions): void {
    this.emitWsDiagnosticReceive(data, options);
  }

  emitSendForTest(data: string, options?: WsDiagnosticSendOptions): void {
    this.emitWsDiagnosticSend(data, options);
  }
}

const createSocket = () => {
  const frames: EmittedFrame[] = [];
  const socket = new TestWebSocket({
    name: 'test',
    isDisposed: false,
    emitter: {
      emit: (event: string, message: string) => {
        frames.push({ event, message });
      },
    },
    store: {},
    log: vi.fn(),
    emitAndLogError: vi.fn(),
    emitWsReconnected: vi.fn(),
  });
  return { frames, socket };
};

describe('WebSocket diagnostics gate', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(0);
    setWsDiagnosticsEnabled(false);
  });

  afterEach(() => {
    setWsDiagnosticsEnabled(false);
    vi.useRealTimers();
  });

  it('does not emit when the runtime gate is off', () => {
    const { frames, socket } = createSocket();

    socket.emitReceiveForTest(JSON.stringify({ channel: 'orders' }));
    socket.emitSendForTest(JSON.stringify({ op: 'subscribe', args: ['orders'] }));

    expect(frames).toEqual([]);
  });

  it('emits receive and send frames when the runtime gate is on', () => {
    const { frames, socket } = createSocket();
    setWsDiagnosticsEnabled(true);

    socket.emitReceiveForTest(JSON.stringify({ channel: 'orders' }));
    socket.emitSendForTest(JSON.stringify({ op: 'subscribe', args: ['orders'] }));

    expect(frames).toEqual([
      { event: 'grafana-pws', message: '{"channel":"orders"}' },
      { event: 'grafana-pws-sub', message: '{"op":"subscribe","args":["orders"]}' },
    ]);
  });

  it('suppresses heartbeat frames by default', () => {
    const { frames, socket } = createSocket();
    setWsDiagnosticsEnabled(true);

    socket.emitReceiveForTest('pong');
    socket.emitSendForTest(JSON.stringify({ op: 'ping' }));
    socket.emitReceiveForTest(JSON.stringify({ pong: 123 }));

    expect(frames).toEqual([]);
  });

  it('supports receive filters and formatter callbacks', () => {
    const { frames, socket } = createSocket();
    setWsDiagnosticsEnabled(true);

    socket.emitReceiveForTest(JSON.stringify({ channel: 'ticker' }), {
      allowChannel: (json) => (json as any)?.channel !== 'ticker',
      format: (data) => `[test] ${String(data)}`,
    });
    socket.emitReceiveForTest(JSON.stringify({ channel: 'orderUpdates' }), {
      allowChannel: (json) => (json as any)?.channel === 'orderUpdates',
      format: (data) => `[test] ${String(data)}`,
    });

    expect(frames).toEqual([{ event: 'grafana-pws', message: '[test] {"channel":"orderUpdates"}' }]);
  });

  it('caps long payloads and reports rate-limit suppression', () => {
    const { frames, socket } = createSocket();
    setWsDiagnosticsEnabled(true);

    socket.emitReceiveForTest('x'.repeat(5000));
    expect(frames[0].message).toContain('...truncated (5000 chars)');

    for (let i = 0; i < 205; i++) {
      socket.emitReceiveForTest(JSON.stringify({ channel: 'orders', i }));
    }
    expect(frames.filter((frame) => frame.event === 'grafana-pws')).toHaveLength(200);

    vi.setSystemTime(1000);
    socket.emitReceiveForTest(JSON.stringify({ channel: 'orders', recovered: true }));

    expect(frames.at(-2)?.message).toContain('suppressed 6 websocket diagnostic frame(s)');
    expect(frames.at(-1)?.message).toBe('{"channel":"orders","recovered":true}');
  });

  it.each([
    [
      'bybit auth args',
      { op: 'auth', args: ['bybit-key', 1234567890, '0123456789abcdef0123456789abcdef01234567'] },
      ['bybit-key', '0123456789abcdef0123456789abcdef01234567'],
    ],
    [
      'bitmex auth args',
      { op: 'authKeyExpires', args: ['bitmex-key', 1234567890, 'abcdefabcdefabcdefabcdefabcdefabcdefabcd'] },
      ['bitmex-key', 'abcdefabcdefabcdefabcdefabcdefabcdefabcd'],
    ],
    [
      'okx login args',
      { op: 'login', args: [{ apiKey: 'okx-key', passphrase: 'okx-pass', sign: 'okx-sign' }] },
      ['okx-key', 'okx-pass', 'okx-sign'],
    ],
    [
      'binance signature params',
      {
        id: 'tealstreet-user-data',
        method: 'userDataStream.subscribe.signature',
        params: { apiKey: 'binance-key', timestamp: 123, signature: 'binance-signature' },
      },
      ['binance-key', 'binance-signature'],
    ],
    ['coinbase jwt', { type: 'subscribe', channel: 'user', jwt: 'coinbase-jwt-token' }, ['coinbase-jwt-token']],
    [
      'mexc login params',
      { method: 'login', param: { apiKey: 'mexc-key', reqTime: '123', signature: 'mexc-signature' } },
      ['mexc-key', 'mexc-signature'],
    ],
    [
      'query string signature',
      'apiKey=query-key&timestamp=123&signature=abcdefabcdefabcdefabcdefabcdefabcdefabcd',
      ['query-key', 'abcdefabcdefabcdefabcdefabcdefabcdefabcd'],
    ],
  ])('redacts %s credentials from outbound diagnostics', (_name, payload, forbidden) => {
    const { frames, socket } = createSocket();
    setWsDiagnosticsEnabled(true);

    socket.emitSendForTest(typeof payload === 'string' ? payload : JSON.stringify(payload));

    expect(frames).toHaveLength(1);
    for (const secret of forbidden) {
      expect(frames[0].message).not.toContain(secret);
    }
    expect(frames[0].message).toContain('[redacted]');
  });
});

