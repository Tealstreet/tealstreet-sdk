import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ReconnectingWebSocket } from '@tealstreet/reconnecting-websocket/src/ReconnectingWebsocket';
import { DefaultStore } from '../../store/store.base';
import type { TxFlowExchange } from '../reference/txflow/txflow.exchange';
import { TxFlowWebSocket } from '../reference/txflow/txflow.ws-public';

class Socket {
  readyState = 1;
  frames: Array<{ at: number; text: string }> = [];
  listeners = new Map<string, Array<(event: MessageEvent) => void>>();
  send = (text: string) => { this.frames.push({ at: Date.now(), text }); };
  close = vi.fn(() => { this.readyState = 3; });
  addEventListener = (name: string, callback: (event: MessageEvent) => void) => {
    const callbacks = this.listeners.get(name) ?? [];
    callbacks.push(callback);
    this.listeners.set(name, callbacks);
  };
  fire(name: string, data?: unknown) {
    for (const callback of this.listeners.get(name) ?? []) callback({ data } as MessageEvent);
  }
  subscriptions() { return this.frames.filter((frame) => JSON.parse(frame.text).method === 'subscribe'); }
}
function create() {
  const store = new DefaultStore();
  store.update({ markets: Array.from({ length: 78 }, (_, i) => ({ id: String(i), symbol: `M${i}-USDC`, base: `M${i}`, quote: 'USDC', active: true, precision: { price: 0.1, amount: 0.001 }, limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } } })) });
  const parent = { store, isDisposed: false, options: { publicDataOnly: true }, account: { apiFrame: vi.fn() }, emitAndLogError: vi.fn(), log: vi.fn() } as unknown as TxFlowExchange;
  const feed = new TxFlowWebSocket(parent, true);
  const sockets: Socket[] = [];
  vi.spyOn(feed, 'createWebsocket').mockImplementation(() => {
    const socket = new Socket(); sockets.push(socket); return socket as unknown as WebSocket;
  });
  feed.connectAndSubscribe();
  sockets[0].fire('open');
  return { feed, sockets };
}
beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(1_700_000_000_000); });
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });
it('paces all 78 real-market subscriptions while keeping ten-second pings independent', () => {
  const { feed, sockets: [socket] } = create();
  const incoming = setInterval(() => socket.fire('message', '{"channel":"pong"}'), 2000);
  expect(socket.subscriptions()).toHaveLength(1);
  vi.advanceTimersByTime(149);
  expect(socket.subscriptions()).toHaveLength(1);
  vi.advanceTimersByTime(150 * 77 - 149);
  const frames = socket.subscriptions();
  expect(frames).toHaveLength(78);
  expect(new Set(frames.map((frame) => JSON.parse(frame.text).subscription.instrumentId)).size).toBe(78);
  expect(frames.slice(1).every((frame, i) => frame.at - frames[i].at >= 150)).toBe(true);
  expect(socket.frames.filter((frame) => JSON.parse(frame.text).method === 'ping')).toHaveLength(1);
  clearInterval(incoming);
  feed.dispose();
});
it('stops a rejected generation and rebuilds intents without sending its backlog on the replacement', () => {
  const { feed, sockets } = create();
  sockets[0].fire('message', JSON.stringify({ channel: 'error', data: { code: 'RISK_RATE_LIMITED' } }));
  expect(sockets[0].close).toHaveBeenCalledOnce();
  vi.advanceTimersByTime(500);
  expect(sockets[0].subscriptions()).toHaveLength(1);
  feed.connectAndSubscribe(); sockets[1].fire('open');
  sockets[0].fire('close');
  sockets[0].fire('message', JSON.stringify({ channel: 'error', data: { code: 'RISK_RATE_LIMITED' } }));
  vi.advanceTimersByTime(150);
  expect(sockets[1].close).not.toHaveBeenCalled();
  expect(sockets[1].subscriptions()).toHaveLength(2);
  expect(JSON.parse(sockets[1].subscriptions()[0].text).subscription.instrumentId).toBe(0);
  feed.dispose();
  vi.advanceTimersByTime(1000);
  expect(sockets[1].subscriptions()).toHaveLength(2);
});
it('uses automatic reconnect for the reconnecting wrapper rather than disabling its retries', async () => {
  const { feed } = create();
  const wrapper = new ReconnectingWebSocket('wss://rpc.txflow-testnet.xyz/api-ws', [], { startClosed: true });
  Object.defineProperty(wrapper, 'readyState', { value: WebSocket.OPEN });
  const frames: string[] = [];
  vi.spyOn(wrapper, 'send').mockImplementation((text) => { frames.push(String(text)); });
  const close = vi.spyOn(wrapper, 'close').mockImplementation(() => {});
  const reconnect = vi.spyOn(wrapper, 'reconnect').mockImplementation(() => { wrapper.dispatchEvent(new Event('open')); });
  vi.spyOn(feed, 'createWebsocket').mockReturnValue(wrapper);
  feed.connectAndSubscribe(); wrapper.dispatchEvent(new Event('open'));
  wrapper.dispatchEvent(new MessageEvent('message', { data: '{"channel":"error","data":{"code":"RISK_RATE_LIMITED"}}' }));
  wrapper.dispatchEvent(new MessageEvent('message', { data: '{"channel":"error","data":{"code":"RISK_RATE_LIMITED"}}' }));
  await vi.advanceTimersByTimeAsync(3001);
  expect(reconnect).toHaveBeenCalledOnce();
  expect(close).not.toHaveBeenCalled();
  expect(frames.filter((text) => JSON.parse(text).subscription?.instrumentId === 0)).toHaveLength(2);
  await vi.advanceTimersByTimeAsync(150);
  expect(frames.length).toBeGreaterThan(2);
  feed.dispose();
});
