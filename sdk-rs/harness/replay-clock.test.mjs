import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
const clockSource = source.slice(source.indexOf('let clockStart = '), source.indexOf('const config = '));
const resets = [...source.matchAll(/if \(isPrivate\) clockStart = performance\.now\(\);/g)];

// Run the actual clock setup without loading engines, fixtures, or exchange workers.
test('recorded epochs cover freshness checks outside MEXC and reset after private startup', () => {
  assert.equal(resets.length, 2, 'Rust and TS both reset the private replay epoch');
  const realNow = Date.now;
  for (const venue of ['lighter', 'woo', 'mexc', 'okx']) {
    for (const epoch of [0, 1_700_000_000_000]) {
      let elapsed = 100;
      const context = vm.createContext({ metadata: { epoch }, values: { venue },
        Date: { now: realNow }, performance: { now: () => elapsed }, isPrivate: true });
      vm.runInContext(clockSource, context);
      assert.equal(context.Date.now(), epoch);
      elapsed += 4_999.75;
      assert.equal(context.Date.now(), epoch + 4_999);
      assert.ok(context.Date.now() - epoch < 5_000, 'a taped recent fill stays fresh');
      elapsed += 10_000;
      for (const reset of resets) {
        vm.runInContext(reset[0], context);
        assert.equal(context.Date.now(), epoch, 'startup time is excluded from taped freshness');
        elapsed += 123.9;
        assert.equal(context.Date.now(), epoch + 123);
      }
    }
  }
  assert.equal(Date.now, realNow, 'the test never replaces the process clock');
});

test('missing or nonfinite recorded epochs preserve the real clock', () => {
  const realNow = Date.now;
  for (const metadata of [undefined, {}, { epoch: null }, { epoch: '1700000000000' },
    { epoch: NaN }, { epoch: Infinity }, { epoch: -Infinity }]) {
    const context = vm.createContext({ metadata, values: { venue: 'lighter' },
      Date: { now: realNow }, performance: { now: () => 42 } });
    vm.runInContext(clockSource, context);
    assert.equal(context.Date.now, realNow);
  }
});
