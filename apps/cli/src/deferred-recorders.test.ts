import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import type { OrderBook } from '@tealstreet/safe-cex/src/types';
import { deferred, sessionFixture } from './deferred-test.fixture';
const transport = vi.hoisted((): { create: () => Promise<PublicExchange> } => ({
  create: async () => new PublicExchange(),
}));
const observed = vi.hoisted(
  (): { owner: () => string | null; effects: Array<{ owner: string | null; kind: string }> } => ({
    owner: () => null,
    effects: [],
  }),
);
const effects = observed.effects;
vi.mock('./logger', () => ({ logger: { info() {}, warn() {}, error() {} } }));
vi.mock('@tealstreet/safe-cex/src/index', () => ({ createExchange: () => transport.create() }));
vi.mock('./record/database', () => {
  const effect = (kind: string) => {
    observed.effects.push({ owner: observed.owner(), kind });
  };
  return {
    cleanupOldData: () => {
      effect('cleanup');
      return 0;
    },
    upsertRecording: () => effect('config'),
    deactivateRecording: () => effect('deactivate'),
    insertOrderbookSnapshot: () => effect('snapshot'),
    countOrderbookSnapshots: () => 0,
  };
});
class PublicExchange {
  callback: (book: OrderBook) => void = () => {};
  start = async () => {};
  dispose = vi.fn();
  listenOrderBook(_symbol: string, callback: (book: OrderBook) => void) {
    this.callback = callback;
    return vi.fn();
  }
}
beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers();
  effects.length = 0;
  transport.create = async () => new PublicExchange();
  const root = mkdtempSync(join(tmpdir(), 'cli-owner-recorder-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(async () => {
  await (await import('./record/recorders')).stopAllRecorders();
  vi.useRealTimers();
  vi.unstubAllEnvs();
});
const options = {
  dataType: 'orderbook',
  exchange: 'bybit',
  symbol: 'BTCUSDT',
  intervalMs: 1,
  retentionDays: 1,
} satisfies import('./record/types').RecordingOptions;
it.each(['create', 'start'].flatMap((wait) => ['same', 'B', 'away and back'].map((change) => ({ wait, change }))))(
  'fences recorder $wait: $change',
  async ({ wait, change }) => {
    const { login, storage } = await sessionFixture(),
      ready = deferred(),
      held = deferred(),
      exchange = new PublicExchange();
    observed.owner = storage.getStorageOwner;
    const recorders = await import('./record/recorders');
    const pause = async () => {
      ready.resolve();
      await held.promise;
    };
    if (wait === 'start') exchange.start = pause;
    transport.create = async () => {
      if (wait === 'create') await pause();
      return exchange;
    };
    const pending = recorders.startRecorder(options).catch(() => null);
    await ready.promise;
    if (change !== 'same') {
      login('B');
      if (change === 'away and back') login('A');
    }
    held.resolve();
    const result = await pending;
    expect(!!result).toBe(change === 'same');
    if (change !== 'same') {
      expect(effects).toEqual([]);
      expect(exchange.dispose).toHaveBeenCalled();
      expect(recorders.getActiveRecorders()).toEqual([]);
    } else expect(effects).toContainEqual({ owner: 'A', kind: 'config' });
  },
);
it.each(
  ['stop', 'stop-exchange', 'replace'].flatMap((wait) =>
    ['same', 'B', 'away and back'].map((change) => ({ wait, change })),
  ),
)('does not remove another login recorder after $wait: $change', async ({ wait, change }) => {
  const { login, storage } = await sessionFixture(),
    recorders = await import('./record/recorders');
  observed.owner = storage.getStorageOwner;
  const old = await recorders.startRecorder(options),
    held = deferred(),
    ready = deferred();
  const stop = old.recorder.stop.bind(old.recorder);
  vi.spyOn(old.recorder, 'stop').mockImplementationOnce(async () => {
    ready.resolve();
    await held.promise;
    await stop();
  });
  const pending = (
    wait === 'stop'
      ? recorders.stopRecorder('orderbook', 'bybit', 'BTCUSDT')
      : wait === 'stop-exchange'
        ? recorders.stopExchangeRecorders('bybit')
        : recorders.startRecorder(options)
  ).catch(() => null);
  await ready.promise;
  let own = old;
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
    own = await recorders.startRecorder(options);
  }
  effects.length = 0;
  held.resolve();
  await pending;
  if (change !== 'same') {
    expect(recorders.getRecorder('orderbook', 'bybit', 'BTCUSDT')).toBe(own.recorder);
    expect(effects).toEqual([]);
  } else if (wait === 'replace') expect(recorders.getActiveRecorders()).toHaveLength(1);
  else expect(recorders.getActiveRecorders()).toEqual([]);
});
it.each(['same', 'B', 'away and back'])('fences queued recorder messages and cleanup timers: %s', async (change) => {
  const { login, storage } = await sessionFixture(),
    recorders = await import('./record/recorders'),
    a = new PublicExchange(),
    b = new PublicExchange();
  observed.owner = storage.getStorageOwner;
  transport.create = async () => a;
  await recorders.startRecorder(options);
  const oldMessage = a.callback;
  if (change !== 'same') {
    login('B');
    if (change === 'away and back') login('A');
    transport.create = async () => b;
    await recorders.startRecorder(options);
  }
  effects.length = 0;
  oldMessage({ bids: [{ price: 1, amount: 1, total: 1 }], asks: [] });
  await vi.advanceTimersByTimeAsync(3_600_000);
  expect(effects.filter((entry) => entry.kind === 'snapshot')).toEqual(
    change === 'same' ? [{ owner: 'A', kind: 'snapshot' }] : [],
  );
  expect(effects.filter((entry) => entry.kind === 'cleanup')).toHaveLength(1);
  expect(effects.every((entry) => entry.owner === (change === 'B' ? 'B' : 'A'))).toBe(true);
});
