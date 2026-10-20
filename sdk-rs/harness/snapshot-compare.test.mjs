import assert from 'node:assert/strict';
import { test } from 'node:test';
import { compareCheckpoints, compareSnapshots, duplicateSymbols, checkpointCoverageErrors, snapshotCoverageErrors } from './snapshot-compare.mjs';

const snapshot = () => ({
  markets: [{ symbol: 'BTC' }], tickers: [{ symbol: 'BTC', last: 100 }], books: {},
  account: {
    orders: [{ id: '1', symbol: 'BTC', clientOrderId: 'caller', takeProfit: 110, stopLoss: 90, conditionalOrderPrice: 99, parentId: 'parent' }],
    positions: [{ symbol: 'BTC', realizedPnl: 7 }],
    fills: [{ id: '1', price: 100, fee: 0.1, timestamp: 123, executionId: 'exec' }],
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
