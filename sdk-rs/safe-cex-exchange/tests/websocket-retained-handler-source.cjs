'use strict';
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const { DefaultStore } = require(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const load = (venue, name) => require(path.join(root, `packages/safe-cex/src/exchanges/${venue}/${venue}.exchange.ts`))[name];
const profiles = [
  ['asterdex', 'AsterDEXExchange', 'privateWebsocket', 'retainedProbe', { e: 'retainedProbe' }],
  ['asterdex', 'AsterDEXExchange', 'publicWebsocket', 'retainedProbe', { e: 'retainedProbe' }],
  ['binance', 'BinanceExchange', 'publicWebsocket', 'retainedProbe', { e: 'retainedProbe' }],
  ['bybit', 'BybitExchange', 'publicWebsocket', 'retainedProbe', { topic: 'retainedProbe.BTC' }],
  ['mexc', 'MexcExchange', 'publicWebsocket', 'trade.BTCUSDT', { channel: 'push.deal', symbol: 'BTC_USDT' }],
  ['okx', 'OKXExchange', 'publicWebsocket', 'retainedProbe', { arg: { channel: 'retainedProbe' } }],
];
const units = value => Array.from({ length: value.length }, (_, index) => value.charCodeAt(index));
const cases = [];
for (const [venue, name, slot, key, envelope] of profiles) {
  const Exchange = load(venue, name);
  const exchange = new Exchange(venue, { publicDataOnly: true, tuning: { consoleLogging: false } }, new DefaultStore());
  try {
    const socket = exchange[slot];
    let current, calls = 0, expected;
    socket.messageHandlers[key] = [value => {
      const row = venue === 'binance' ? value[0] : value;
      assert.equal(row.overflow, Infinity);
      assert(Object.is(row.zero, -0));
      assert.deepEqual(units(Object.keys(row.unused)[0]), expected);
      assert.deepEqual(units(Object.values(row.unused)[0][0]), expected);
      current = value;
      row.mutated = { retained: true };
      row.presentUndefined = undefined;
      calls++;
    }, value => {
      assert.strictEqual(value, current);
      const row = venue === 'binance' ? value[0] : value;
      assert.deepEqual(row.mutated, { retained: true });
      assert(Object.hasOwn(row, 'presentUndefined'));
      assert.equal(row.presentUndefined, undefined);
      calls++;
    }];
    for (const marker of ['\ud800', '\udfff', '\ud83e\udd80', 'neighbor']) {
      expected = units(marker);
      const frame = JSON.stringify({ ...envelope, unused: { [marker]: [marker] }, overflow: '__OVERFLOW__', zero: '__ZERO__' })
        .replace('"__OVERFLOW__"', '1e400').replace('"__ZERO__"', '-0');
      const before = calls;
      socket.onMessage({ data: frame });
      assert.equal(calls - before, 2, `${venue} both callbacks receive one native graph`);
      cases.push({ venue, slot, frame, units: expected, callbacks: 2, sameReference: true, earlierMutationVisible: true, ownUndefined: true, overflow: 'Infinity', zeroBits: '8000000000000000' });
    }
    assert.equal(calls, 8);
  } finally { exchange.dispose(); }
}
assert.equal(cases.length, 24);
process.stdout.write(JSON.stringify({ cases }), () => process.exit(0));
