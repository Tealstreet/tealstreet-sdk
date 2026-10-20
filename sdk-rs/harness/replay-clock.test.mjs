import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { ReplayClock } from './tape.mjs';
import { venueOf } from './venues.mjs';
import { deterministicReplay } from './polling.mjs';

const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
const clockSource = source.slice(source.indexOf('let clockStart = '), source.indexOf('const config = '));
const resets = [...source.matchAll(/if \(isPrivate\) clockStart = performance\.now\(\);/g)];

// Run the actual clock setup without loading engines, fixtures, or exchange workers.
test('recorded epochs keep signer and freshness clocks continuous through private startup', () => {
  assert.equal(resets.length, 0, 'neither runtime may rebase Date.now at readiness');
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
      assert.equal(context.Date.now(), epoch + 14_999, 'readiness cannot rewind the signing/freshness epoch');
      elapsed += 123.9;
      assert.equal(context.Date.now(), epoch + 15_123);
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

test('public, polling and opted-in private replays share deterministic clocks in either runtime', () => {
  const predicate = source.match(/^const publicReplayClock = .*;$/m)?.[0];
  assert.ok(predicate);
  const constructors = [...source.matchAll(/const clock = publicReplayClock \? new ReplayClock\(metadata\?\.epoch \?\? Date\.now\(\), isPrivate && venue\.virtualPrivateClock === true\) : undefined;/g)];
  assert.equal(constructors.length, 2, 'both actual runtime constructors share the scoped predicate');
  for (const name of ['binance', 'bybit', 'okx', 'cryptocom', 'weex', 'bingx', 'mexc', 'blofin',
    'backpack', 'bitget', 'hyperliquid', 'bitrue', 'woo', 'lighter', 'orderly', 'tealstreet-dex', 'woofi', 'breakoutprop', 'coinbase', 'grvt', 'polymarket', 'txflow']) {
    for (const isPrivate of [false, true]) {
      const venue = { rust: name, polling: name === 'breakoutprop', virtualPrivateClock: name === 'txflow', orderlyProtocol: ['orderly', 'tealstreet-dex', 'woofi'].includes(name) };
      for (const constructor of constructors) {
        const clock = vm.runInNewContext(`${predicate}\n${constructor[0]}\nclock`, {
          deterministicReplay, values: { venue: name }, venue, isPrivate, metadata: { epoch: 123 },
          ReplayClock: class { constructor(epoch, privateClock) { this.epoch = epoch; this.privateClock = privateClock; } },
        });
        const expected = !isPrivate || name === 'breakoutprop' || name === 'txflow';
        assert.equal(Boolean(clock), expected, `${name} private=${isPrivate}`);
        if (clock) {
          assert.equal(clock.epoch, 123);
          assert.equal(clock.privateClock, isPrivate && name === 'txflow');
        }
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
    config, isPrivate, deterministicReplay, values: { venue: 'orderly' },
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


test('both Bitrue replay clocks activate subscribed fallback depth after its protocol delay', async () => {
  const predicate = source.match(/^const publicReplayClock = .*;$/m)[0];
  const constructors = [...source.matchAll(/const clock = publicReplayClock \? new ReplayClock\(metadata\?\.epoch \?\? Date\.now\(\), isPrivate && venue\.virtualPrivateClock === true\) : undefined;/g)];
  assert.equal(constructors.length, 2);
  const url = 'wss://fmarket-ws.bitrue.com/kline-api/ws';
  const venue = venueOf('bitrue');
  const primary = 'market_e_btcusdt_simple_depth_step0';
  const fallback = 'market_e_btcusdt_depth_step0';
  const frame = JSON.stringify({ channel: fallback, tick: { buys: [[100, 2]], asks: [[101, 3]] } });
  const subscribe = (channel) => JSON.stringify({ event: 'sub', params: { channel } });
  for (const constructor of constructors) {
    const clock = vm.runInNewContext(`${predicate}\n${constructor[0]}\nclock`, {
      deterministicReplay, values: { venue: 'bitrue' }, venue, isPrivate: false,
      metadata: { epoch: 1700000000000 }, Date, ReplayClock,
    });
    assert.ok(clock, 'actual runtime clock selection must drive the Bitrue fallback timer');
    const audit = venue.createSocketAudit('ts', { isPrivate: false, books: ['E-BTC-USDT'] });
    const sent = [];
    try {
      clock.install();
      clock.start();
      audit.sent(url, subscribe(primary));
      setTimeout(() => { sent.push(fallback); audit.sent(url, subscribe(fallback)); }, 3000);
      await clock.advance(2999);
      assert.deepEqual(sent, []);
      await clock.advance(3000);
      assert.deepEqual(sent, [fallback]);
      await clock.advance(3600);
      audit.received(url, frame);
      assert.deepEqual(audit.errors(), []);
    } finally { clock.restore(); }
    const unsubscribed = venue.createSocketAudit('rust', { isPrivate: false, books: ['E-BTC-USDT'] });
    unsubscribed.sent(url, subscribe(primary));
    unsubscribed.received(url, frame);
    assert(unsubscribed.errors().includes('data arrived without subscription: ' + fallback));
  }
});
