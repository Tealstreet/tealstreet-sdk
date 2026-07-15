import type { BaseExchange } from '../../exchanges/base';

import {
  BaseWebSocket,
  getCloseCodeDescription,
  isHeartbeatFrame,
  WS_RECONNECT_THRESHOLD_MS,
} from '../../exchanges/base.ws';

const createParent = (isDisposed = false) =>
  ({
    isDisposed,
    emitter: { emit: vi.fn() },
    log: vi.fn(),
    name: 'contract-test',
    store: { update: vi.fn() },
  }) as unknown as BaseExchange;

describe('BaseWebSocket documented runtime contracts', () => {
  test('a disposed exchange cannot send or trigger socket resurrection', () => {
    const socket = new BaseWebSocket(createParent(true));
    const send = vi.fn();
    socket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
    const reconnect = vi.spyOn(socket, 'reconnect');

    socket.send('private-auth-frame');

    expect(send).not.toHaveBeenCalled();
    expect(reconnect).not.toHaveBeenCalled();
  });

  test('health requires an open socket with a recent inbound message', () => {
    const socket = new BaseWebSocket(createParent());
    socket.ws = { readyState: WebSocket.OPEN } as unknown as WebSocket;

    expect(socket.isHealthy).toBe(false);

    socket.lastReceived = Date.now();
    expect(socket.isHealthy).toBe(true);

    socket.lastReceived = Date.now() - WS_RECONNECT_THRESHOLD_MS;
    expect(socket.isHealthy).toBe(false);
  });

  test('close codes have stable human-readable fallback text', () => {
    expect(getCloseCodeDescription(1000)).toBe('Normal closure');
    expect(getCloseCodeDescription(4999)).toBe('Unknown close code (4999)');
  });

  test('only classifies actual protocol heartbeat frames as ping or pong', () => {
    expect(isHeartbeatFrame('ping')).toBe(true);
    expect(isHeartbeatFrame(JSON.stringify({ op: 'ping' }))).toBe(true);
    expect(isHeartbeatFrame(JSON.stringify({ method: 'server.ping', id: 1 }))).toBe(true);
    expect(isHeartbeatFrame(JSON.stringify({ ping: 123 }))).toBe(true);
    expect(isHeartbeatFrame(JSON.stringify({ op: 'subscribe', symbol: 'SPONGEUSDT' }))).toBe(false);
    expect(isHeartbeatFrame(JSON.stringify({ type: 'order', clientId: 'ping-strategy' }))).toBe(false);
  });

  test('a subscription containing ping in its symbol still triggers reconnect while closed', () => {
    const socket = new BaseWebSocket(createParent());
    socket.ws = { readyState: WebSocket.CLOSED } as unknown as WebSocket;
    const reconnect = vi.spyOn(socket, 'reconnect').mockResolvedValue();

    socket.send(JSON.stringify({ op: 'subscribe', args: ['SPONGEUSDT'] }));

    expect(reconnect).toHaveBeenCalledTimes(1);
  });
});
