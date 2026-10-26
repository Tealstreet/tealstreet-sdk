import assert from 'node:assert/strict';
import { test } from 'node:test';
import { compareCheckpoints, compareSnapshots, duplicateSymbols, checkpointCoverageErrors, snapshotCoverageErrors } from './snapshot-compare.mjs';

const snapshot = () => ({
  markets: [{ symbol: 'BTC' }], tickers: [{ symbol: 'BTC', last: 100 }], books: {},
  account: {
    hedged: false, balance: { used: 20, free: 80, total: 100, upnl: 0 }, settings: [{ symbol: 'BTC', marginType: 'cross', leverage: 10 }], flatPositionPnl: [],
    orders: [{ id: '1', symbol: 'BTC', clientOrderId: 'caller', takeProfit: 110, stopLoss: 90, conditionalOrderPrice: 99, parentId: 'parent',
      takeProfitTrigger: 'mark_price', stopLossTrigger: 'last_price', status: 'open', type: 'limit', side: 'buy', price: 100,
      amount: 2, filled: 1, remaining: 1, reduceOnly: false, close: false, trigger: null, rejectReason: null, timeInForce: 'GTC' }],
    positions: [{ symbol: 'BTC', realizedPnl: 7, side: 'long', entryPrice: 100, contracts: 1, notional: 100, leverage: 10,
      unrealizedPnl: 0, liquidationPrice: null, marginType: 'cross', positionMode: null, marginSize: 10 }],
    fills: [{ id: '1', symbol: 'BTC', side: 'buy', amount: 1, price: 100, fee: 0.1, timestamp: 123, executionId: 'exec' }],
  },
});

test('snapshot comparison detects each widened economic and identity field', () => {
  for (const [section, fields] of [
    ['orders', ['clientOrderId', 'takeProfit', 'stopLoss', 'conditionalOrderPrice', 'parentId']],
    ['positions', ['realizedPnl']],
    ['fills', ['price', 'fee', 'timestamp', 'executionId']],
  ]) for (const field of fields) {
    const left = snapshot();
    const right = snapshot();
    right.account[section][0][field] = 'incorrect';
    assert.ok(compareSnapshots(left, right).length, `${section}.${field}`);
  }
});

test('duplicate symbols fail even when duplicates and both answers agree', () => {
  for (const field of ['markets', 'tickers']) {
    const value = snapshot();
    value[field].push({ ...value[field][0] });
    assert.match(compareSnapshots(value, structuredClone(value)).join('\n'), /duplicate symbol BTC/);
  }
  assert.deepEqual(duplicateSymbols([{ symbol: 'BTC' }, { symbol: 'ETH' }], 'rows'), []);
  const filtered = { markets: [], tickers: [], books: {}, duplicateSymbols: ['snapshot: duplicate markets symbol "SPOT" at rows 0 and 1'] };
  assert.match(compareSnapshots(filtered, structuredClone(filtered)).join('\n'), /duplicate markets symbol/);
});

test('intermediate snapshots detect transient defects despite final convergence', () => {
  const checkpoints = [0, 1, 2].map((ordinal) => ({ ordinal, recordIndex: ordinal, t: ordinal * 250, ...snapshot() }));
  const candidate = structuredClone(checkpoints);
  candidate[1].tickers[0].last = 1;
  assert.match(compareCheckpoints(checkpoints, candidate).join('\n'), /checkpoint 1 tickers/);
  assert.deepEqual(candidate[2], checkpoints[2]);
  candidate[1] = structuredClone(checkpoints[1]);
  candidate[1].t++;
  assert.match(compareCheckpoints(checkpoints, candidate).join('\n'), /checkpoint 1 t:/);
  assert.deepEqual(compareCheckpoints(checkpoints, structuredClone(checkpoints)), []);
  assert.match(compareCheckpoints([], []).join('\n'), /missing/);
  assert.match(compareCheckpoints([snapshot()], [snapshot()]).join('\n'), /invalid tape boundary/);
});


test('checkpoint coverage rejects identical omissions, duplication, reordering and retiming against tape plan', () => {
  const plan = [
    { ordinal: 0, recordIndex: -1, t: 0 },
    { ordinal: 2, recordIndex: 3, t: 20 },
    { ordinal: 3, recordIndex: 4, t: 30 }, // command boundary outside quarters
    { ordinal: 4, recordIndex: 5, t: 40 },
    { ordinal: 6, recordIndex: 7, t: 60 },
    { ordinal: 8, recordIndex: 9, t: 80 },
  ];
  const complete = plan.map((point) => ({ ...point, ...snapshot() }));
  assert.deepEqual(checkpointCoverageErrors(complete, plan, 'ts'), []);
  const mutations = [
    (rows) => rows.splice(1, 1),
    (rows) => rows.splice(2, 1),
    (rows) => rows.splice(1),
    (rows) => { rows[1] = structuredClone(rows[0]); },
    (rows) => { [rows[1], rows[2]] = [rows[2], rows[1]]; },
    (rows) => { rows[1].t++; },
    (rows) => { rows[1].recordIndex++; },
    (rows) => rows.push(structuredClone(rows[1])),
  ];
  for (const mutate of mutations) {
    const rows = structuredClone(complete);
    mutate(rows);
    // Pairwise equality used to accept each of these broken capture streams.
    assert.deepEqual(compareCheckpoints(rows, structuredClone(rows)), []);
    const errors = checkpointCoverageErrors(rows, plan, 'rust');
    assert(errors.length, String(mutate));
    assert(errors.every((error) => error.startsWith('checkpoint coverage rust:')), String(mutate));
  }
});

test('checkpoint coverage requires startup on empty tapes and rejects missing or malformed boundaries', () => {
  const plan = [{ ordinal: 0, recordIndex: -1, t: 0 }];
  assert.deepEqual(checkpointCoverageErrors(plan.map((point) => ({ ...point, ...snapshot() })), plan), []);
  for (const rows of [undefined, [], [null], [{ ordinal: 0, recordIndex: -1, t: NaN }], [{ ordinal: 0, recordIndex: -1 }]])
    assert(checkpointCoverageErrors(rows, plan).length);
  assert(checkpointCoverageErrors(plan, []).length);
});


test('coverage rejects identical financial-section erasure and public/private scope substitution', () => {
  const plan = [{ ordinal: 0, recordIndex: -1, t: 0 }];
  const point = { ...plan[0], ...snapshot() };
  assert.deepEqual(checkpointCoverageErrors([point], plan, 'ts', { isPrivate: true }), []);
  for (const mutate of [
    (value) => { delete value.account; },
    (value) => { value.account = null; },
    (value) => { value.account = []; },
    (value) => { delete value.books; },
    (value) => { value.books = []; },
  ]) {
    const erased = structuredClone(point);
    mutate(erased);
    const errors = checkpointCoverageErrors([erased], plan, 'rust', { isPrivate: true });
    assert(errors.length, String(mutate));
    assert(errors.every((error) => error.startsWith('checkpoint coverage ')));
  }
  const publicSnapshot = { markets: [], tickers: [], books: {} };
  assert.deepEqual(snapshotCoverageErrors(publicSnapshot, 'ts final', { isPrivate: false }), []);
  assert(snapshotCoverageErrors({ ...publicSnapshot, account: {} }, 'ts final', { isPrivate: false }).length);
  assert(snapshotCoverageErrors(publicSnapshot, 'rust final', { isPrivate: true }).length);
});

test('private coverage rejects shared missing sections and newly compared row fields', () => {
  const original = snapshot();
  assert.deepEqual(snapshotCoverageErrors(original, 'private', { isPrivate: true }), []);
  for (const field of ['orders', 'positions', 'settings', 'fills', 'flatPositionPnl', 'hedged', 'balance']) {
    for (const mutate of [
      (value) => { delete value.account[field]; },
      (value) => { value.account[field] = field === 'hedged' ? null : {}; },
    ]) {
      const missing = structuredClone(original); mutate(missing);
      assert.deepEqual(compareSnapshots(missing, structuredClone(missing)), []);
      const errors = snapshotCoverageErrors(missing, 'private', { isPrivate: true });
      assert.ok(errors.some((error) => error.includes(`account.${field}`)), field);
      assert.ok(errors.every((error) => error.startsWith('checkpoint coverage ')));
    }
  }
  for (const [section, fields] of Object.entries({
    orders: ['clientOrderId', 'takeProfit', 'stopLoss', 'conditionalOrderPrice', 'parentId', 'takeProfitTrigger', 'stopLossTrigger'],
    positions: ['realizedPnl'], fills: ['price', 'fee', 'timestamp', 'executionId'],
  })) for (const field of fields) {
    const missing = structuredClone(original); delete missing.account[section][0][field];
    assert.deepEqual(compareSnapshots(missing, structuredClone(missing)), []);
    assert.ok(snapshotCoverageErrors(missing, 'private', { isPrivate: true })
      .some((error) => error.includes(`account.${section}[0].${field} missing`)), `${section}.${field}`);
  }
  const flat = structuredClone(original);
  flat.account.flatPositionPnl.push({ symbol: 'BTC', side: 'long', positionMode: null });
  assert.ok(snapshotCoverageErrors(flat, 'private', { isPrivate: true }).some((error) => error.includes('flatPositionPnl[0].realizedPnl missing')));
});

test('private balance preserves explicit unavailable evidence while rejecting absent or malformed financial values', () => {
  const original = snapshot();
  original.account.balance = null;
  assert.deepEqual(snapshotCoverageErrors(original, 'startup', { isPrivate: true }), []);
  original.account.balance = { used: null, free: null, total: null, upnl: null };
  assert.deepEqual(snapshotCoverageErrors(original, 'startup', { isPrivate: true }), []);
  for (const field of ['used', 'free', 'total', 'upnl']) {
    const missing = structuredClone(original); delete missing.account.balance[field];
    assert.ok(snapshotCoverageErrors(missing, 'private', { isPrivate: true }).some((error) => error.includes(`balance.${field} missing`)));
    for (const value of [undefined, NaN, Infinity, '100', false, {}]) {
      const malformed = structuredClone(original); malformed.account.balance[field] = value;
      assert.ok(snapshotCoverageErrors(malformed, 'private', { isPrivate: true }).some((error) => error.includes(`balance.${field} finite number or null`)));
    }
  }
});

test('private row coverage rejects null, scalar and sparse rows even when both streams agree', () => {
  for (const section of ['orders', 'positions', 'settings', 'fills', 'flatPositionPnl']) {
    for (const rows of [[null], [0], [[]], new Array(1)]) {
      const malformed = snapshot(); malformed.account[section] = rows;
      assert.ok(snapshotCoverageErrors(malformed, 'private', { isPrivate: true })
        .some((error) => error.includes(`account.${section}[0] row object missing`)));
    }
  }
});

test('base normalized row keys cannot disappear from both financial snapshots', () => {
  const original = snapshot();
  for (const section of ['orders', 'positions', 'settings', 'fills']) {
    for (const field of Object.keys(original.account[section][0])) {
      const missing = structuredClone(original); delete missing.account[section][0][field];
      assert.deepEqual(compareSnapshots(missing, structuredClone(missing)), []);
      assert.ok(snapshotCoverageErrors(missing, 'private', { isPrivate: true }).length, `${section}.${field}`);
    }
  }
  // Explicit unavailable values remain evidence; no zero or venue facts are invented.
  const unavailable = snapshot();
  for (const section of ['orders', 'positions', 'settings', 'fills'])
    for (const field of Object.keys(unavailable.account[section][0])) unavailable.account[section][0][field] = null;
  assert.deepEqual(snapshotCoverageErrors(unavailable, 'private', { isPrivate: true }), []);
});

test('settings require exactly one complete normalized leverage shape', () => {
  for (const leverage of [{ leverage: null }, { buyLeverage: null, sellLeverage: null }, { buyLeverage: '10', sellLeverage: '20' }]) {
    const value = snapshot(); value.account.settings = [{ symbol: 'BTC', marginType: null, ...leverage }];
    assert.deepEqual(snapshotCoverageErrors(value, 'private', { isPrivate: true }), []);
  }
  for (const leverage of [{}, { buyLeverage: 10 }, { sellLeverage: 10 }, { leverage: 10, buyLeverage: 10, sellLeverage: 10 }, { leverage: undefined }]) {
    const value = snapshot(); value.account.settings = [{ symbol: 'BTC', marginType: null, ...leverage }];
    assert.ok(snapshotCoverageErrors(value, 'private', { isPrivate: true }).some((error) => error.includes('leverage or buyLeverage/sellLeverage')));
  }
});
