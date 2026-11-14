import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { CLISession, ExchangeLike, ExchangeAccountConfig } from '@tealstreet/cli-core';
import { deferred, evaluate, sessionFixture, sourceFunction } from './deferred-test.fixture';
vi.mock('./logger', () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
  originalConsoleLog: vi.fn(),
  originalConsoleWarn: vi.fn(),
  originalConsoleError: vi.fn(),
  isStdoutReservedForJson: () => false,
  writeAbovePrompt: (run: () => void) => run(),
}));
vi.mock('../../../packages/safe-cex/src/health/ExchangeHealthMonitor', () => ({
  ExchangeHealthMonitor: class {
    dispose() {}
  },
}));
beforeEach(() => {
  vi.resetModules();
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-deferred-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(() => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});
const switches = ['same', 'B', 'away and back'];
function exchange(): ExchangeLike {
  return { name: 'bybit', emitter: { on() {}, off() {}, emit() {} }, dispose: vi.fn() };
}
it.each(switches)('does not install an old account after the actual start await: %s', async (change) => {
  const { storage, config, login } = await sessionFixture();
  const { createStandalonePlatformAdapter } = await import('./platform-adapter');
  const { ExchangeManager } = await import('./exchange-manager');
  const { AccountCommand } = await import('@tealstreet/cli-core/src/commands/account/AccountCommand');
  const audit = await import('./audit-log');
  const adapter = createStandalonePlatformAdapter(),
    manager = new ExchangeManager(adapter);
  adapter.setExchangeManager(manager);
  const account: ExchangeAccountConfig = {
    name: 'Main',
    exchange: 'bybit',
    apiKey: 'synthetic',
    apiSecret: 'synthetic',
  };
  config.addAccount(account);
  const a = exchange(),
    b = exchange(),
    wait = deferred(),
    entered = deferred();
  a.start = async () => {
    entered.resolve();
    await wait.promise;
  };
  vi.spyOn(adapter, 'createExchange').mockResolvedValue(a);
  const session: CLISession = { currentSymbol: 'BTCUSDT', currentAccountName: undefined, currentExchange: null };
  const unsubscribe = storage.onStorageOwnerChange(() => {
    session.currentExchange = null;
    session.currentAccountName = '';
    session.currentSymbol = '';
  });
  const run = evaluate<(input: string) => Promise<void>>(
    `${sourceFunction('repl.ts', 'processSingleCommand')}\nprocessSingleCommand;`,
    {
      ...storage,
      session,
      adapter,
      exchangeManager: manager,
      promptConfig: {},
      preprocessStore: {},
      logger: { info() {} },
      getAdapterContext: () => undefined,
      recordWriteOp: audit.recordWriteOp,
      setupExchangeListeners() {},
      exchangeListenerContext: {},
      networkMonitor: null,
      cliExecuteCommand: async (raw: string) => {
        const result = await new AccountCommand().execute({
          raw,
          args: { _: raw.split(' ') },
          symbol: 'BTCUSDT',
          accountName: undefined,
          exchange: null,
          adapter,
        });
        return {
          handled: true,
          success: result.success,
          sessionChanged: !!result.updateSession,
          newSession: result.updateSession && {
            currentExchange: result.updateSession.exchange,
            currentAccountName: result.updateSession.accountName,
          },
        };
      },
    },
  );
  const pending = storage.runInStorageSession(() => run('account Main')).catch(() => undefined);
  await entered.promise;
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
    session.currentExchange = b;
    session.currentAccountName = 'B-main';
  }
  audit.isAuditEnabled();
  wait.resolve();
  await pending;
  expect(session.currentExchange).toBe(change === 'same' ? a : b);
  expect(a.dispose).toHaveBeenCalledTimes(change === 'same' ? 0 : 1);
  unsubscribe();
  await manager.stopAll();
});
it.each(switches)('checks an old audit mutation before its cache changes: %s', async (change) => {
  const { storage, login } = await sessionFixture();
  const audit = await import('./audit-log');
  const wait = deferred();
  const pending = storage
    .runInStorageSession(async () => {
      await wait.promise;
      audit.setAuditEnabled(false);
    })
    .catch(() => undefined);
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
  }
  audit.setAuditEnabled(true);
  wait.resolve();
  await pending;
  expect(audit.isAuditEnabled()).toBe(change !== 'same');
  expect(JSON.parse(readFileSync(storage.getStoragePath('audit.json'), 'utf8'))).toEqual({
    enabled: change !== 'same',
  });
});
it.each(switches)('binds updater immediate and fetch work to its creation: %s', async (change) => {
  const { storage, config, login } = await sessionFixture();
  const aPath = storage.getStoragePath('update-check.json');
  writeFileSync(
    aPath,
    JSON.stringify({ lastCheck: 0, latestVersion: '0.0.1', notifiedVersion: 'A-old', releaseNotes: 'A-notes' }),
  );
  const wait = deferred();
  let run = async () => {};
  const start = evaluate<() => void>(
    ['getUpdateState', 'saveUpdateState', 'checkForUpdatesBackground']
      .map((n) => sourceFunction('updater.ts', n))
      .join('\n') + '\ncheckForUpdatesBackground;',
    {
      ...storage,
      existsSync,
      readFileSync,
      writeFileSync,
      getConfigDir: config.getConfigDir,
      UPDATE_CHECK_FILE: () => storage.getStoragePath('update-check.json'),
      CHECK_INTERVAL_MS: 86_400_000,
      setImmediate: (fn: () => Promise<void>) => {
        run = fn;
      },
      fetchLatestVersion: async () => {
        await wait.promise;
        return null;
      },
    },
  );
  start();
  const pending = run();
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
  }
  const current = storage.getStoragePath('update-check.json'),
    before = existsSync(current) ? readFileSync(current, 'utf8') : null;
  wait.resolve();
  await pending;
  if (change === 'same') expect(JSON.parse(readFileSync(aPath, 'utf8')).lastCheck).toBeGreaterThan(0);
  else expect(existsSync(current) ? readFileSync(current, 'utf8') : null).toBe(before);
});
it.each(switches)('keeps daemon shutdown out of the new audit owner: %s', async (change) => {
  const { storage, login } = await sessionFixture();
  const audit = await import('./audit-log');
  login('B');
  audit.setAuditEnabled(true);
  const bPath = audit.getAuditLogPath();
  login('A');
  audit.setAuditEnabled(true);
  const aPath = audit.getAuditLogPath();
  const entered = deferred(),
    handlers = new Map<string, () => void>();
  class Manager {
    start() {}
    stop() {}
  }
  class ExchangeManager {
    getExchange() {}
    async stopAll() {}
  }
  const identity = (text: string) => text;
  const run = evaluate<(opts: { configs: unknown[] }) => Promise<void>>(
    'let currentContext = null;\n' +
      ['dropChaseConfigs', 'collectAccountsToBoot', 'crossTradeCliPlaceOrder', 'runCrossTradeDaemon']
        .map((n) => sourceFunction('cross-trade-daemon.ts', n))
        .join('\n') +
      '\nrunCrossTradeDaemon;',
    {
      ...storage,
      ExchangeManager,
      CrossTradeManager: Manager,
      console: { log() {} },
      logger: { warn() {} },
      chalk: { gray: identity, cyan: identity, yellow: identity, red: identity, green: identity },
      G: {},
      BRAND: {},
      process: { once: (signal: string, fn: () => void) => handlers.set(signal, fn) },
      createStandalonePlatformAdapter: () => ({ setExchangeManager() {} }),
      listAccounts: () => [],
      recordCrossTradeEvent: audit.recordCrossTradeEvent,
      writeHeartbeat: () => entered.resolve(),
      clearHeartbeat() {},
      HEARTBEAT_INTERVAL_MS: 5000,
      STATUS_INTERVAL_MS: 30000,
      setInterval: () => 1,
      clearInterval() {},
    },
  );
  const pending = run({
    configs: [{ masterAccount: 'Main', copiers: [{ account: 'Main', enabled: true, execution: 'Market' }] }],
  });
  await entered.promise;
  if (change === 'same') handlers.get('SIGINT')?.();
  else {
    login('B');
    if (change === 'away and back') login('A');
  }
  await pending;
  expect(existsSync(bPath) ? readFileSync(bPath, 'utf8') : '').not.toContain('[cross-trade] stop');
  if (change === 'same') expect(readFileSync(aPath, 'utf8')).toContain('[cross-trade] stop');
});
it.each(['same', 'B', 'away and back'])(
  'disposes an old start settling after its connection timeout: %s',
  async (change) => {
    vi.useFakeTimers();
    try {
      const { login } = await sessionFixture();
      const { createStandalonePlatformAdapter } = await import('./platform-adapter');
      const adapter = createStandalonePlatformAdapter();
      const a = exchange(),
        wait = deferred();
      a.start = async () => {
        await wait.promise;
      };
      const pending = adapter.startExchange(a).catch((error) => error);
      await vi.advanceTimersByTimeAsync(30_000);
      expect((await pending).message).toContain('timed out');
      if (change !== 'same') {
        login('B');
        if (change === 'away and back') login('A');
      }
      wait.resolve();
      await Promise.resolve();
      await Promise.resolve();
      expect(a.dispose).toHaveBeenCalledTimes(change === 'same' ? 0 : 1);
    } finally {
      vi.useRealTimers();
    }
  },
);
