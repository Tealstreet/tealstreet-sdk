import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';
import type { ScriptRecord } from '@tealstreet/script-api-core';
import type { ExchangeLike } from '@tealstreet/cli-core';
import type { OrderBook } from '@tealstreet/safe-cex/src/types';
import { deferred, sessionFixture } from './deferred-test.fixture';
vi.setConfig({ testTimeout: 30_000 });
vi.mock('./logger', () => ({
  logger: { info() {}, warn() {}, error() {} },
  originalConsoleLog() {},
  originalConsoleWarn() {},
  originalConsoleError() {},
}));
vi.mock('../../../packages/safe-cex/src/health/ExchangeHealthMonitor', () => ({
  ExchangeHealthMonitor: class {
    dispose() {}
  },
}));
beforeEach(() => {
  vi.resetModules();
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-scripts-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(() => {
  vi.unstubAllEnvs();
});
it.each(['same', 'B', 'away and back'])('checks script capture after the held body returns: %s', async (change) => {
  const { login } = await sessionFixture();
  const { FilePreprocessStore } = await import('./preprocess-store');
  const { ExchangeManager } = await import('./exchange-manager');
  const { createStandalonePlatformAdapter } = await import('./platform-adapter');
  const { createStandaloneCoreAdapter } = await import('./script-adapter');
  const { runScriptFromHostWithAdapter } = await import('./script-runner');
  const { CORE_API_VERSION } = await import('@tealstreet/script-api-core');
  const platform = createStandalonePlatformAdapter(),
    manager = new ExchangeManager(platform),
    store = new FilePreprocessStore();
  const entered = deferred(),
    wait = deferred();
  const record: ScriptRecord = {
    id: 'held',
    name: 'held',
    targets: ['core'],
    coreApiVersion: CORE_API_VERSION,
    capabilities: { accounts: [], writes: [], idempotent: true, storage: { enabled: false, quotaBytes: 0 } },
    fn: async (_args, api) => {
      entered.resolve();
      await wait.promise;
      api.io.return('A-result');
    },
  };
  let failure: string | undefined;
  const pending = runScriptFromHostWithAdapter(
    {
      adapter: createStandaloneCoreAdapter(
        manager,
        { currentAccountName: 'Main', currentSymbol: 'BTCUSDT', currentExchange: null },
        platform,
      ),
      transpile: async (text) => text,
    },
    { record, rawArgs: '', invokedFrom: 'cli', capture: { captureTo: 'result', preprocessStore: store } },
  ).catch((error) => {
    failure = String(error);
    return null;
  });
  await entered.promise;
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
  }
  store.setVar('result', 'current-result');
  wait.resolve();
  const result = await pending;
  if (change === 'same') expect(failure).toBeUndefined();
  expect(store.getVar('result')).toBe(change === 'same' ? 'A-result' : 'current-result');
  expect(result?.ok ?? false).toBe(change === 'same');
  await manager.stopAll();
});
it.each(['same', 'B', 'away and back'])('checks first snapshot, late messages and idle cleanup: %s', async (change) => {
  const { login } = await sessionFixture();
  const { ScriptOrderbookCache } = await import('./script-orderbook-cache');
  vi.useFakeTimers();
  const callbacks: Array<(book: OrderBook) => void> = [];
  let current: ExchangeLike = {
    name: 'bybit',
    emitter: { on() {}, off() {}, emit() {} },
    listenOrderBook: (_symbol, callback) => {
      callbacks.push(callback);
      return vi.fn();
    },
  };
  const cache = new ScriptOrderbookCache(
    { getExchange: () => current },
    { idleTtlMs: 100, firstSnapshotTimeoutMs: 50 },
  );
  const old = cache.get('BTCUSDT', { account: 'Main' }).catch(() => undefined);
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
    current = { ...current };
  }
  const own = change === 'same' ? old : cache.get('BTCUSDT', { account: 'Main' });
  const a = { bids: [{ price: 1, amount: 1, total: 1 }], asks: [] },
    b = { bids: [{ price: 2, amount: 1, total: 2 }], asks: [] };
  callbacks[0](a);
  if (change !== 'same') callbacks[1](b);
  expect(await old).toEqual(change === 'same' ? a : undefined);
  expect(await own).toEqual(change === 'same' ? a : b);
  await vi.advanceTimersByTimeAsync(100);
  cache.disposeAll();
  vi.useRealTimers();
});
it.each(['same', 'B', 'away and back'])(
  'does not publish a script imported across a login transition: %s',
  async (change) => {
    const { login } = await sessionFixture();
    const scripts = await import('./scripts-store');
    scripts.ensureScriptsDir();
    const file = join(scripts.getScriptsDir(), 'held.ts');
    writeFileSync(file, 'export default async function() {}');
    const entered = deferred(),
      wait = deferred(),
      url = `${pathToFileURL(file).href}?t=1`;
    vi.doMock(url, async () => {
      entered.resolve();
      await wait.promise;
      return { meta: { targets: ['cli'], capabilities: {} }, default: async () => {} };
    });
    const loading = scripts.loadScripts().catch(() => []);
    await entered.promise;
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
    }
    wait.resolve();
    await loading;
    expect(!!scripts.lookupScript('held')).toBe(change === 'same');
    vi.doUnmock(url);
  },
);
