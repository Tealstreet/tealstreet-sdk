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

test('only MEXC and Orderly public replays install the virtual timer clock in either runtime', () => {
  const predicate = source.match(/^const publicReplayClock = .*;$/m)?.[0];
  assert.ok(predicate);
  const constructors = [...source.matchAll(/const clock = publicReplayClock \? new ReplayClock\(metadata\?\.epoch \?\? Date\.now\(\)\) : undefined;/g)];
  assert.equal(constructors.length, 2, 'both actual runtime constructors share the scoped predicate');
  for (const name of ['binance', 'bybit', 'okx', 'cryptocom', 'weex', 'bingx', 'mexc', 'blofin',
    'backpack', 'bitget', 'hyperliquid', 'bitrue', 'woo', 'lighter', 'orderly', 'tealstreet-dex', 'woofi']) {
    for (const isPrivate of [false, true]) {
      const venue = { orderlyProtocol: ['orderly', 'tealstreet-dex', 'woofi'].includes(name) };
      for (const constructor of constructors) {
        const clock = vm.runInNewContext(`${predicate}\n${constructor[0]}\nclock`, {
          values: { venue: name }, venue, isPrivate, metadata: { epoch: 123 },
          ReplayClock: class { constructor(epoch) { this.epoch = epoch; } },
        });
        const expected = !isPrivate && (name === 'mexc' || venue.orderlyProtocol);
        assert.equal(Boolean(clock), expected, `${name} private=${isPrivate}`);
        if (clock) assert.equal(clock.epoch, 123);
      }
    }
  }
});

test('recorded metadata options and testnet preserve credential and explicit config precedence', () => {
  const optionsSource = source.slice(source.indexOf('const replayOptions = '), source.indexOf('const credentials = '));
  const run = (isPrivate, config = {}) => vm.runInNewContext(`${optionsSource}\nreplayOptions`, {
    venue: { options: { testnet: false, applicationId: 'venue' },
      publicOptions: { key: 'public-key' }, replayCredentials: { key: 'private-key', secret: 'private-secret' } },
    metadata: { options: { applicationId: 'recorded', testnet: false, key: 'metadata-key' }, testnet: true },
    config, isPrivate, values: { venue: 'orderly' },
  });
  for (const isPrivate of [false, true]) {
    const options = run(isPrivate);
    assert.equal(options.applicationId, 'recorded');
    assert.equal(options.testnet, true);
    assert.equal(options.key, isPrivate ? 'private-key' : 'public-key');
    const explicit = run(isPrivate, { options: { applicationId: 'explicit', testnet: false } });
    assert.equal(explicit.applicationId, 'explicit');
    assert.equal(explicit.testnet, false);
  }
});
