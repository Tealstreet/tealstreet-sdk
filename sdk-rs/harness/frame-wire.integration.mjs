// Cross-language contract: Rust's actual writer -> JS decoder -> SDK book state.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { Frame } from '../hosts/wasm/js/wire.mjs';
import { captureRustFill } from './comparison-policy.mjs';
import { FrameApplier } from '../../packages/safe-cex/src/store/frameApplier.ts';
import { DefaultStore } from '../../packages/safe-cex/src/store/store.base.ts';

test('Rust BOOK_DELTA reconstructs two full 1,000-level books through FrameApplier', () => {
  const directory = mkdtempSync(join(tmpdir(), 'safe-cex-frame-wire-'));
  try {
    const result = spawnSync('cargo', ['run', '--quiet', '--locked', '--manifest-path',
      'sdk-rs/Cargo.toml', '-p', 'safe-cex-engine', '--example', 'book_bandwidth', '--', directory],
    { encoding: 'utf8', timeout: 120_000 });
    assert.equal(result.status, 0, result.stderr || result.error?.message);
    const measurement = JSON.parse(result.stdout);
    assert.equal(measurement.full_frame_bytes, 96_064);
    assert.equal(measurement.delta_frame_bytes, 704);
    const read = (name) => {
      const bytes = readFileSync(join(directory, name));
      return new Frame(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
    };
    const deltaApplier = new FrameApplier(new DefaultStore());
    const fullApplier = new FrameApplier(new DefaultStore());
    for (const applier of [deltaApplier, fullApplier]) applier.apply(read('markets.bin'));
    const baseline = read('baseline.bin');
    deltaApplier.apply(baseline);
    const retained = deltaApplier.books(baseline);
    const delta = read('delta.bin');
    deltaApplier.apply(delta);
    const full = read('full-after.bin');
    fullApplier.apply(full);
    assert.deepEqual(deltaApplier.books(delta), fullApplier.books(full));
    assert.equal(deltaApplier.books(delta).length, 2);
    for (const book of deltaApplier.books(delta)) {
      assert.equal(book.orderbook.bids.length, 1000);
      assert.equal(book.orderbook.asks.length, 1000);
      assert.equal(book.orderbook.bids[999].total, 1020);
      assert.equal(book.orderbook.asks[999].total, 2020);
    }
    assert.equal(retained[0].orderbook.bids[999].total, 1000);
    assert.equal(retained[0].orderbook.asks[999].total, 2000);
    console.log(JSON.stringify(measurement));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('Rust revised sections preserve native fields and identity through the SDK', async (t) => {
  const result = spawnSync('cargo', ['run', '--quiet', '--locked', '--manifest-path',
    'sdk-rs/Cargo.toml', '-p', 'safe-cex-engine', '--example', 'wire_contract_frames'],
  { encoding: 'utf8', timeout: 120_000 });
  assert.equal(result.status, 0, result.stderr || result.error?.message);
  const fixtures = JSON.parse(result.stdout);
  const read = (name) => {
    const bytes = Uint8Array.from(Buffer.from(fixtures[name], 'hex'));
    return new Frame(bytes.buffer);
  };
  const store = new DefaultStore();
  const applier = new FrameApplier(store);
  const initial = read('initial');
  applier.apply(initial);
  await t.test('H2: display symbols retain native, empty and absent values', () => {
    assert.equal(store.markets[0].prettySymbol, 'Native A / USD');
    assert.equal(store.markets[1].prettySymbol, '');
    assert.equal(store.markets[2].prettySymbol, undefined);
  });
  await t.test('H3: orphan orders, positions, executions and settings retain symbols', () => {
    assert.deepEqual(store.orders.map((row) => row.symbol), ['A', 'ORPHAN']);
    assert.deepEqual(store.positions.map((row) => row.symbol), ['A', 'ORPHAN']);
    assert.deepEqual(applier.fills(initial).map((row) => [row.orderId, row.symbol, row.amount]),
      [['tracked-order', 'A', 0.25], ['orphan-order', 'ORPHAN', 0.25]]);
    assert.equal(initial.first(11).revision, 2);
    assert.deepEqual(applier.fills(initial).map((row) => row.id), ['9007199254740997', 'orphan-order']);
    const table = initial.fills();
    const captures = Array.from({ length: table.count }, (_, i) => captureRustFill(table, i));
    assert.deepEqual(captures.map(({ id, executionId, fee }) => ({ id, executionId, fee })), [
      { id: 'tracked-order', executionId: '9007199254740997', fee: -0.25 },
      { id: 'orphan-order', executionId: undefined, fee: undefined },
    ]);
    assert.equal(store.fetchedLeverage.ORPHAN.buyLeverage, 11);
    assert.equal(store.marketsBySymbol.ORPHAN, undefined);
  });
  await t.test('M3: native order extras and independent bracket triggers survive', () => {
    assert.equal(store.orders[0].tpTriggerType, 'index_price');
    assert.equal(store.orders[0].slTriggerType, 'last_price');
    assert.equal(store.orders[0].extra.remainingNative, 7);
    assert.equal(store.orders[0].extra.filledInPlacementUnits, 3);
    assert.equal(store.orders[0].extra.remainingNotional, 42);
    assert.equal(store.orders[0].extra.conditionalOrderPrice, 125);
    assert.equal(store.orders[1].extra, undefined);
  });
  await t.test('M3: midpoint snapshots, deltas and clearing preserve absence', () => {
    assert.equal(store.tickers[0].midPrice, 101.25);
    assert.equal('midPrice' in store.tickers[1], false);
    applier.apply(read('deltaMid'));
    assert.equal(store.tickers[0].midPrice, 102.5);
    applier.apply(read('clearMid'));
    assert.equal('midPrice' in store.tickers[0], false);
  });
  await t.test('M1: market reorder preserves settings and queues market-flush rows', () => {
    const reordered = read('reordered');
    applier.apply(reordered);
    assert.deepEqual(store.markets.map((row) => row.symbol), ['B', 'A']);
    assert.deepEqual([store.fetchedLeverage.A.buyLeverage, store.fetchedLeverage.A.sellLeverage,
      store.fetchedLeverage.B.buyLeverage, store.fetchedLeverage.ORPHAN.buyLeverage], [6, 7, 9, 11]);
    assert.deepEqual(store.orders.map((row) => row.symbol), ['A', 'ORPHAN']);
    assert.deepEqual(applier.fills(reordered).map((row) => row.symbol), ['A', 'ORPHAN']);
    assert.equal(applier.candles(reordered)[0].symbol, 'A');
    assert.equal(applier.books(reordered)[0].symbol, 'A');
    const resumed = read('resumed');
    applier.apply(resumed);
    assert.deepEqual(applier.fills(resumed).map((row) => row.symbol), ['A', 'ORPHAN']);
  });
});
