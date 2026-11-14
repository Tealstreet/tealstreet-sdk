import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { deferred, evaluate, sessionFixture, sourceFunction } from './deferred-test.fixture';
beforeEach(() => {
  vi.resetModules();
  const root = mkdtempSync(join(tmpdir(), 'cli-deferred-runner-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(() => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});
const switches = ['same', 'B', 'away and back'];
it.each(switches)('does not retry a failed A attempt after switching during its await: %s', async (change) => {
  const { storage, login } = await sessionFixture();
  const { runCommandWithRetries } = await import('./command-outcome');
  const wait = deferred();
  const run = vi.fn(async () => {
    if (run.mock.calls.length === 1) {
      await wait.promise;
      throw new Error('retryable fixture');
    }
  });
  const pending = storage.runInStorageSession(() => runCommandWithRetries(run, 1)).catch((error) => error);
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
  }
  wait.resolve();
  const result = await pending;
  expect(run).toHaveBeenCalledTimes(change === 'same' ? 2 : 1);
  if (change !== 'same') expect(result).toBeInstanceOf(storage.StorageSessionCancelledError);
});
it.each(['barrier', 'block', 'retry', 'capture'])('fences the next chain step after a held %s', async (boundary) => {
  for (const change of switches) {
    const { storage, login } = await sessionFixture();
    const wait = deferred(),
      entered = deferred();
    const dispatched: string[] = [];
    const first = {
      command: 'first',
      awaitPrior: false,
      block: boundary === 'block',
      retry: boundary === 'retry' ? 1 : undefined,
      captureTo: boundary === 'capture' ? 'ids' : undefined,
    };
    const run = evaluate<(input: string) => Promise<void>>(
      `${sourceFunction('repl.ts', 'processCommandInSession')}\nprocessCommandInSession;`,
      {
        ...storage,
        getAdapterContext: () => undefined,
        session: {},
        preprocessStore: {},
        preprocessCommand: () => ({
          warnings: [],
          steps: [first, { command: 'second', awaitPrior: boundary === 'barrier' }],
        }),
        runWithAuditRunId: (_id: string, fn: () => Promise<void>) => fn(),
        newRunId: () => 'fixture',
        runStep: async (step: { command: string }) => {
          dispatched.push(step.command);
          if (step.command === 'first') {
            entered.resolve();
            await wait.promise;
          }
        },
      },
    );
    const pending = storage.runInStorageSession(() => run('fixture')).catch((error) => error);
    await entered.promise;
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
    }
    wait.resolve();
    await pending;
    expect(dispatched, change).toEqual(change === 'same' ? ['first', 'second'] : ['first']);
  }
});
it.each(switches)('drops an updater immediate created before a login change: %s', async (change) => {
  const { storage, login } = await sessionFixture();
  let run = async () => {};
  const fetchLatestVersion = vi.fn(async () => null),
    saveUpdateState = vi.fn();
  const start = evaluate<() => void>(
    `${sourceFunction('updater.ts', 'checkForUpdatesBackground')}\ncheckForUpdatesBackground;`,
    {
      ...storage,
      setImmediate: (fn: () => Promise<void>) => {
        run = fn;
      },
      getUpdateState: () => ({ lastCheck: 0 }),
      CHECK_INTERVAL_MS: 1,
      fetchLatestVersion,
      saveUpdateState,
    },
  );
  start();
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
  }
  await run();
  expect(fetchLatestVersion).toHaveBeenCalledTimes(change === 'same' ? 1 : 0);
  expect(saveUpdateState).toHaveBeenCalledTimes(change === 'same' ? 1 : 0);
});
