import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { sessionFixture } from './deferred-test.fixture';
const observed = vi.hoisted((): { changes: Array<() => void> } => ({ changes: [] }));
vi.mock('fs', async (original) => ({
  ...(await original<typeof import('fs')>()),
  watch: (_path: string, _opts: unknown, change: () => void) => {
    observed.changes.push(change);
    return { close() {} };
  },
}));
vi.mock('./logger', () => ({ logger: { info() {}, warn() {} } }));
beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  observed.changes.length = 0;
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-broadcast-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
  vi.stubGlobal('Bun', { hash: (input: string) => input });
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});
const cases = ['accounts', 'tasks', 'cross-trade'].flatMap((kind) =>
  [
    'watch',
    'debounce',
    ...(kind === 'tasks' ? [] : ['notify']),
    ...(kind === 'cross-trade' ? ['staleness'] : []),
  ].flatMap((wait) => ['same', 'B', 'away and back', 'stopped'].map((change) => ({ kind, wait, change }))),
);
it.each(cases)('drops stale $kind $wait work: $change', async ({ kind, wait, change }) => {
  const { config, login } = await sessionFixture();
  const tasks = await import('./tasks-store'),
    cross = await import('./cross-trade-store'),
    heartbeat = await import('./cross-trade-heartbeat');
  const seed = (name: string) => {
    config.addAccount({ name, exchange: 'bybit', apiKey: 'synthetic', apiSecret: 'synthetic' });
    tasks.upsertTask({
      id: name,
      type: 'fixture',
      status: 'running',
      createdAt: '2026-01-01',
      params: {},
      progress: {},
    });
    cross.upsertCrossTradeConfig(
      cross.buildSingleCopierConfig({ masterAccount: name, copierAccount: 'copy', enabled: false }),
    );
    heartbeat.writeHeartbeat({ startedAt: 1, configCount: config.listAccounts().length });
  };
  seed('A');
  const publish = vi.fn(() => 1);
  const handle: { stop(): void; notifyOwnWrite?(): void } =
    kind === 'accounts'
      ? (await import('./listener/account-broadcast')).startAccountsBroadcast({ publish })
      : kind === 'tasks'
        ? (await import('./listener/tasks-broadcast')).startTasksBroadcast({ publish })
        : (await import('./listener/cross-trade-broadcast')).startCrossTradeBroadcast({ publish });
  if (wait === 'debounce') observed.changes[0]();
  if (change === 'B' || change === 'away and back') {
    login('B');
    if (change === 'away and back') login('A');
  }
  if (change === 'stopped') handle.stop();
  seed('changed');
  if (wait === 'watch') observed.changes[0]();
  if (wait === 'notify') handle.notifyOwnWrite?.();
  await vi.advanceTimersByTimeAsync(wait === 'staleness' ? 5000 : 300);
  expect(publish.mock.calls.length > 0).toBe(change === 'same');
  handle.stop();
});
