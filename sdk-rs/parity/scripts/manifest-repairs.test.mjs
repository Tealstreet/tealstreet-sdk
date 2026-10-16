import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { diagnosticCensus } from './diagnostic-census.mjs';

const require = createRequire(import.meta.url);
const { parseManifest } = require('../extract-ts/src/manifest.ts');
const root = fileURLToPath(new URL('../../../', import.meta.url));
const crossSource = 'packages/safe-cex/src/crossTrade/runtime/CrossTradeManager.ts';
const backpackSource = 'packages/safe-cex/src/exchanges/backpack/backpack.api.ts';

function manifest(relative, source) {
  const file = path.join(root, 'sdk-rs/parity/manifest', relative);
  return parseManifest(readFileSync(file, 'utf8'), source, file);
}

test('CrossTrade names the three independently expected rename targets', () => {
  const data = manifest('crossTrade/runtime/CrossTradeManager.ts.toml', crossSource);
  for (const [symbol, target] of [
    ['CrossTradeManager#_failureKey', 'safe-cex-exchange/src/cross_trade/manager.rs::CrossTradeManager::failure_key'],
    ['CrossTradeManager#_recordSuccess', 'safe-cex-exchange/src/cross_trade/manager.rs::CrossTradeManager::record_success'],
    ['CrossTradeManager#_meetsMinAmount', 'safe-cex-exchange/src/cross_trade/manager.rs::CrossTradeManager::meets_min_amount'],
  ]) {
    const row = data.symbol.find(row => row.ts === symbol && row.kind === 'method');
    assert.ok(row, `CROSSTRADE source row exists: ${symbol}`);
    assert.equal(row.rs, target, `CROSSTRADE native target: ${symbol}`);
    assert.equal(row.layer, 'adapted');
    assert.equal(row.status, 'ported');
    assert.ok(row.note.split(/\s+/).includes(target), `CROSSTRADE explicit rename token: ${symbol}`);
  }
});

test('Backpack declares exactly its helper and factory files', () => {
  const data = manifest('exchanges/backpack/backpack.api.ts.toml', backpackSource);
  assert.deepEqual(data.file.rust_files, [
    'safe-cex-venue-backpack/src/api.rs',
    'safe-cex-venue-backpack/src/api_factory.rs',
  ], 'BACKPACK native source declarations');
  const row = data.symbol.find(row => row.ts === 'createAPI' && row.kind === 'const-fn');
  assert.ok(row, 'BACKPACK source factory row exists');
  assert.equal(row.rs, 'safe-cex-venue-backpack/src/api_factory.rs::create_api');
  assert.equal(row.layer, 'adapted');
  assert.equal(row.status, 'divergent');
});

test('maintained checker accepts the repaired source annotations', () => {
  const result = spawnSync(path.join(root, 'sdk-rs/target/debug/parity-check'), ['--workspace', 'sdk-rs'], {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
  });
  assert.ifError(result.error);
  const output = result.stdout + result.stderr;
  if (result.status === 0) {
    assert.match(output, /^parity: invention and status gates passed /);
  } else {
    assert.equal(result.status, 1, 'REPAIR checker returned a validation result');
    diagnosticCensus(output);
  }
  const remaining = output.split('\n').filter(line =>
    line.startsWith(`status: ${crossSource}`) || line.startsWith(`status: ${backpackSource}`));
  assert.deepEqual(remaining, [], 'REPAIR selected source declarations validate');
});
