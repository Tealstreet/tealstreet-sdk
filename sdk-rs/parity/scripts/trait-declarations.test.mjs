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

import { families } from './trait-declaration-families.mjs';

assert.equal(families.length, 30, 'TRAIT ranked family floor stops at 30');
const sources = [
  ['grvt', 'grvt.ws-private'], ['grvt', 'grvt.builder'],
  ['woo', 'woo.exchange'], ['woo', 'woo.ws-private'], ['woo', 'woo.ws-public'],
  ['bitrue', 'bitrue.exchange'], ['backpack', 'backpack.exchange'],
  ['kraken', 'kraken.exchange'], ['kraken', 'krakenspot.exchange'],
  ['grvt', 'grvt.ws-public'], ['grvt', 'grvt.exchange'], ['grvt', 'grvt.api'],
  ['backpack', 'backpack.ws-public'], ['bybit', 'bybit.exchange'],
  ['hyperliquid', 'hyperliquidspot.exchange'],
  ['mexc', 'mexc.exchange'], ['orderly', 'orderly.exchange'],
  ['orderly', 'orderly.ws-private'], ['orderly', 'orderly.ws-public'],
  ['backpack', 'backpack.api'], ['hyperliquid', 'hyperliquid.exchange'],
  ['kraken', 'kraken.ws-private'], ['kraken', 'kraken.ws-public'],
  ['kraken', 'krakenspot.ws-private'], ['lighter', 'lighter.exchange'],
];
const manifests = sources.map(([venue, stem]) => {
  const file = path.join(root, `sdk-rs/parity/manifest/exchanges/${venue}/${stem}.ts.toml`);
  return parseManifest(readFileSync(file, 'utf8'), `packages/safe-cex/src/exchanges/${venue}/${stem}.ts`, file);
});
const mapped = manifests.flatMap(data => data.symbol.map(row => ({ row, file: data.file })));
const run = (binary, args) => {
  const result = spawnSync(path.join(root, `sdk-rs/target/debug/${binary}`), args, {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
  });
  assert.ifError(result.error);
  return { status: result.status, output: result.stdout + result.stderr };
};
const inventory = run('parity-trait-inventory', ['sdk-rs']);
assert.equal(inventory.status, 0, 'TRAIT production AST inventory must complete');
const declarations = inventory.output.trimEnd().split('\n').map(line => {
  const parts = line.split('\t');
  assert.equal(parts.length, 2, 'TRAIT inventory record shape');
  assert.ok(['trait', 'trait-member'].includes(parts[0]));
  return parts[1];
});

for (const [venue, module, owner, count, stem, symbol, member, unassigned = []] of families) {
  const file = `safe-cex-venue-${venue}/src/${module}.rs`;
  const family = `${file}::${owner}`;
  test(`TRAIT native declarations: ${family}`, () => {
    const expected = declarations.filter(target => target === family || target.startsWith(`${family}::`)).sort();
    assert.equal(expected.length, count, `TRAIT independent source floor: ${family}`);
    const actual = [];
    for (const { row, file: source } of mapped) {
      const targets = Array.isArray(row.rs) ? row.rs : [row.rs];
      const selected = targets.filter(target => target === family || target.startsWith(`${family}::`));
      if (!selected.length) continue;
      assert.equal(row.layer, 'adapted', `TRAIT adapted operation: ${row.ts}`);
      assert.ok(['ported', 'divergent'].includes(row.status), `TRAIT implemented operation: ${row.ts}`);
      assert.ok(source.rust_files?.includes(file), `TRAIT source provenance: ${row.ts}`);
      assert.match(row.note, /API split: .+ supply .+/, `TRAIT explained API roles: ${row.ts}`);
      for (const target of selected) {
        assert.ok(row.note.split(/\s+/).includes(target), `TRAIT explicit role target: ${target}`);
        actual.push(target);
      }
    }
    const outstanding = unassigned.map(member => `${family}::${member}`);
    assert.ok(outstanding.every(target => expected.includes(target)), `TRAIT outstanding source identity: ${family}`);
    assert.ok(outstanding.every(target => !actual.includes(target)), `TRAIT unassigned capabilities remain unmapped: ${family}`);
    assert.deepEqual(actual.sort(), expected.filter(target => !outstanding.includes(target)), `TRAIT native declared ownership: ${family}`);
    const witness = mapped.find(({ row, file }) => row.ts === symbol && file.ts.endsWith(`/${stem}.ts`));
    assert.ok(witness, `TRAIT independent consuming operation: ${symbol}`);
    assert.ok(witness.row.rs.includes(`${family}::${member}`), `TRAIT operation-specific API: ${symbol} -> ${member}`);
  });
}

test('TRAIT maintained checker accepts every selected family declaration', () => {
  const result = run('parity-check', ['--workspace', 'sdk-rs']);
  if (result.status === 0) {
    assert.match(result.output, /^parity: invention and status gates passed /);
  } else {
    assert.equal(result.status, 1, 'TRAIT checker must return manifest diagnostics');
    diagnosticCensus(result.output);
  }
  const selected = result.output.split('\n').filter(line => families.some(([venue, module, owner]) =>
    line.includes(`safe-cex-venue-${venue}/src/${module}.rs::${owner}`)));
  const outstanding = families.flatMap(([venue, module, owner, , , , , unassigned = []]) =>
    unassigned.map(member => `invention: safe-cex-venue-${venue}/src/${module}.rs::${owner}::${member}: public item has no manifest rs or exemption`));
  assert.deepEqual(selected.sort(), outstanding.sort(), 'TRAIT maintained selected ownership validates');
});
