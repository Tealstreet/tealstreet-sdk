import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { captureCheckpoint, captureTsFill, comparesField, COMPARISON_EXCLUSIONS, duplicateSymbolErrors, normalizeAccount, projectFill } from './comparison-policy.mjs';
import { checkpointOrdinals, expectedCheckpointPlan, Tape } from './tape.mjs';

const venue = { rust: 'example' };
const account = () => ({
  hedged: false, balance: { total: 100, free: 80, used: 20, upnl: 0 },
  orders: [{ id: 'order', symbol: 'BTCUSDT', status: 'open', type: 'conditional', side: 'buy',
    price: 90, amount: 1, filled: 0, remaining: 1, clientOrderId: 'caller', parentId: 'parent',
    takeProfit: 110, stopLoss: 85, extra: { conditionalOrderPrice: 91 } }],
  positions: [{ symbol: 'BTCUSDT', side: 'long', contracts: 1, entryPrice: 90, notional: 90,
    leverage: 5, unrealizedPnl: 10, realizedPnl: 3, marginSize: 18 }],
  settings: [], fills: [{ id: 'order', executionId: 'execution', symbol: 'BTCUSDT', side: 'buy',
    amount: 1, price: 90, fee: 0.01, timestamp: 1700000000 }],
});
const normalize = (value) => normalizeAccount(value, venue, 'ts');
const json = (value) => JSON.stringify(value);

test('fill prices and client IDs default on with no silent legacy opt-in', () => {
  assert.deepEqual(COMPARISON_EXCLUSIONS, {});
  assert.equal(comparesField('unlisted-venue', 'fillPrice'), true);
  assert.equal(comparesField('unlisted-venue', 'clientOrderId'), true);
  const result = normalize(account());
  assert.equal(result.orders[0].clientOrderId, 'caller');
  assert.equal(result.fills[0].price, 90);
});

test('every newly compared financial and identity field survives normalization', () => {
  const original = account();
  const mutations = [
    (a) => { a.orders[0].takeProfit = 111; },
    (a) => { a.orders[0].stopLoss = 84; },
    (a) => { a.orders[0].extra.conditionalOrderPrice = 92; },
    (a) => { a.orders[0].parentId = 'wrong-parent'; },
    (a) => { a.orders[0].clientOrderId = 'wrong-caller'; },
    (a) => { a.positions[0].realizedPnl = 4; },
    (a) => { a.fills[0].price = 91; },
    (a) => { a.fills[0].fee = 0.02; },
    (a) => { a.fills[0].timestamp += 1; },
    (a) => { a.fills[0].executionId = 'wrong-execution'; },
  ];
  for (const mutate of mutations) {
    const changed = structuredClone(original);
    mutate(changed);
    assert.notEqual(json(normalize(changed)), json(normalize(original)), String(mutate));
  }
});

test('missing fill metadata stays null and order identity is never an execution identity', () => {
  assert.deepEqual(projectFill({ id: 'native-order', price: 1, amount: 2, side: 'buy', symbol: 'BTCUSDT' }, 'example'), {
    id: 'native-order', price: 1, amount: 2, side: 'buy', symbol: 'BTCUSDT',
    executionId: null, fee: null, timestamp: null,
  });
  const zero = projectFill({ fee: 0, timestamp: 0, price: 0 }, 'example');
  assert.equal(zero.fee, 0);
  assert.equal(zero.timestamp, 0);
  assert.equal(zero.price, 0);
  const strings = projectFill({ price: '100.25', fee: '-0.01', timestamp: '123' }, 'example');
  assert.equal(strings.price, 100.25);
  assert.equal(strings.fee, -0.01);
  assert.equal(strings.timestamp, 123);
  for (const value of ['', ' ', null, undefined, 'NaN', 'Infinity'])
    assert.equal(projectFill({ price: value }, 'example').price, null);
  const missing = account();
  delete missing.positions[0].realizedPnl;
  delete missing.orders[0].takeProfit;
  delete missing.orders[0].stopLoss;
  delete missing.orders[0].extra;
  delete missing.orders[0].parentId;
  const result = normalize(missing);
  for (const key of ['takeProfit', 'stopLoss', 'conditionalOrderPrice', 'parentId']) assert.equal(result.orders[0][key], null);
  assert.equal(result.positions[0].realizedPnl, null);
});

test('native TS execution identities survive capture and normalization without promoting Rust order IDs', () => {
  const event = { id: 'native-trade', orderId: 'native-order', symbol: 'BTCUSDT', side: 'buy', amount: 1, price: '100.25', fee: '-0.01', timestamp: '123' };
  const captured = captureTsFill(event);
  assert.equal(captured.id, 'native-trade');
  assert.equal(captured.orderId, 'native-order');
  for (const name of ['hyperliquid', 'hyperliquidspot', 'grvt', 'woo', 'weex', 'mexc']) {
    const value = account();
    value.fills = [captured];
    const actual = normalizeAccount(value, { rust: name }, 'ts');
    assert.equal(actual.fills[0].id, 'native-order', name);
    assert.equal(actual.fills[0].executionId, 'native-trade', name);
    assert.equal(actual.fills[0].fee, -0.01, name);
    assert.equal(actual.fills[0].timestamp, 123, name);
    const checkpoint = captureCheckpoint({ ordinal: 1 }, () => actual);
    const mutated = structuredClone(value);
    mutated.fills[0].id = 'wrong-trade';
    assert.notDeepEqual(checkpoint.fills, normalizeAccount(mutated, { rust: name }, 'ts').fills, name);
    assert.equal(projectFill(captured, name, 'rust').executionId, null, name);
    assert.equal(projectFill({ id: 'native-order' }, name, 'rust').executionId, null, name);
    assert.equal(projectFill({ ...captured, executionId: 'explicit-trade' }, name, 'rust').executionId, 'explicit-trade', name);
  }
  // GRVT emits a trade id even when no client order id is available.
  assert.equal(projectFill({ id: 'grvt-trade' }, 'grvt', 'ts').executionId, 'grvt-trade');
  for (const name of ['hyperliquid', 'hyperliquidspot', 'grvt'])
    for (const id of [undefined, null, '', ' ']) assert.equal(projectFill({ id }, name, 'ts').executionId, null);
  for (const name of ['woo', 'weex', 'mexc']) {
    for (const fill of [{ id: 'order', orderId: 'order' }, { id: 'notification' }, { id: '', orderId: 'order' }, { orderId: 'order' }])
      assert.equal(projectFill(fill, name, 'ts').executionId, null, `${name}: ${JSON.stringify(fill)}`);
  }
  for (const name of ['blofin', 'asterdex', 'backpack', 'unknown'])
    assert.equal(projectFill(captured, name, 'ts').executionId, null, name);
});

test('real Hyperliquid perp and spot fill events retain transaction IDs through the replay capture path', () => {
  const script = `
    import assert from 'node:assert/strict';
    import { HyperliquidPrivateWebSocket } from './packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquid.ws-private.ts';
    import { HyperliquidSpotPrivateWebSocket } from './packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.ws-private.ts';
    import { captureTsFill, captureCheckpoint, normalizeAccount } from './sdk-rs/harness/comparison-policy.mjs';
    for (const [name, Socket] of [['hyperliquid', HyperliquidPrivateWebSocket], ['hyperliquidspot', HyperliquidSpotPrivateWebSocket]]) {
      const executions = [];
      const parent = {
        store: { marketsById: { BTC: { id: 'BTC', symbol: 'BTC', info: {} } } },
        emitter: { emit: (event, fill) => { assert.equal(event, 'fill'); executions.push(fill); } },
        requestSpotBalanceRefresh() {},
      };
      const socket = new Socket(parent);
      // Two production transaction ids on the same order, amount and price.
      socket.handleFills({ fills: [9912350101, 9912350102].map(tid => ({
        tid, oid: 9001, coin: 'BTC', side: 'B', px: '100', sz: '1',
      })), isSnapshot: false, user: 'test' });
      assert.equal(executions.length, 2);
      const account = { hedged: false, balance: {}, orders: [], positions: [], settings: [], fills: executions.map(captureTsFill) };
      const checkpoint = captureCheckpoint({ ordinal: 1 }, () => normalizeAccount(account, { rust: name }, 'ts'));
      assert.deepEqual(checkpoint.fills.map(fill => fill.id), ['9001', '9001']);
      assert.deepEqual(checkpoint.fills.map(fill => fill.executionId), ['9912350101', '9912350102']);
      const candidate = { ...account, fills: account.fills.map(fill => ({ ...fill, id: '9001', orderId: undefined })) };
      const projectedCandidate = normalizeAccount(candidate, { rust: name }, 'rust');
      assert.deepEqual(projectedCandidate.fills.map(fill => fill.executionId), [null, null]);
      assert.notDeepEqual(checkpoint.fills, projectedCandidate.fills);
      account.fills[1].id = account.fills[0].id;
      assert.notDeepEqual(checkpoint.fills, normalizeAccount(account, { rust: name }, 'ts').fills);
    }
    process.exit(0);
  `;
  const result = spawnSync(process.execPath, ['--import', 'tsx', '--input-type=module'], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)),
    input: script, encoding: 'utf8', timeout: 30_000,
  });
  assert.equal(result.status, 0, result.error?.stack ?? result.stderr ?? result.stdout);
});

test('duplicate public symbols fail even for otherwise equal rows before Map collapse', () => {
  const snapshot = { markets: [{ symbol: 'BTCUSDT' }, { symbol: 'BTCUSDT' }], tickers: [{ symbol: 'BTCUSDT', last: 90 }, { symbol: 'BTCUSDT', last: 90 }] };
  assert.equal(duplicateSymbolErrors(snapshot, 'rust').length, 2);
  assert.match(duplicateSymbolErrors(snapshot, 'rust')[0], /rust: duplicate markets symbol.*rows 0 and 1/);
  assert.equal(duplicateSymbolErrors({ markets: [{ symbol: 'BTCUSDT' }, { symbol: 'ETHUSDT' }] }).length, 0);
});

test('fixed checkpoint ordinals include startup, quarters, terminal and every command', () => {
  const frames = Array.from({ length: 8 }, (_, index) => ({ type: index === 2 ? 'command' : 'wait' }));
  assert.deepEqual([...checkpointOrdinals(frames)].sort((a, b) => a - b), [0, 2, 3, 4, 6, 8]);
  assert.deepEqual([...checkpointOrdinals([])], [0]);
});

test('Tape emits deterministic detached checkpoints after awaited commands and event-loop delivery', async () => {
  const records = [{ type: 'meta' }, { type: 'wait', t: 10 }, { type: 'command', t: 20, op: 'place' }, { type: 'wait', t: 30 }, { type: 'wait', t: 40 }];
  const tape = new Tape(records);
  const live = { orders: [], prices: new Float64Array([90]) };
  const checkpoints = [];
  const delivered = await tape.play(undefined, undefined, async () => {
    await new Promise((resolve) => setImmediate(resolve));
    live.orders.push({ id: 'accepted', filled: 0 });
    setImmediate(() => { live.orders[0].filled = 0.5; });
    return ['accepted'];
  }, undefined, undefined, (point) => checkpoints.push(captureCheckpoint(point, () => live)));
  assert.equal(delivered, 4);
  assert.deepEqual(checkpoints.map(({ ordinal, recordIndex, t }) => ({ ordinal, recordIndex, t })), [
    { ordinal: 0, recordIndex: -1, t: 0 }, { ordinal: 1, recordIndex: 1, t: 10 },
    { ordinal: 2, recordIndex: 2, t: 20 }, { ordinal: 3, recordIndex: 3, t: 30 }, { ordinal: 4, recordIndex: 4, t: 40 },
  ]);
  assert.deepEqual(checkpoints[0].orders, []);
  assert.deepEqual(checkpoints[2].orders, [{ id: 'accepted', filled: 0.5 }]);
  live.orders[0].filled = 1;
  live.prices[0] = 1;
  assert.equal(checkpoints[2].orders[0].filled, 0.5);
  assert.equal(checkpoints[2].prices[0], 90);
});

test('empty tapes still capture a ready checkpoint; callbacks are optional for performance runs', async () => {
  const tape = new Tape([]);
  const seen = [];
  assert.equal(await tape.play(undefined, undefined, undefined, undefined, undefined, (point) => seen.push(point)), 0);
  assert.deepEqual(seen, [{ ordinal: 0, recordIndex: -1, t: 0 }]);
  assert.equal(await new Tape([]).play(), 0);
});

test('intermediate wrong state remains visible even after identical terminal convergence', () => {
  const left = account();
  const right = structuredClone(left);
  right.orders[0].stopLoss = 20;
  const transient = captureCheckpoint({ ordinal: 1, recordIndex: 1, t: 1 }, () => normalize(right));
  right.orders[0].stopLoss = left.orders[0].stopLoss;
  assert.equal(json(normalize(left)), json(normalize(right)));
  assert.notEqual(json(transient.orders), json(normalize(right).orders));
});

test('implementation-tagged response filtering does not renumber shared tape checkpoint identities', async () => {
  const records = [
    { type: 'meta' },
    { type: 'http', impl: 'ts', method: 'GET', url: 'https://example.test/seed', status: 200, body: '{}' },
    { type: 'wait', t: 10 },
  ];
  const identities = [];
  for (const impl of ['ts', 'rust']) {
    const seen = [];
    await new Tape(records, {}, false, 30000, impl).play(undefined, undefined, undefined, undefined, undefined, (point) => seen.push(point));
    identities.push(seen);
  }
  assert.deepEqual(identities[0], identities[1]);
  assert.equal(identities[0][1].recordIndex, 2);
});

test('checkpoint capture failure rejects replay instead of silently losing audit coverage', async () => {
  await assert.rejects(new Tape([]).play(undefined, undefined, undefined, undefined, undefined, () => {
    throw new Error('checkpoint capture failed');
  }), /checkpoint capture failed/);
});


test('expected checkpoint plans use original input ordinals across aliases, implementation plans and bootstrap filters', () => {
  const records = [
    { type: 'meta', liveSmoke: true },
    { type: 'http', impl: 'ts', t: 1, method: 'GET', url: 'https://example.test/seed', requestBody: 'original', tsRequestBody: 'skip', body: '{}', status: 200 },
    { type: 'http', impl: 'rust', t: 2, method: 'GET', url: 'https://example.test/seed', body: '{}', status: 200 },
    { type: 'ws', t: 3, bootstrap: true, url: 'wss://example.test/stream', text: 'bootstrap' },
    { type: 'wait', t: 4, skip: true },
    { type: 'wait', t: 5 },
    { type: 'command', t: 6, op: 'example' },
    { type: 'wait', t: 7 },
  ];
  const venue = { polling: true, skipFrame: (record) => record.skip || record.requestBody === 'skip' };
  assert.deepEqual(expectedCheckpointPlan(records, venue, 'ts'), [
    { ordinal: 0, recordIndex: -1, t: 0 }, { ordinal: 1, recordIndex: 5, t: 5 },
    { ordinal: 2, recordIndex: 6, t: 6 }, { ordinal: 3, recordIndex: 7, t: 7 },
  ]);
  assert.deepEqual(expectedCheckpointPlan(records, venue, 'rust'), [
    { ordinal: 0, recordIndex: -1, t: 0 }, { ordinal: 1, recordIndex: 2, t: 2 },
    { ordinal: 2, recordIndex: 5, t: 5 }, { ordinal: 3, recordIndex: 6, t: 6 }, { ordinal: 4, recordIndex: 7, t: 7 },
  ]);
  assert.deepEqual(expectedCheckpointPlan([], {}, 'rust'), [{ ordinal: 0, recordIndex: -1, t: 0 }]);
  assert.throws(() => expectedCheckpointPlan(records, venue), /Tagged tape records require/);
  assert.throws(() => expectedCheckpointPlan(records, venue, 'unknown'), /Unknown replay implementation/);
});

test('real Tape captures exactly the independently derived complete schedule including a nonquarter command', async () => {
  const records = [{ type: 'meta' }, ...Array.from({ length: 8 }, (_, index) => ({ type: index === 2 ? 'command' : 'wait', t: index * 10, ...(index === 2 ? { op: 'example' } : {}) }))];
  const expected = expectedCheckpointPlan(records, {}, 'rust');
  assert.deepEqual(expected.map(({ ordinal }) => ordinal), [0, 2, 3, 4, 6, 8]);
  const actual = [];
  await new Tape(records, {}, false, 30000, 'rust').play(undefined, undefined, async () => [], undefined, undefined, (point) => actual.push(point));
  assert.deepEqual(actual, expected);
});
