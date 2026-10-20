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
  removeEventListener = (name: string, callback: (event: MessageEvent) => void) => {
    this.listeners.set(name, (this.listeners.get(name) ?? []).filter((listener) => listener !== callback));
  };
  fire(name: string, data?: unknown) {
    if (name === 'open') this.readyState = 1;
    if (name === 'close') this.readyState = 3;
    for (const callback of this.listeners.get(name) ?? []) callback({ data } as MessageEvent);
  }
  subscriptions() { return this.frames.filter((frame) => JSON.parse(frame.text).method === 'subscribe'); }
}
function create(ids = Array.from({ length: 78 }, (_, i) => i), api = true, open = true, publicDataOnly = true) {
  const store = new DefaultStore();
  store.update({ markets: ids.map((i) => ({ id: String(i), symbol: `M${i}-USDC`, base: `M${i}`, quote: 'USDC', active: true, precision: { price: 0.1, amount: 0.001 }, limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } } })) });
  const parent = { store, isDisposed: false, options: { publicDataOnly, key: '' }, wallet: () => '0x1111111111111111111111111111111111111111', account: { apiFrame: vi.fn(), disconnected: vi.fn(), refresh: vi.fn(async () => {}) }, emitAndLogError: vi.fn(), emitWsReconnected: vi.fn(), log: vi.fn() } as unknown as TxFlowExchange;
  const feed = new TxFlowWebSocket(parent, api);
  const sockets: Socket[] = [];
  vi.spyOn(feed, 'createWebsocket').mockImplementation(() => {
    const socket = new Socket(); if (!open) socket.readyState = 0; sockets.push(socket); return socket as unknown as WebSocket;
  });
  feed.connectAndSubscribe();
  if (open) sockets[0].fire('open');
  return { feed, sockets, store };
}
beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(1_700_000_000_000); });
afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });
it('orders initial and replacement API ticker queues by numeric instrument ID without reordering metadata', () => {
  const ids = [65, 2, 6, 10, 1, 100];
  const { feed, sockets, store } = create(ids);
  for (let generation = 0; generation < 2; generation++) {
    const socket = sockets[generation];
    expect(socket.subscriptions()).toHaveLength(1);
    vi.advanceTimersByTime(150 * (ids.length - 1));
    const frames = socket.subscriptions();
    expect(frames.map(({ text }) => JSON.parse(text).subscription)).toEqual(
      [1, 2, 6, 10, 65, 100].map((instrumentId) => ({ type: 'ticker', coin: `M${instrumentId}`, instrumentId })),
    );
    expect(frames.slice(1).map((frame, i) => frame.at - frames[i].at)).toEqual(Array(ids.length - 1).fill(150));
    expect(store.markets.map((market) => Number(market.id))).toEqual(ids);
    if (generation === 0) {
      socket.fire('close');
      feed.connectAndSubscribe();
      sockets[1].fire('open');
    }
  }
  feed.dispose();
});
it.each([false, true])('sorts restored intents on initial and replacement sockets without changing active call order (api=%s)', (api) => {
  const ids = [65, 2, 10, 1, 100, 6];
  const { feed, sockets } = create(ids, api, false, false);
  const candles = [[65, '5m'], [2, '5m'], [2, '1m'], [10, '15m'], [1, '1m'], [1, '15m']] as const;
  for (const id of ids.slice(0, 4)) feed.listenOrderBook(`M${id}-USDC`, vi.fn());
  for (const [id, interval] of candles) feed.listenOHLCV({ symbol: `M${id}-USDC`, interval }, vi.fn());
  expect(sockets[0].subscriptions()).toHaveLength(0);
  const expected: Array<Record<string, unknown>> = api ? [1, 2, 6, 10, 65, 100].map((instrumentId) => ({ type: 'ticker', coin: `M${instrumentId}`, instrumentId })) : [{ type: 'allAssetCtxs' }];
  if (api) {
    for (const [id, interval] of [[1, '15m'], [1, '1m'], [2, '1m'], [2, '5m'], [10, '15m'], [65, '5m']]) {
      expected.push({ type: 'candle', coin: String(id), interval });
    }
  } else {
    for (const id of [1, 2, 10, 65]) expected.push({ type: 'l2Book', coin: String(id) });
    for (const type of ['orderUpdates', 'clearinghouseState', 'userAssetData']) expected.push({ type, user: '0x1111111111111111111111111111111111111111' });
  }
  for (let generation = 0; generation < 2; generation++) {
    const socket = sockets[generation];
    socket.fire('open');
    expect(socket.subscriptions()).toHaveLength(1);
    vi.advanceTimersByTime(150 * expected.length);
    const frames = socket.subscriptions();
    expect(frames.map(({ text }) => JSON.parse(text).subscription)).toEqual(expected);
    expect(frames.slice(1).map((frame, i) => frame.at - frames[i].at)).toEqual(Array(expected.length - 1).fill(150));
    expect(frames.every(({ text }) => {
      const frame = JSON.parse(text);
      return api ? !('type' in frame) : frame.type === frame.subscription.type;
    })).toBe(true);
    if (generation === 0) {
      socket.fire('close');
      vi.advanceTimersByTime(3001);
      expect(sockets).toHaveLength(2);
    }
  }
  const socket = sockets[1];
  const before = socket.subscriptions().length;
  // New intents on an already-open socket retain caller order, even when IDs descend.
  if (api) {
    feed.listenOHLCV({ symbol: 'M65-USDC', interval: '1h' }, vi.fn());
    feed.listenOHLCV({ symbol: 'M2-USDC', interval: '1h' }, vi.fn());
  } else {
    feed.listenOrderBook('M100-USDC', vi.fn());
    feed.listenOrderBook('M6-USDC', vi.fn());
  }
  vi.advanceTimersByTime(150);
  expect(socket.subscriptions().slice(before).map(({ text }) => JSON.parse(text).subscription.coin)).toEqual(api ? ['65', '2'] : ['100', '6']);
  feed.dispose();
});
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
