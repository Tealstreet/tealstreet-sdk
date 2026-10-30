// The LOCAL worker's Rust algos end to end: a real `algos` build of the Binance
// USD-M engine behind EngineHost, over a fake venue on a virtual clock. Build it
// with `bash sdk-rs/harness/build.sh --venue binance`; other builds skip.
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { binanceVenue } from './binance-venue-fixture.mjs';
import { EngineHost } from './host.mjs';

const built = new URL('../pkg/node/safe_cex_wasm.js', import.meta.url);
const engineModule = existsSync(built) ? createRequire(import.meta.url)(built.pathname) : undefined;
const construct = () => new engineModule.WasmEngine('binance', JSON.stringify({ key: 'fixture-key', secret: 'fixture-secret' }));
const hasAlgos = (() => {
  if (typeof engineModule?.WasmEngine?.prototype?.algoStart !== 'function') return false;
  try { construct().free(); return true; } catch { return false; }
})();
const enabled = { skip: !hasAlgos && 'needs an `algos` build containing binance' };

async function startedAccount() {
  const venue = binanceVenue();
  const emits = [];
  const host = new EngineHost(construct(), venue.transport, {
    onAlgoEmit: (op, data) => emits.push({ op, data }),
    onLog: () => {},
  }, { ownsEngine: true });
  host.start();
  await venue.settle();
  host.flush();
  assert.ok(venue.privateSocket(), `user stream connected (unserved: ${[...venue.unknown].join(', ')})`);
  host.algoStart('acct');
  return { venue, host, emits };
}

const lastRows = (emits) => emits.filter((emit) => emit.op === 'store').at(-1)?.data;

test('a local chase places a limit at the bid and completes when the user stream fills it', enabled, async () => {
  const { venue, host, emits } = await startedAccount();
  try {
    host.algoOp('chaser:start', { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10, accountName: 'acct', clientId: 'chase-1' });
    await venue.settle();
    assert.equal(venue.placements.length, 1, 'the Rust chaser placed one order');
    const [order] = venue.placements;
    assert.deepEqual([order.type, order.side, order.price, order.origQty], ['LIMIT', 'BUY', '95999.9', '0.002']);
    assert.equal(lastRows(emits).activeChasers.length, 1);

    venue.fill(order);
    // The host's flush hands the fill to the algos; the timer it arms runs the chase's next check.
    host.flush();
    assert.ok(host.algoTimer !== undefined, 'the fill woke the algos');
    await venue.advance(1_000);
    const complete = emits.find((emit) => emit.op === 'chaser:complete');
    assert.ok(complete, `chaser:complete emitted: ${JSON.stringify(emits.map((emit) => emit.op))}`);
    assert.equal(complete.data.clientId, 'chase-1');
    assert.equal(complete.data.symbol, 'BTCUSDT');
    assert.equal(complete.data.filled, 0.002);
    assert.deepEqual(lastRows(emits).activeChasers, []);
    assert.equal(venue.placements.length, 1, 'a filled chase places nothing more');
    assert.deepEqual(emits.filter((emit) => emit.op === 'error'), []);
  } finally { host.dispose(); }
});

test('a two-slice TWAP places its second slice when the host timer fires, then completes', enabled, async () => {
  const { venue, host, emits } = await startedAccount();
  try {
    host.algoOp('twap:start', { accountName: 'acct', activeMarket: { symbol: 'BTCUSDT' }, size: 0.004, duration: 1, ordersCount: 2,
      side: 'buy', chaserDelay: 1000, reduceOnly: false, randomnessPercentage: 0, useLimitOrders: false, useChaserOrders: false });
    await venue.settle();
    assert.equal(venue.placements.length, 1, 'the first slice at once');
    assert.equal(venue.placements[0].type, 'MARKET');
    assert.equal(lastRows(emits).activeTWAPs.length, 1);
    const running = emits.filter((emit) => emit.op === 'twap:stateUpdate').at(-1);
    assert.equal(Object.keys(running.data).length, 1, 'the running TWAP is persisted');
    assert.ok(host.algoTimer !== undefined, 'the host armed the slice timer');

    await venue.advance(29_000);
    assert.equal(venue.placements.length, 1, 'nothing before the interval');
    await venue.advance(2_000);
    assert.equal(venue.placements.length, 2, 'the second slice 30 s in, from the host timer');
    assert.deepEqual(venue.placements.map((order) => order.origQty), ['0.002', '0.002']);

    await venue.advance(1_000);
    assert.deepEqual(lastRows(emits).activeTWAPs, [], 'the TWAP completed');
    assert.deepEqual(emits.filter((emit) => emit.op === 'twap:stateUpdate').at(-1).data, {}, 'nothing left to restore');
  } finally { host.dispose(); }
});

test('algoDispose stops a resting chase, cancels its order and lets the host dispose', enabled, async () => {
  const { venue, host, emits } = await startedAccount();
  try {
    host.algoOp('chaser:start', { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10 });
    await venue.settle();
    assert.equal(venue.placements.length, 1);
    let done = false;
    const stopped = host.algoDispose('test').then(() => { done = true; });
    for (let i = 0; i < 20 && !done; i++) await venue.advance(500);
    await stopped;
    assert.equal(host.algosRunning, false);
    assert.deepEqual(venue.cancels, [String(venue.placements[0].orderId)]);
    assert.equal(host.algoTimer, undefined, 'no algo timer outlives the algos');
    assert.ok(emits.some((emit) => emit.op === 'error' && emit.data === 'Chases and TWAPs stopped: test'));
    assert.throws(() => host.algoOp('chaser:start', {}), /not running/);
  } finally { host.dispose(); }
});

const TWAP = { accountName: 'acct', activeMarket: { symbol: 'BTCUSDT' }, size: 0.004, duration: 1, ordersCount: 2,
  side: 'buy', chaserDelay: 1000, reduceOnly: false, randomnessPercentage: 0, useLimitOrders: false, useChaserOrders: false };
const stateUpdates = (emits) => emits.filter((emit) => emit.op === 'twap:stateUpdate').map((emit) => emit.data);

test('a TWAP slice is persisted in flight with the client id it is placed under, until it settles', enabled, async () => {
  const { venue, host, emits } = await startedAccount();
  try {
    host.algoOp('twap:start', TWAP);
    const [placing] = Object.values(stateUpdates(emits).at(-1));
    assert.equal(placing.state.inFlight.size, 0.002);
    assert.match(placing.state.inFlight.clientOrderId, /^x-s3vFFRQp[0-9a-f]{26}$/);
    assert.deepEqual(placing.state.ordersSizes, [0.002], 'the slice left ordersSizes before its order went out');
    await venue.settle();
    assert.equal(venue.placements[0].clientOrderId, placing.state.inFlight.clientOrderId);
    // The completion wakes the TWAP; the host timer then runs it.
    await venue.advance(10);
    const [placed] = Object.values(stateUpdates(emits).at(-1));
    assert.equal(placed.state.inFlight, undefined, 'cleared once the placement settled');
    assert.equal(placed.state.ordersExecuted, 1);
  } finally { host.dispose(); }
});

test('algoStopAll stops TWAPs and chases, waits for the chase cancels, and keeps the algos up', enabled, async () => {
  const { venue, host, emits } = await startedAccount();
  try {
    host.algoOp('chaser:start', { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10 });
    host.algoOp('twap:start', TWAP);
    await venue.settle();
    const rows = lastRows(emits);
    assert.equal(rows.activeChasers.length, 1);
    assert.equal(rows.activeTWAPs.length, 1);
    const chase = venue.placements.find((order) => order.type === 'LIMIT');

    let result;
    const stopping = host.algoStopAll().then((value) => { result = value; });
    assert.equal(result, undefined, 'answers only once the cancel settles');
    for (let i = 0; i < 20 && !result; i++) await venue.advance(250);
    await stopping;
    assert.deepEqual(result, { stoppedChasers: [rows.activeChasers[0].id], stoppedTwaps: [rows.activeTWAPs[0].id], settled: true });
    assert.deepEqual(venue.cancels, [String(chase.orderId)]);
    assert.deepEqual(stateUpdates(emits).at(-1), {}, 'the stopped TWAP is no longer persisted');
    assert.deepEqual(lastRows(emits), { activeChasers: [], activeTWAPs: [] });
    assert.ok(!emits.some((emit) => emit.op === 'algos:stopped'), 'the report answers the wait, not the worker');
    assert.equal(host.algosRunning, true);
    await venue.advance(60_000);
    assert.equal(venue.placements.length, 2, 'the stopped TWAP places no second slice');
  } finally { host.dispose(); }
});

test('an algoStopAll wait still answers when the host is disposed first', enabled, async () => {
  const { venue, host } = await startedAccount();
  host.algoOp('chaser:start', { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10 });
  await venue.settle();
  const stopping = host.algoStopAll();
  host.dispose();
  assert.equal((await stopping).settled, false);
});
