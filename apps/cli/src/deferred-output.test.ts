import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import ts from 'typescript';
import { deferred, evaluate, parsedSource, sessionFixture, sourceFunction } from './deferred-test.fixture';
import type { RelaySocket } from '@tealstreet/cli-core';
vi.mock('./logger', () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
  originalConsoleLog: vi.fn(),
  originalConsoleWarn: vi.fn(),
  originalConsoleError: vi.fn(),
  isStdoutReservedForJson: () => false,
  writeAbovePrompt: (run: () => void) => run(),
}));
vi.mock('./auth', () => ({ getProxyTokenFromConfig: () => 'synthetic-fixture' }));
vi.mock('./updater', () => ({ getCurrentVersion: () => '0.0.0' }));
beforeEach(() => {
  vi.resetModules();
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-output-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(async () => {
  const { stopWebhookRelay } = await import('./listener/webhook-relay');
  stopWebhookRelay();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});
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
it.each(['same', 'B', 'away and back'])(
  'drops late relay state output after a login switch: %s',
  async (change) => {
    const { login } = await sessionFixture();
    const { createStandalonePlatformAdapter } = await import('./platform-adapter');
    const { cmdWebhookStatus } = await import('./listener/webhook-repl');
    const { startWebhookRelay } = await import('./listener/webhook-relay');
    const { originalConsoleLog } = await import('./logger');
    const sockets: Socket[] = [];
    startWebhookRelay({
      runBody: async () => {},
      notice: { info() {}, warn() {} },
      createSocket: () => {
        const socket = new Socket();
        sockets.push(socket);
        return socket;
      },
    });
    const socket = sockets[0];
    socket.onopen?.();
    socket.receive({ type: 'rpc.auth', status: 'success' });
    const pending = cmdWebhookStatus(createStandalonePlatformAdapter());
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
    }
    socket.receive({
      type: 'webhook.state',
      state: {
        claimed_listener_id: 'old',
        hold_seconds: 30,
        listeners: [
          { listener_id: 'old', label: 'A-private-label', accounts: ['A-private-account'], online: true, kind: 'cli' },
        ],
      },
    });
    await pending;
    const printed = vi.mocked(originalConsoleLog).mock.calls.flat().join(' ');
    if (change === 'same') expect(printed).toContain('A-private-account');
    else expect(printed).not.toContain('A-private-account');
  },
  30_000,
);
it.each(['same', 'B'])('checks print sinks after a held command continuation: %s', async (change) => {
  const { storage, login } = await sessionFixture();
  const { createStandalonePlatformAdapter } = await import('./platform-adapter');
  const logger = await import('./logger');
  const adapter = createStandalonePlatformAdapter(),
    wait = deferred();
  const pending = storage.runInStorageSession(async () => {
    await wait.promise;
    adapter.printInfo('A-private-output');
  });
  if (change === 'B') login('B');
  wait.resolve();
  await pending;
  expect(vi.mocked(logger.originalConsoleLog).mock.calls.flat().join(' ')).toBe(
    change === 'same' ? 'A-private-output' : '',
  );
});
it.each(['same', 'B'])('rebinds footer state while its renderer is waiting for a redraw: %s', async (change) => {
  const { storage, config, login } = await sessionFixture();
  login('B');
  const b = config.loadConfig();
  b.footer = { segments: [{ type: 'text', text: 'B-note' }] };
  config.saveConfig(b);
  login('A');
  const source = parsedSource('repl.ts'),
    callback = source.statements.find(
      (node) => ts.isExpressionStatement(node) && node.expression.getText(source).startsWith('onStorageOwnerChange('),
    );
  if (!callback) throw new Error('Missing REPL owner callback');
  const scope = {
    ...storage,
    session: { currentExchange: null, currentAccountName: 'Main', currentSymbol: 'BTCUSDT' },
    connectionStates: new Map(),
    stopMonitorFn: null,
    connectionMonitor: null,
    networkMonitor: null,
    rlInstance: null,
    stopFooterScriptRefresh() {},
    startFooterScriptRefresh() {},
    enableFooter() {},
    disableFooter() {},
    setFooterRenderer() {},
    loadConfig: config.loadConfig,
    DEFAULT_SEGMENTS: [],
    watchMessageBuffer: ['A-fill'],
    footerSegments: [{ type: 'text', text: 'A-note' }],
    renderFooter: (segments: unknown) => segments,
    buildFooterState: () => ({}),
  };
  evaluate<void>(callback.getText(source), scope);
  if (change === 'B') login('B');
  const render = evaluate<() => unknown>(`${sourceFunction('repl.ts', 'footerRenderer')}\nfooterRenderer;`, scope);
  expect(render()).toEqual([{ type: 'text', text: change === 'same' ? 'A-note' : 'B-note' }]);
  expect(scope.watchMessageBuffer).toEqual(change === 'same' ? ['A-fill'] : []);
});
it.each(['same', 'B', 'away and back'])('keeps capture completion in its own buffer: %s', async (change) => {
  const { login } = await sessionFixture();
  const { createStandalonePlatformAdapter } = await import('./platform-adapter');
  const adapter = createStandalonePlatformAdapter();
  const finishA = adapter.startCapture();
  adapter.print(['A-output']);
  if (change === 'same') {
    expect(finishA()).toBe('A-output');
    return;
  }
  login('B');
  if (change === 'away and back') login('A');
  const finishB = adapter.startCapture();
  adapter.print(['B-output']);
  expect(finishA()).toBe('');
  adapter.print(['B-after-A-finished']);
  expect(finishB()).toBe('B-output\nB-after-A-finished');
});
it.each(['same', 'B', 'away and back'])('drops saved exchange event callbacks during login: %s', async (change) => {
  const { login } = await sessionFixture();
  const { setupExchangeListeners } = await import('./exchange-listeners');
  const callbacks = new Map<string, (...args: unknown[]) => void>();
  const output = vi.fn(),
    recordFill = vi.fn();
  setupExchangeListeners(
    { emitter: { on: (name, fn) => callbacks.set(name, fn), off() {}, emit() {} } },
    {
      adapter: { printError: output, printSuccess: output },
      logger: { info: output },
      recordFill,
      isMonitorMode: () => true,
      getCurrentAccountName: () => 'Main',
      getHealthMonitor: () => undefined,
      addWatchMessage: output,
    },
    'Main',
  );
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
  }
  callbacks.get('exec-log')?.({ message: 'A-private' });
  callbacks.get('fill')?.({ orderId: 'A-order', amount: 1, symbol: 'BTC', price: 1 });
  callbacks.get('error')?.('A-error');
  callbacks.get('warning')?.('A-warning');
  expect(recordFill).toHaveBeenCalledTimes(change === 'same' ? 1 : 0);
  expect(output.mock.calls.length).toBe(change === 'same' ? 5 : 0);
});
