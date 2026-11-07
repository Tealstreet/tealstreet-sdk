import assert from 'node:assert/strict';
import { CandleCache } from '../../../packages/safe-cex/src/candleCache/CandleCache.ts';

const original = { setTimeout, clearTimeout, warn: console.warn, error: console.error };
const timers = new Map();
let timerId = 0;
let diagnostics = 0;
globalThis.setTimeout = callback => { const id = ++timerId; timers.set(id, callback); return id; };
globalThis.clearTimeout = id => timers.delete(id);
console.warn = console.error = () => { diagnostics++; };
const flush = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
try {
  for (const late of [false, true]) {
    let reject;
    let reads = 0;
    const observed = [];
    const completionOrder = [];
    const backend = {
      loadAllChunks: async () => {
        reads++;
        if (late && reads === 1) return await new Promise((_, fail) => { reject = fail; });
        throw Error('persisted key refused');
      },
      persistChunks: async () => {},
    };
    const cache = new CandleCache({ backend, maxConcurrentHydrations: 1, hydrationTimeoutMs: 10,
      onHydrationError: (...args) => observed.push(args) });
    const candle = { timestamp: 1699999800000, open: 1, high: 2, low: 0, close: 1, volume: 1 };
    const make = cacheKey => cache.fetch({ cacheKey, interval: '5m', fromMs: candle.timestamp,
      toMs: candle.timestamp, limit: 1, nowMs: candle.timestamp + 300000, fetcher: async () => [candle] });
    const first = make('first').then(value => { completionOrder.push('first'); return value; });
    const queued = make('queued').then(value => { completionOrder.push('queued'); return value; });
    await flush();
    if (late) {
      assert.equal(reads, 1);
      [...timers.values()][0]();
      await flush();
      reject(Error('persisted key refused'));
      await flush();
    }
    assert.deepEqual(await first, [candle]);
    assert.deepEqual(await queued, [candle]);
    assert.equal(reads, 2, 'failed hydration must release the cap');
    assert.deepEqual(completionOrder, ['first', 'queued'], 'caller settlement order changed');
    assert.deepEqual(observed, [], 'TS unexpectedly added a hydration diagnostic');
    assert.equal(diagnostics, 0, 'TS unexpectedly reported hydration rejection');
    timers.clear();
  }
} finally {
  globalThis.setTimeout = original.setTimeout;
  globalThis.clearTimeout = original.clearTimeout;
  console.warn = original.warn;
  console.error = original.error;
}
console.log('TS oracle: initial and late hydration errors are silent; slot release and network fallback pass');
