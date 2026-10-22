import { test } from 'node:test';
import assert from 'node:assert/strict';
import { withoutMexcBookIngress, assertMexcDecoderControl, runMissingFeatureControl } from './capability-feature-control.mjs';

test('mutant removes exactly one decoder opt-in and preserves other dependency features', () => {
  const source = 'safe-cex-engine = { workspace = true, features = ["order-locks", "command-order-book"] }';
  assert.equal(withoutMexcBookIngress(source), 'safe-cex-engine = { workspace = true, features = ["order-locks"] }');
  assert.throws(() => withoutMexcBookIngress('other manifest'));
  assert.throws(() => withoutMexcBookIngress(source + source));
});

test('the expected failure must be the advertised isolated missing decoder', () => {
  let freed = 0;
  const fake = (advertised, action) => class {
    capabilities() { return JSON.stringify({ orderBookQuery: advertised }); }
    command() { return action(); }
    free() { freed++; }
  };
  assertMexcDecoderControl(fake(true, () => { throw new Error('unknown variant `fetchOrderBook`, expected others'); }));
  assert.throws(() => assertMexcDecoderControl(fake(false, () => { throw new Error('unknown variant `fetchOrderBook`'); })));
  assert.throws(() => assertMexcDecoderControl(fake(true, () => '[]')));
  assert.throws(() => assertMexcDecoderControl(fake(true, () => { throw new Error('crypto unavailable'); })));
  assert.equal(freed, 4);
});

test('local release builds are refused before any subprocess starts', () => {
  if (process.env.GITHUB_ACTIONS !== 'true') assert.throws(() => runMissingFeatureControl(), /only in CI/);
});
