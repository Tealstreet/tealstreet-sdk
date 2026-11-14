import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { WebhookRelayClient, type RelaySocket, type WebhookRelayOptions } from '@tealstreet/cli-core';
import { deferred, sessionFixture } from './deferred-test.fixture';
class Socket implements RelaySocket {
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  sent: string[] = [];
  send(frame: string) {
    this.sent.push(frame);
  }
  close() {
    this.onclose?.();
  }
  receive(frame: Record<string, unknown>) {
    this.onmessage?.({ data: JSON.stringify(frame) });
  }
}
const clients: WebhookRelayClient[] = [];
beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-relay-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(() => {
  clients.splice(0).forEach((client) => client.stop());
  vi.useRealTimers();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});
async function fixture(overrides: Partial<WebhookRelayOptions> = {}) {
  const { storage, login } = await sessionFixture();
  const sockets: Socket[] = [];
  const client = new WebhookRelayClient({
    listenerId: 'fixture',
    kind: 'cli',
    label: 'fixture',
    getAccounts: () => [],
    getToken: () => `${storage.getStorageOwner()}-token-fixture`,
    runBody: async () => {},
    notice: { info() {}, warn() {} },
    createSocket: () => {
      const socket = new Socket();
      sockets.push(socket);
      return socket;
    },
    ...overrides,
  });
  clients.push(client);
  storage.onStorageOwnerChange(() => client.stop());
  client.start();
  return { client, sockets, login };
}
it.each(['same', 'B', 'away and back'])('invalidates a token fetched before the login switch: %s', async (change) => {
  const wait = deferred();
  const { sockets, login } = await fixture({
    getToken: async () => {
      await wait.promise;
      return 'A-token-fixture';
    },
  });
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
  }
  wait.resolve();
  await Promise.resolve();
  await Promise.resolve();
  expect(sockets).toHaveLength(change === 'same' ? 1 : 0);
});
it.each(['reconnect', 'register'])('invalidates a saved %s retry during login', async (kind) => {
  for (const change of ['same', 'B', 'away and back']) {
    const timeout = vi.spyOn(globalThis, 'setTimeout');
    const { client, sockets, login } = await fixture();
    const socket = sockets[0];
    socket.onopen?.();
    socket.receive({ type: 'rpc.auth', status: 'success' });
    if (kind === 'reconnect') socket.onclose?.();
    else socket.receive({ type: 'webhook.error', code: 'unavailable' });
    const retry = timeout.mock.calls.at(-1)?.[0];
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
    }
    const before = socket.sent.length;
    if (typeof retry === 'function') retry();
    if (kind === 'reconnect') expect(sockets, change).toHaveLength(change === 'same' ? 2 : 1);
    else expect(socket.sent.length - before, change).toBe(change === 'same' ? 1 : 0);
    client.stop();
    timeout.mockRestore();
  }
});
it.each(['same', 'B', 'away and back'])(
  'drops output and buffered results from a body held during login: %s',
  async (change) => {
    const entered = deferred(),
      wait = deferred(),
      mirror = vi.fn(),
      onFinished = vi.fn();
    const { sockets, login } = await fixture({
      mirror,
      onFinished,
      runBody: async (_body, context) => {
        entered.resolve();
        await wait.promise;
        context.printSink('print', 'A-private-result');
      },
    });
    const socket = sockets[0];
    socket.onopen?.();
    socket.receive({ type: 'rpc.auth', status: 'success' });
    socket.receive({ type: 'webhook.registered', claimed_listener_id: 'fixture' });
    socket.receive({ type: 'webhook.deliver', deliveryId: 'A-id', body: 'fixture' });
    socket.receive({ type: 'webhook.acked', deliveryId: 'A-id', status: 'received', accepted: true });
    await entered.promise;
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
    }
    wait.resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(mirror).toHaveBeenCalledTimes(change === 'same' ? 1 : 0);
    expect(onFinished).toHaveBeenCalledTimes(change === 'same' ? 1 : 0);
    expect(socket.sent.some((frame) => frame.includes('A-private-result'))).toBe(change === 'same');
  },
);
it.each(['state', 'claim', 'release'])(
  'invalidates a saved %s timeout during login without settling B requests',
  async (kind) => {
    for (const change of ['same', 'B', 'away and back']) {
      const { client, sockets, login } = await fixture();
      const timeout = vi.spyOn(globalThis, 'setTimeout');
      const request = () =>
        kind === 'state' ? client.requestState() : kind === 'claim' ? client.claim() : client.release();
      const pendingA = request();
      const timerA = timeout.mock.calls.at(-1)?.[0];
      if (change === 'same') {
        if (typeof timerA === 'function') timerA();
        expect(await pendingA).toBe(kind === 'state' ? null : 'timeout');
      } else {
        login('B');
        if (change === 'away and back') login('A');
        expect(await pendingA).toBe(kind === 'state' ? null : 'disconnected');
        client.start();
        const pendingB = request();
        let bSettled = false;
        void pendingB.then(() => {
          bSettled = true;
        });
        if (typeof timerA === 'function') timerA();
        await Promise.resolve();
        expect(bSettled).toBe(false);
        sockets
          .at(-1)
          ?.receive(
            kind === 'state'
              ? { type: 'webhook.state', state: { claimed_listener_id: 'B', hold_seconds: 1, listeners: [] } }
              : { type: 'webhook.claim_result', status: 'B-result' },
          );
        if (kind === 'state')
          expect(await pendingB).toEqual({ claimed_listener_id: 'B', hold_seconds: 1, listeners: [] });
        else expect(await pendingB).toBe('B-result');
      }
      client.stop();
      timeout.mockRestore();
    }
  },
);
