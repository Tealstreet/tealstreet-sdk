import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { families } from './trait-declaration-families.mjs';

const require = createRequire(import.meta.url);
const { parseManifest, serializeManifest } = require('../extract-ts/src/manifest.ts');
const root = fileURLToPath(new URL('../../../', import.meta.url));
const snapshots = new Map();
const inputs = new Map();
const start = Number(process.argv[2] ?? '1');
assert.ok(Number.isInteger(start) && start >= 1 && start <= families.length, 'TRAIT valid first family index');
const targets = families.slice(start - 1).map(([venue, module, owner]) => `safe-cex-venue-${venue}/src/${module}.rs::${owner}`);
const sha = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim();
const run = phase => {
  const result = spawnSync(process.execPath, [
    '--import', 'tsx', '--test', '--test-reporter=tap', 'sdk-rs/parity/scripts/trait-declarations.test.mjs',
  ], { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
  assert.ifError(result.error);
  const output = result.stdout + result.stderr;
  const directory = path.join(root, 'sdk-rs/target/trait-declaration-controls', sha, String(start));
  mkdirSync(directory, { recursive: true });
  writeFileSync(path.join(directory, `${phase}.log`), output);
  return { status: result.status, output };
};

for (const [venue, stem] of [
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
]) {
  const file = path.join(root, `sdk-rs/parity/manifest/exchanges/${venue}/${stem}.ts.toml`);
  const clean = spawnSync('git', ['diff', '--quiet', 'HEAD', '--', file], { cwd: root });
  assert.ifError(clean.error);
  assert.equal(clean.status, 0, 'TRAIT control requires manifests identical to HEAD');
  const bytes = readFileSync(file);
  snapshots.set(file, bytes);
  inputs.set(file, parseManifest(bytes.toString('utf8'), `packages/safe-cex/src/exchanges/${venue}/${stem}.ts`, file));
}
const baseline = run('baseline');
assert.equal(baseline.status, 0, 'TRAIT baseline must pass');
assert.ok(baseline.output.includes(`# tests ${families.length + 1}\n`));
for (const mode of ['owners', 'unassigned']) {
  const current = new Map([...snapshots].map(([file, bytes]) => [file,
    parseManifest(bytes.toString('utf8'), inputs.get(file).file.ts, file)]));
  const mutations = mode === 'owners' ? targets : families.slice(start - 1).flatMap(
    ([venue, module, owner, , , , , unassigned = []]) =>
      unassigned.map(member => `safe-cex-venue-${venue}/src/${module}.rs::${owner}::${member}`));
  if (!mutations.length) continue;
  let mutated;
  let failure;
  try {
    for (const target of mutations) {
      let edits = 0;
      for (const data of current.values()) for (const row of data.symbol) {
        if (!Array.isArray(row.rs)) continue;
        if (mode === 'owners' && row.rs.includes(target)) {
          row.rs = row.rs.filter(item => item !== target);
          edits += 1;
        } else if (mode === 'unassigned' && row.rs.includes(target.slice(0, target.lastIndexOf('::')))) {
          assert.ok(!row.rs.includes(target), 'TRAIT unassigned control starts without false claim');
          row.rs.push(target);
          row.note += ` Deliberate false ownership control: ${target} is claimed without a production call site.`;
          edits += 1;
        }
      }
      assert.equal(edits, 1, `TRAIT exactly one declaration mutation: ${target}`);
    }
    for (const [file, data] of current) writeFileSync(file, serializeManifest(data));
    mutated = run(`${mode}-mutated`);
    assert.equal(mutated.status, 1, 'TRAIT mutated declarations must fail');
    assert.ok(mutated.output.includes(`# tests ${families.length + 1}\n`));
    assert.ok(mutated.output.includes(`# fail ${mutations.length + 1}\n`));
    for (const target of mutations) {
      if (mode === 'owners') {
        assert.ok(mutated.output.includes(`TRAIT native declared ownership: ${target}`), `TRAIT named native failure: ${target}`);
        assert.ok(mutated.output.includes(`invention: ${target}: public item has no manifest rs or exemption`), `TRAIT maintained invention failure: ${target}`);
      } else {
        const family = target.slice(0, target.lastIndexOf('::'));
        assert.ok(mutated.output.includes(`TRAIT unassigned capabilities remain unmapped: ${family}`), `TRAIT false ownership rejected: ${target}`);
      }
    }
    assert.ok(mutated.output.includes('TRAIT maintained selected ownership validates'), 'TRAIT canonical remainder assertion discriminates');
  } catch (error) {
    failure = error;
  } finally {
    for (const [file, bytes] of snapshots) writeFileSync(file, bytes);
  }
  const restored = run(`${mode}-restored`);
  assert.equal(restored.status, 0, 'TRAIT restored declarations must pass');
  assert.ok(restored.output.includes(`# tests ${families.length + 1}\n`));
  assert.match(restored.output, /^# fail 0$/m);
  for (const [file, bytes] of snapshots) assert.deepEqual(readFileSync(file), bytes, `TRAIT exact restoration: ${file}`);
  if (failure) throw failure;
  console.log(JSON.stringify({ sha, start, mode, baseline: baseline.status, mutated: mutated.status, restored: restored.status,
    nativeFailures: mutations.length, exactBytesRestored: true }));
}
