import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { deferred, sessionFixture } from './deferred-test.fixture';
vi.setConfig({ testTimeout: 30_000 });
vi.mock('./logger', () => ({ logger: { info() {}, warn() {} } }));
beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-alias-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(async () => {
  (await import('./alias-sync')).stopAliasSync();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});
it.each(
  ['periodic', 'change', 'snapshot', 'upload'].flatMap((wait) =>
    ['same', 'B', 'away and back'].map((change) => ({ wait, change })),
  ),
)('fences alias $wait deferral: $change', async ({ wait, change }) => {
  const { storage, login } = await sessionFixture();
  const { FilePreprocessStore } = await import('./preprocess-store');
  const sync = await import('./alias-sync'),
    store = new FilePreprocessStore();
  const blocked = deferred(),
    entered = deferred();
  let active = false,
    stage = 0;
  const sent: Array<{ owner: string | null; changes: unknown }> = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (_url: string, opts: RequestInit) => {
      const owner = storage.getStorageOwner();
      sent.push({ owner, changes: JSON.parse(String(opts.body)).changes });
      stage++;
      if (active && ((wait === 'snapshot' && stage === 1) || (wait === 'upload' && stage === 2))) {
        entered.resolve();
        await blocked.promise;
      }
      return new Response(JSON.stringify({ revision: stage, entries: [] }), { status: 200 });
    }),
  );
  store.setAlias('fixture', 'A-command');
  if (wait === 'snapshot' || wait === 'upload') {
    active = true;
    sync.startAliasSync(store, vi.fn());
    await entered.promise;
  } else {
    sync.startAliasSync(store, vi.fn());
    await sync.syncAliasesNow();
    if (wait === 'change') store.setAlias('held', 'A-held');
  }
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
  }
  const before = sent.length;
  if (active) {
    blocked.resolve();
    await Promise.resolve();
    await Promise.resolve();
  } else await vi.advanceTimersByTimeAsync(wait === 'periodic' ? 300_000 : 2000);
  await sync.syncAliasesNow();
  expect(
    sent
      .filter((entry) => entry.owner !== 'A')
      .every(
        (entry) =>
          !JSON.stringify(entry.changes).includes('A-command') && !JSON.stringify(entry.changes).includes('A-held'),
      ),
  ).toBe(true);
  if (change === 'same' && wait !== 'upload') expect(sent.length).toBeGreaterThan(before);
  if (change === 'same' && wait === 'upload') expect(store.getAliasSyncState().cursor).not.toBeNull();
  expect(store.getAlias('fixture')).toBe(change === 'B' ? undefined : 'A-command');
});
it.each(['periodic', 'change'].flatMap((wait) => ['same', 'B', 'away and back'].map((change) => ({ wait, change }))))(
  'does not run a retained alias $wait timer in another generation: $change',
  async ({ wait, change }) => {
    const { login } = await sessionFixture();
    const { FilePreprocessStore } = await import('./preprocess-store');
    const sync = await import('./alias-sync'),
      store = new FilePreprocessStore();
    const fetch = vi.fn(async () => new Response(JSON.stringify({ revision: 1, entries: [] }), { status: 200 }));
    vi.stubGlobal('fetch', fetch);
    const timer = vi.spyOn(globalThis, 'setTimeout');
    sync.startAliasSync(store, vi.fn());
    await sync.syncAliasesNow();
    if (wait === 'change') store.setAlias('held', 'A-body');
    const callback = timer.mock.calls.find((call) => call[1] === (wait === 'periodic' ? 300_000 : 2000))?.[0];
    expect(typeof callback).toBe('function');
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
      await sync.syncAliasesNow();
    }
    const before = fetch.mock.calls.length;
    if (typeof callback === 'function') await callback();
    await vi.advanceTimersByTimeAsync(0);
    expect(fetch.mock.calls.length - before).toBe(change === 'same' ? 1 : 0);
    timer.mockRestore();
  },
);
