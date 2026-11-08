import { beforeEach, expect, it, vi } from 'vitest';
import type { ServerWebSocket } from 'bun';
import type { WsSessionData } from './ws';

const approval = vi.hoisted(() => vi.fn<() => Promise<boolean>>());
vi.mock('./prompt', () => ({ promptApproval: approval }));
vi.mock('../logger', () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
  originalConsoleLog: vi.fn(),
  originalConsoleWarn: vi.fn(),
  originalConsoleError: vi.fn(),
}));
beforeEach(() => {
  vi.resetModules();
  approval.mockReset();
  approval.mockResolvedValue(false);
});

function socket() {
  const frames: unknown[] = [];
  // Spread so bun-types releases without socket flow control still accept the fixture.
  const flowControl = { pause() {}, resume() {}, isPaused: false };
  const ws: ServerWebSocket<WsSessionData> = {
    ...flowControl,
    data: { origin: 'https://v3.tealstreet.io', trusted: false, helloSeen: false, webCrossTradeActive: false },
    send: (data) => {
      if (typeof data === 'string') frames.push(JSON.parse(data));
      return 1;
    },
    close: vi.fn(),
    terminate: vi.fn(),
    sendText: () => 1,
    sendBinary: () => 1,
    ping: () => 1,
    pong: () => 1,
    publish: () => 1,
    publishText: () => 1,
    publishBinary: () => 1,
    subscribe: () => true,
    unsubscribe: () => true,
    isSubscribed: () => false,
    subscriptions: [],
    remoteAddress: '127.0.0.1',
    readyState: 1,
    getBufferedAmount: () => 0,
    cork: () => {
      throw new Error('the listener never corks');
    },
  };
  return { ws, frames };
}
async function fixture() {
  const config = await import('../config');
  const login = (userId: string) => {
    const c = config.loadConfig();
    c.auth = { userId, accessToken: 'fixture' };
    config.saveConfig(c);
  };
  login('A');
  const state = await import('./state');
  const { createStandalonePlatformAdapter } = await import('../platform-adapter');
  const { buildWebSocketHandler } = await import('./ws');
  const options = () => ({
    adapter: createStandalonePlatformAdapter(),
    listenerId: state.getListenerId(),
    processCommand: vi.fn(async () => {}),
    notifyOwnAccountWrite: vi.fn(),
    notifyOwnCrossTradeWrite: vi.fn(),
  });
  return { config, login, state, buildWebSocketHandler, options };
}
it('closes A’s approved socket on switch and requires B to approve the same browser again', async () => {
  const f = await fixture();
  f.config.addAccount({ name: 'A', exchange: 'bybit', apiKey: 'fixture-A', apiSecret: 'fixture-A-secret' });
  f.state.addTrustedClient({ clientId: 'browser', label: 'A browser', origin: 'https://v3.tealstreet.io' });
  const handler = f.buildWebSocketHandler(f.options());
  const a = socket();
  handler.open?.(a.ws);
  await handler.message?.(a.ws, JSON.stringify({ t: 'hello', clientId: 'browser' }));
  expect(a.ws.data.trusted).toBe(true);
  expect(JSON.stringify(a.frames)).toContain('fixture-A-secret');
  f.login('B');
  expect(a.ws.data.trusted).toBe(false);
  expect(a.ws.close).toHaveBeenCalledWith(4001, 'login-changed');
  const next = f.buildWebSocketHandler(f.options());
  const b = socket();
  next.open?.(b.ws);
  await next.message?.(b.ws, JSON.stringify({ t: 'hello', clientId: 'browser' }));
  expect(b.ws.data.trusted).toBe(false);
  expect(approval).toHaveBeenCalledTimes(1);
  expect(JSON.stringify(b.frames)).not.toContain('fixture-A-secret');
});
it('does not save a pending A approval or send its snapshot after switching to B', async () => {
  const f = await fixture();
  let approve: (value: boolean) => void = () => {};
  approval.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        approve = resolve;
      }),
  );
  const handler = f.buildWebSocketHandler(f.options());
  const { ws, frames } = socket();
  handler.open?.(ws);
  const pending = handler.message?.(ws, JSON.stringify({ t: 'hello', clientId: 'pending' }));
  f.login('B');
  approve(true);
  await pending;
  expect(f.state.isTrusted('pending')).toBeUndefined();
  expect(ws.data.trusted).toBe(false);
  expect(frames).toEqual([]);
});
