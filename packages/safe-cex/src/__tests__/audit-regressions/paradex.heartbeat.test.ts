import { afterEach, expect, it, vi } from 'vitest';
import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { DefaultStore } from '../../store/store.base';

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });
it('uses socket control pings without sending invalid Paradex application heartbeat requests', () => {
  vi.useFakeTimers();
  const exchange = new ParadexExchange('paradex', { publicDataOnly: true, tuning: { consoleLogging: false } } as any, new DefaultStore());
  for (const socket of [exchange.publicWebsocket, exchange.privateWebsocket]) {
    Object.defineProperty(socket, 'isConnected', { get: () => true });
    const send = vi.spyOn(socket as any, 'send').mockImplementation(() => {});
    socket.ping();
    socket.handlePongEvent();
    vi.advanceTimersByTime(35_000);
    expect(send).not.toHaveBeenCalled();
    expect(socket.getWatchdogStaleThresholdMs()).toBe(90_000);
  }
  exchange.dispose();
});
it('authenticates only the matching object result with the documented node identity or legacy empty object', () => {
  const exchange = new ParadexExchange('paradex', { publicDataOnly: true, tuning: { consoleLogging: false } } as any, new DefaultStore());
  const socket = exchange.privateWebsocket as any;
  Object.defineProperty(socket, 'isConnected', { get: () => true });
  socket.authRequestId = 7;
  const subscribe = vi.spyOn(socket, 'subscribeToPrivateChannels').mockImplementation(() => {});
  for (const result of [null, false, 'ok', [], { channel: 'orders.ALL' }, { node_id: '' }]) {
    socket.onMessage({ data: JSON.stringify({ jsonrpc: '2.0', id: 7, result }) });
    expect(subscribe).not.toHaveBeenCalled();
  }
  for (const result of [{ node_id: 'a1b2c3' }, {}]) {
    socket.authenticated = false;
    socket.onMessage({ data: JSON.stringify({ jsonrpc: '2.0', id: 6, result }) });
    expect(socket.authenticated).toBe(false);
    socket.onMessage({ data: JSON.stringify({ jsonrpc: '2.0', id: 7, result }) });
    expect(socket.authenticated).toBe(true);
  }
  expect(subscribe).toHaveBeenCalledTimes(2);
  exchange.dispose();
});
