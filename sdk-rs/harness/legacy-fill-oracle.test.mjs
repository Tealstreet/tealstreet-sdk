import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { applyLegacyFillOracle, matchLegacyFillRows, cloneOracleData } from './legacy-fill-oracle.mjs';
import { createTsFixes } from './ts-oracle.mjs';
import { auditFixerChanges } from './fixer-contracts.mjs';
import { fixerLint } from './fixer-lint.mjs';
import { runAuditedFixer } from './oracle-audit.mjs';
import { ORDERLY_BROKERS } from './venues.mjs';

const fill = (extra = {}) => ({ id: 'order-1', symbol: 'BTCUSD', side: 'buy', amount: 1, price: 100, executionId: null, fee: null, timestamp: null, ...extra });
const output = (fills = []) => ({
  markets: [], tickers: [], books: {}, requests: [], subscriptions: [], commands: [],
  account: { fills, orders: [], positions: [], settings: [], balance: { used: 0 } },
});
const rebuild = (rows) => (shadow) => { shadow.account.fills = structuredClone(rows); };
const oldShape = (row) => Object.fromEntries(Object.entries(row).filter(([key]) => !['executionId', 'fee', 'timestamp'].includes(key)));

test('adapter and production wrapper are linted without candidate access', () => {
  for (const file of ['legacy-fill-oracle.mjs', 'ts-oracle.mjs'])
    assert.deepEqual(fixerLint(readFileSync(new URL(file, import.meta.url), 'utf8'), file, true), []);
  assert.ok(fixerLint('const shadow = rust.account;', 'invalid.mjs', true).length);
});

test('legacy assertions see their old shape while observed metadata remains intact', () => {
  const actual = fill({ fee: -0.25, timestamp: 12345, executionId: 'execution-1' });
  const reference = output([actual]);
  const before = structuredClone(reference);
  const result = applyLegacyFillOracle(reference, (shadow) => {
    assert.deepEqual(shadow.account.fills, [oldShape(actual)]);
    shadow.account.balance.used = 10;
    shadow.account.fills = [oldShape(actual)];
  });
  assert.deepEqual(reference.account.fills, [actual]);
  assert.equal(reference.account.balance.used, 10);
  assert.deepEqual(before.account.balance, { used: 0 });
  assert.equal(result.preservedRawFills, false);
});

test('a sparse price omission preserves the actual TS execution price and metadata', () => {
  const actual = fill({ fee: 0.1, timestamp: 1, executionId: 'x' });
  const reference = output([actual]);
  const sparse = oldShape(actual); delete sparse.price;
  applyLegacyFillOracle(reference, rebuild([sparse]));
  assert.deepEqual(reference.account.fills, [actual]);
});

test('an explicitly tape-derived execution price is retained when TS lacks it', () => {
  const reference = output([fill({ price: null })]);
  applyLegacyFillOracle(reference, rebuild([oldShape(fill({ price: 101 }))]));
  assert.deepEqual(reference.account.fills, [fill({ price: 101 })]);
});

test('new expected executions initialize the full shape with null metadata only', () => {
  const reference = output([]), before = structuredClone(reference);
  applyLegacyFillOracle(reference, rebuild([oldShape(fill())]));
  assert.deepEqual(reference.account.fills, [fill()]);
  assert.deepEqual(auditFixerChanges('phemex', before, reference).errors, []);
});

test('uniquely matched rows retain every TS field and reorder metadata with their execution', () => {
  const a = fill({ fee: 0.1, timestamp: 1, executionId: 'a', extension: 'still visible' });
  const b = fill({ id: 'order-2', amount: 2, fee: -0.2, timestamp: 2, executionId: 'b' });
  const reference = output([a, b]), before = structuredClone(reference);
  const first = oldShape(b), second = oldShape(a); delete second.extension;
  applyLegacyFillOracle(reference, rebuild([first, second]));
  assert.deepEqual(reference.account.fills, [b, a]);
  // Existing unknown fields cannot be erased by sparse rebuilding. The field
  // auditor still refuses moving an undeclared extension, which is intentional.
  assert.ok(auditFixerChanges('phemex', before, reference).errors.every((error) => error.includes('extension')));
});

test('insertions preserve existing metadata without granting metadata writes', () => {
  const existing = fill({ fee: 0.1, timestamp: 1, executionId: 'original' });
  const added = fill({ id: 'new-order', amount: 2 });
  const reference = output([existing]), before = structuredClone(reference);
  applyLegacyFillOracle(reference, rebuild([oldShape(added), oldShape(existing)]));
  assert.deepEqual(reference.account.fills, [added, existing]);
  assert.deepEqual(auditFixerChanges('phemex', before, reference).errors, []);
  const erased = structuredClone(reference); erased.account.fills[1].fee = null;
  assert.ok(auditFixerChanges('phemex', reference, erased).errors.some((error) => error.includes('.fee')));
});

test('ambiguous equal executions keep all original evidence rather than selecting a row', () => {
  const a = fill({ fee: 0.1, timestamp: 1, executionId: 'a' });
  const b = fill({ fee: 0.2, timestamp: 2, executionId: 'b' });
  const reference = output([a, b]), before = structuredClone(reference);
  const result = applyLegacyFillOracle(reference, (shadow) => {
    shadow.account.fills = [oldShape(a)];
    shadow.account.balance.used = 1;
  });
  assert.equal(result.preservedRawFills, true);
  assert.deepEqual(reference.account.fills, [a, b]);
  assert.equal(reference.account.balance.used, 1);
  const audit = auditFixerChanges('phemex', before, { ...reference, account: { ...reference.account, balance: before.account.balance } });
  assert.deepEqual(audit.errors, []);
});

test('ambiguous fan-out cannot duplicate one observed execution fee', () => {
  const actual = fill({ fee: 0.1, executionId: 'a' });
  const reference = output([actual]);
  assert.equal(applyLegacyFillOracle(reference, rebuild([oldShape(actual), oldShape(actual)])).preservedRawFills, true);
  assert.deepEqual(reference.account.fills, [actual]);
});

test('cumulative quantity cannot inherit metadata for smaller reconstructed executions', () => {
  const actual = fill({ amount: 2, fee: 0.1, timestamp: 5, executionId: 'cumulative' });
  const reference = output([actual]);
  assert.equal(applyLegacyFillOracle(reference, rebuild([oldShape(fill()), oldShape(fill())])).preservedRawFills, true);
  assert.deepEqual(reference.account.fills, [actual]);
});

test('observed evidence cannot disappear with unmatched rows or changed prices', () => {
  for (const rows of [[], [oldShape(fill({ price: 101 }))], [oldShape(fill({ id: 'other' }))]]) {
    const actual = fill({ fee: 0, timestamp: 0, executionId: '' });
    const reference = output([actual]);
    assert.equal(applyLegacyFillOracle(reference, rebuild(rows)).preservedRawFills, true);
    assert.deepEqual(reference.account.fills, [actual]);
  }
});

test('unobserved rows can be removed without deleting fields from retained executions', () => {
  const reference = output([fill()]), before = structuredClone(reference);
  applyLegacyFillOracle(reference, rebuild([]));
  assert.deepEqual(reference.account.fills, []);
  assert.deepEqual(auditFixerChanges('phemex', before, reference).errors, []);
  const malformed = structuredClone(before); delete malformed.account.fills[0].timestamp;
  assert.ok(auditFixerChanges('phemex', before, malformed).errors.some((error) => error.includes('.timestamp')));
});

test('exceptions and legacy attempts to write metadata publish nothing', () => {
  for (const mutate of [
    (shadow) => { shadow.account.balance.used = 1; throw new Error('legacy assertion'); },
    (shadow) => { shadow.account.balance.used = 1; shadow.account.fills[0].fee = null; },
    (shadow) => { delete shadow.account; },
  ]) {
    const reference = output([fill({ fee: 0.1 })]), before = structuredClone(reference);
    assert.throws(() => applyLegacyFillOracle(reference, mutate));
    assert.deepEqual(reference, before);
  }
});

test('matching requires the whole execution and preserves amount/price ambiguity', () => {
  assert.deepEqual(matchLegacyFillRows([fill()], [fill()]).sourceIndices, [0]);
  for (const patch of [{ amount: 2 }, { side: 'sell' }, { symbol: 'ETHUSD' }, { id: 'other' }, { price: 101 }])
    assert.deepEqual(matchLegacyFillRows([fill()], [fill(patch)]).sourceIndices, [undefined]);
  assert.equal(matchLegacyFillRows([fill(), fill()], [fill()]).ambiguous, true);
});

test('production Phemex wrapper restores non-null actual TS metadata after rebuilding', () => {
  const reference = output([fill({ fee: 0.2, timestamp: 12, executionId: 'observed' })]);
  const before = structuredClone(reference);
  const records = [{ type: 'ws', text: JSON.stringify({ orders_p: [{ orderID: 'order-1', symbol: 'BTCUSD', side: 'Buy', execStatus: 'MakerFill', execID: 'venue-execution', execQty: '1', execPriceRp: '100' }] }) }];
  createTsFixes({ venue: 'phemex' }, records).applyTsCorrections(reference);
  assert.deepEqual(reference.account.fills, before.account.fills);
  assert.deepEqual(auditFixerChanges('phemex', before, reference).errors, []);
});

test('production Coinbase wrapper accepts widened numeric-string price shape and emits complete rows', () => {
  const native = (quantity, price, status) => ({ order_id: 'order-1', product_id: 'BTC-USD', order_side: 'BUY', cumulative_quantity: quantity, avg_price: price, status });
  const frame = (type, orders) => ({ type: 'ws', url: 'wss://advanced-trade-ws-user.coinbase.com', text: JSON.stringify({ channel: 'user', events: [{ type, orders }] }) });
  const records = [
    { type: 'meta', private: true, profile: 'spot' },
    { type: 'http', method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/products', status: 200, body: JSON.stringify({ products: [{ product_id: 'BTC-USD', product_type: 'SPOT' }] }) },
    frame('snapshot', []), frame('update', [native('0.4', '70', 'OPEN')]), frame('update', [native('1', '80', 'FILLED')]),
  ];
  const reference = output([fill({ symbol: 'BTC-USD', price: 80 })]), before = structuredClone(reference);
  createTsFixes({ venue: 'coinbase' }, records).applyTsCorrections(reference);
  assert.equal(reference.account.fills.length, 2);
  assert.deepEqual(reference.account.fills.map((row) => [row.amount, row.price]), [[0.4, 70], [0.6, (80 - 28) / 0.6]]);
  for (const row of reference.account.fills)
    for (const field of ['executionId', 'fee', 'timestamp']) assert.equal(row[field], null);
  assert.deepEqual(auditFixerChanges('coinbase', before, reference).errors, []);
});

test('auditor refuses reassigning unchanged metadata to another execution identity', () => {
  const before = output([fill({ fee: 0.1, executionId: 'a', timestamp: 1 })]);
  const after = structuredClone(before); after.account.fills[0].id = 'some-other-order';
  assert.ok(auditFixerChanges('phemex', before, after).errors.some((error) => error.includes('.fee')));
});

test('proxy-safe data clone preserves scalar truth, undefined properties and array shape', () => {
  const source = { value: undefined, nan: NaN, negativeZero: -0, infinity: Infinity, rows: new Array(2) };
  source.rows[1] = { child: undefined };
  Object.defineProperty(source, 'hidden', { value: { actual: 1 }, enumerable: false });
  const clone = cloneOracleData(new Proxy(source, {}));
  assert.ok(Object.hasOwn(clone, 'value'));
  assert.equal(clone.value, undefined);
  assert.ok(Number.isNaN(clone.nan));
  assert.ok(Object.is(clone.negativeZero, -0));
  assert.equal(clone.infinity, Infinity);
  assert.equal(clone.rows.length, 2);
  assert.equal(Object.hasOwn(clone.rows, 0), false);
  assert.ok(Object.hasOwn(clone.rows[1], 'child'));
  assert.notEqual(clone.rows, source.rows);
  assert.notEqual(clone.hidden, source.hidden);
  clone.hidden.actual = 2;
  assert.equal(source.hidden.actual, 1);
});

test('data clone rejects getters and cyclic/non-data containers without invoking them', () => {
  let calls = 0;
  const accessor = { get value() { calls++; return 1; } };
  const cycle = {}; cycle.self = cycle;
  for (const value of [accessor, cycle, new Date(), new Map(), new Set(), { callback() {} }])
    assert.throws(() => cloneOracleData(value), /replay data|data properties/);
  assert.equal(calls, 0);
});

const legacyVenues = [
  'coinbase', 'coinbasespot', 'orderly', ...Object.keys(ORDERLY_BROKERS),
  'phemex', 'kraken', 'bingx', 'backpack', 'cryptocom', 'lighter', 'lighter-robinhood',
];
const integrationInput = (venue, populated) => {
  const row = fill({ executionId: 'observed-execution', fee: -0.25, timestamp: 12345 });
  const records = [{ type: 'meta', private: true }];
  if (venue === 'coinbase' || venue === 'coinbasespot') {
    row.symbol = 'BTC-USD';
    records.push({ type: 'http', method: 'GET', url: 'https://api.coinbase.com/api/v3/brokerage/products', status: 200,
      body: JSON.stringify({ products: [{ product_id: 'BTC-USD', product_type: 'SPOT' }] }) });
    if (populated) {
      const frame = (type, orders) => ({ type: 'ws', url: 'wss://advanced-trade-ws-user.coinbase.com',
        text: JSON.stringify({ channel: 'user', events: [{ type, orders }] }) });
      records.push(frame('snapshot', []), frame('update', [{ order_id: row.id, product_id: row.symbol,
        order_side: 'BUY', cumulative_quantity: '1', avg_price: '100', status: 'FILLED' }]));
    }
  }
  const reference = output(populated ? [row] : []);
  reference.scalarTruth = { nan: NaN, negativeZero: -0, absent: undefined };
  return { reference, records };
};

for (const venue of legacyVenues)
  for (const populated of [false, true])
    test(`production audited ${venue} legacy adapter supports ${populated ? 'populated' : 'empty'} private fills`, () => {
      const { reference, records } = integrationInput(venue, populated);
      const original = structuredClone(reference.account.fills);
      const { applyTsCorrections } = createTsFixes({ venue }, records);
      const audit = runAuditedFixer(venue, reference, applyTsCorrections);
      assert.deepEqual(audit.errors, []);
      assert.deepEqual(reference.account.fills, original);
      assert.ok(Number.isNaN(reference.scalarTruth.nan));
      assert.ok(Object.is(reference.scalarTruth.negativeZero, -0));
      assert.ok(Object.hasOwn(reference.scalarTruth, 'absent'));
      assert.equal(reference.scalarTruth.absent, undefined);
      assert.doesNotThrow(() => structuredClone(reference), 'published data must contain no observer Proxy');
    });

test('a failing legacy callback cannot mutate the observed reference through shadow aliases', () => {
  const reference = output([fill({ fee: 1 })]), original = structuredClone(reference);
  assert.throws(() => runAuditedFixer('phemex', reference, (observed) => applyLegacyFillOracle(observed, (shadow) => {
    shadow.account.balance.used = 2;
    shadow.account.fills[0].price = 200;
    throw new Error('stop before publication');
  })), /stop before publication/);
  assert.deepEqual(reference, original);
});
