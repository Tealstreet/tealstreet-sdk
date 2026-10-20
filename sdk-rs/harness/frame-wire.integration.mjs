// Cross-language contract: Rust's actual writer -> JS decoder -> SDK book state.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { Frame } from '../hosts/wasm/js/wire.mjs';
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
