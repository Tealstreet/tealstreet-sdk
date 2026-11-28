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
        failedSteps: new WeakSet(),
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
const chainFixture = async (steps: { command: string; awaitPrior?: boolean; block?: boolean }[], failing: string) => {
  const { storage } = await sessionFixture();
  const failedSteps = new WeakSet<object>();
  const dispatched: string[] = [];
  const errors: string[] = [];
  const run = evaluate<(input: string) => Promise<boolean>>(
    `${sourceFunction('repl.ts', 'processCommandInSession')}\nprocessCommandInSession;`,
    {
      ...storage,
      getAdapterContext: () => undefined,
      session: {},
      preprocessStore: {},
      adapter: { printError: (message: string) => void errors.push(message) },
      preprocessCommand: () => ({ warnings: [], steps }),
      runWithAuditRunId: (_id: string, fn: () => Promise<void>) => fn(),
      newRunId: () => 'fixture',
      failedSteps,
      runStep: async (step: { command: string }) => {
        dispatched.push(step.command);
        // reportStepError's contract: a failed, non-`set ec` step enters the set.
        if (step.command === failing) failedSteps.add(step);
      },
    },
  );
  const succeeded = await storage.runInStorageSession(() => run('fixture'));
  return { succeeded, dispatched, errors };
};

it('stops the reported chain when its margin-mode step fails, before the entry', async () => {
  const { succeeded, dispatched, errors } = await chainFixture(
    [
      { command: 'symbol BTCUSDT' },
      { command: 'lev 50 isolated', awaitPrior: true },
      { command: 'buy BTCUSDT $500 at 75800.0 stop 75700.0 tp 76100.0', awaitPrior: true },
    ],
    'lev 50 isolated',
  );
  expect(succeeded).toBe(false);
  expect(dispatched).toEqual(['symbol BTCUSDT', 'lev 50 isolated']);
  expect(errors).toEqual(['Chain stopped after a failed step. Not run: buy BTCUSDT $500 at 75800.0 stop 75700.0 tp 76100.0']);
});

it('stops after a failed awaited step even before a parallel step', async () => {
  const { succeeded, dispatched } = await chainFixture(
    [{ command: 'sleep 1', block: true }, { command: 'buy $500' }],
    'sleep 1',
  );
  expect(succeeded).toBe(false);
  expect(dispatched).toEqual(['sleep 1']);
});

it('reports success when every step succeeds', async () => {
  const { succeeded, dispatched, errors } = await chainFixture(
    [{ command: 'lev 50' }, { command: 'buy $500', awaitPrior: true }],
    'none',
  );
  expect(succeeded).toBe(true);
  expect(dispatched).toEqual(['lev 50', 'buy $500']);
  expect(errors).toEqual([]);
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
