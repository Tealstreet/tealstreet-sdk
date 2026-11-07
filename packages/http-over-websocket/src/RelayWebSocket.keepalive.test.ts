import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { configureHOW } from './config';
import { RelayWebSocket } from './RelayWebSocket';

class BrowserSocket extends EventTarget {
  static instances: BrowserSocket[] = [];
  readyState = 0;
  bufferedAmount = 0;
  sent: string[] = [];
  constructor() {
    super();
    BrowserSocket.instances.push(this);
  }
  send(text: string) {
    this.sent.push(text);
  }
  message(value: unknown) {
    this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify(value) }));
  }
  close = vi.fn(() => {
    this.readyState = 3;
    this.dispatchEvent(new Event('close'));
  });
  open() {
    this.readyState = 1;
    this.dispatchEvent(new Event('open'));
  }
}
class NodeSocket extends BrowserSocket {
  readonly pongListeners = new Set<() => void>();
  ping = vi.fn();
  on(_event: 'pong', listener: () => void) {
    this.pongListeners.add(listener);
  }
  off(_event: 'pong', listener: () => void) {
    this.pongListeners.delete(listener);
  }
  pong() {
    this.pongListeners.forEach((listener) => listener());
  }
}
const sockets: RelayWebSocket[] = [];
beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(100_000);
  BrowserSocket.instances = [];
  configureHOW({ getProxyToken: async () => 'fixture-token' });
});
afterEach(() => {
  sockets.splice(0).forEach((socket) => socket.close());
  vi.useRealTimers();
});

async function connected(Socket: typeof BrowserSocket = NodeSocket) {
  const relay = new RelayWebSocket(
    'wss://relay.test/tunnel?ts-relay=wss%3A%2F%2Fstream.bybit.com%2Fv5%2Fprivate',
    [],
    undefined,
    Socket as never,
  );
  sockets.push(relay);
  const inner = BrowserSocket.instances.at(-1)!;
  inner.open();
  await vi.advanceTimersByTimeAsync(0);
  inner.message({ type: 'rpc.auth', status: 'success' });
  inner.message({ type: 'rpc.relay', status: 'open' });
  expect(relay.readyState).toBe(relay.OPEN);
  return { relay, inner: inner as NodeSocket };
}

it('keeps a quiet Node relay alive with native ping/pong without sending tunnel RPC into the venue pipe', async () => {
  const { relay, inner } = await connected();
  const before = [...inner.sent];
  for (let i = 1; i <= 6; i++) {
    await vi.advanceTimersByTimeAsync(15_000);
    expect(inner.ping).toHaveBeenCalledTimes(i);
    inner.pong();
    expect(relay.readyState).toBe(relay.OPEN);
  }
  expect(inner.sent).toEqual(before);
  relay.close();
  expect(inner.pongListeners.size).toBe(0);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(inner.ping).toHaveBeenCalledTimes(6);
});

it('closes a half-open Node relay once after 45 seconds without a pong or venue message', async () => {
  const { relay, inner } = await connected();
  const close = vi.fn();
  const messages = vi.fn();
  relay.addEventListener('close', close);
  relay.addEventListener('message', messages);
  await vi.advanceTimersByTimeAsync(44_999);
  expect(relay.readyState).toBe(relay.OPEN);
  await vi.advanceTimersByTimeAsync(1);
  expect(relay.readyState).toBe(relay.CLOSED);
  expect(close).toHaveBeenCalledTimes(1);
  expect(messages).not.toHaveBeenCalled();
  expect(inner.close).toHaveBeenCalledTimes(1);
  expect(inner.pongListeners.size).toBe(0);
});

it('uses elapsed time after a delayed timer and never sends a stale ping', async () => {
  const { relay, inner } = await connected();
  vi.setSystemTime(Date.now() + 60_000);
  await vi.advanceTimersByTimeAsync(15_000);
  expect(relay.readyState).toBe(relay.CLOSED);
  expect(inner.ping).not.toHaveBeenCalled();
});

it('retains browser application framing and relies on the venue heartbeat when native ping is unavailable', async () => {
  const { relay, inner } = await connected(BrowserSocket);
  await vi.advanceTimersByTimeAsync(90_000);
  relay.send('{"op":"ping"}');
  expect(relay.readyState).toBe(relay.OPEN);
  expect(inner.sent.map((text) => JSON.parse(text))).toEqual([
    { type: 'rpc.auth', token: 'fixture-token', source: 'worker' },
    { type: 'rpc.relay', upstream: 'wss://stream.bybit.com/v5/private' },
    { op: 'ping' },
  ]);
});

it.each(['pong', 'message'])('does not let a late %s revive a relay past its silence deadline', async (kind) => {
  const { relay, inner } = await connected();
  const message = vi.fn();
  relay.addEventListener('message', message);
  vi.setSystemTime(Date.now() + 45_000);
  if (kind === 'pong') inner.pong();
  else inner.message({ op: 'pong' });
  expect(relay.readyState).toBe(relay.CLOSED);
  expect(message).not.toHaveBeenCalled();
});

it('retires before notifying errors so a reconnect handler cannot close the old transport twice', async () => {
  const { relay, inner } = await connected();
  inner.ping.mockImplementation(() => {
    throw new Error('fixture-private-transport-error');
  });
  relay.onerror = () => relay.close();
  const errors = vi.fn();
  relay.addEventListener('error', errors);
  await vi.advanceTimersByTimeAsync(15_000);
  expect(relay.readyState).toBe(relay.CLOSED);
  expect(inner.close).toHaveBeenCalledOnce();
  expect(errors).toHaveBeenCalledOnce();
  expect((errors.mock.calls[0][0] as Event & { message: string }).message).toBe('relay heartbeat failed');
  expect(inner.pongListeners.size).toBe(0);
});
