import { afterEach, expect, it, vi } from 'vitest';
import type { RelaySocket } from '@tealstreet/cli-core';
import { loadConfig, saveConfig } from '../config';
import { enqueueSessionWork, getStorageOwner } from '../storage-context';
import { startWebhookRelay, stopWebhookRelay } from './webhook-relay';
import { createHook } from './hooks-store';
import { handleHookFire } from './handlers/hook';

vi.mock('../auth', () => ({ getProxyTokenFromConfig: () => 'synthetic-token' }));
vi.mock('../updater', () => ({ getCurrentVersion: () => '0.0.0' }));
vi.mock('../logger', () => ({ originalConsoleLog: vi.fn(), logger: { warn: vi.fn() } }));

class FakeSocket implements RelaySocket {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  send(_data: string) {}
  close() {
    this.onclose?.();
  }
  receive(frame: Record<string, unknown>) {
    this.onmessage?.({ data: JSON.stringify(frame) });
  }
}
function login(userId: string) {
  const config = loadConfig();
  config.auth = { userId, accessToken: 'synthetic-token' };
  saveConfig(config);
}
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
afterEach(() => stopWebhookRelay());

it.each(['unchanged', 'before ack', 'after ack', 'away and back'])(
  'binds an accepted hosted delivery to its original login: %s',
  async (switchAt) => {
    login('A');
    stopWebhookRelay();
    const sockets: FakeSocket[] = [];
    const runs: Array<string | null> = [];
    const warn = vi.fn();
    startWebhookRelay({
      createSocket: () => {
        const socket = new FakeSocket();
        sockets.push(socket);
        return socket;
      },
      notice: { info: vi.fn(), warn },
      runBody: () =>
        enqueueSessionWork(
          Promise.resolve(),
          () => {
            runs.push(getStorageOwner());
          },
          warn,
        ),
    });
    const socket = sockets[0];
    socket.onopen?.();
    socket.receive({ type: 'rpc.auth', status: 'success' });
    socket.receive({ type: 'webhook.registered', listenerId: 'synthetic', claimedListenerId: 'synthetic' });
    socket.receive({ type: 'webhook.deliver', deliveryId: 'old-delivery', body: 'buy' });
    if (switchAt === 'before ack') login('B');
    socket.receive({ type: 'webhook.acked', deliveryId: 'old-delivery', status: 'received', accepted: true });
    if (switchAt === 'after ack' || switchAt === 'away and back') login('B');
    if (switchAt === 'away and back') login('A');
    await flush();
    expect(runs).toEqual(switchAt === 'unchanged' ? ['A'] : []);
    if (switchAt === 'after ack' || switchAt === 'away and back') {
      expect(warn).toHaveBeenCalledWith(expect.stringMatching(/login changed.*cancel/i));
      // The replacement relay may still accept this login's own alerts.
      const current = sockets[sockets.length - 1];
      current.onopen?.();
      current.receive({ type: 'rpc.auth', status: 'success' });
      current.receive({ type: 'webhook.deliver', deliveryId: 'new-delivery', body: 'buy' });
      current.receive({ type: 'webhook.acked', deliveryId: 'new-delivery', status: 'received', accepted: true });
      await flush();
      expect(runs).toEqual([getStorageOwner()]);
    }
  },
);

it.each([false, true])('keeps a local hook in its accepted session (switch: %s)', async (switchUser) => {
  login('A');
  const { hook } = createHook({ name: 'session-test', command: 'buy' });
  const publish = vi.fn(() => 1);
  let resume = () => {};
  const gate = new Promise<void>((resolve) => {
    resume = resolve;
  });
  const runs: Array<string | null> = [];
  const result = await handleHookFire({ publish }, hook.id, hook.secret, async () => {
    await gate;
    runs.push(getStorageOwner());
  });
  expect(result.status).toBe(200);
  if (switchUser) login('B');
  resume();
  await flush();
  expect(runs).toEqual(switchUser ? [] : ['A']);
  expect(publish).toHaveBeenCalledWith('hook-output', expect.stringContaining(`"success":${!switchUser}`));
  if (switchUser) expect(publish).toHaveBeenCalledWith('hook-output', expect.stringMatching(/login changed.*cancel/i));
});
