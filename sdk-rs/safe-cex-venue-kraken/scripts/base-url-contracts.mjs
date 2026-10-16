// One reviewed string-union owner only; other Kraken contracts remain open.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...env },
  });
  assert.equal(result.status, 0, `${args.join(' ')} failed\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const selected = ['safe_cex_venue_kraken::types::KrakenBaseUrl'];
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(value => value.name === 'safe-cex-venue-kraken');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual Kraken compiler package required');
const target = pkg.targets.find(value => value.kind.includes('lib'));
assert.ok(target, 'actual production library required');
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], { RUSTC_BOOTSTRAP: '1' });
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc/safe_cex_venue_kraken.json'), 'utf8'));
const found = inventory(doc, {
  onlyOwners: selected,
  readSource: file => fs.readFileSync(path.join(workspace, file)),
});
const types = found.types.filter(value => selected.includes(value.name));
assert.deepEqual(types.map(value => value.name).sort(), selected);
assert.ok(types.every(value => value.deserialize && value.kind === 'enum'));
// Remote derive emits a private associated method, not a Deserialize owner.
// Check actual compiler identity rather than inferring this from derive syntax.
const companion = Object.values(doc.index).find(item => item.crate_id === 0
  && item.name === 'KrakenBaseUrlSerde' && item.inner.enum);
assert.ok(companion && companion.visibility !== 'public', 'private remote companion required');
for (const id of companion.inner.enum.impls) {
  const impl = doc.index[String(id)]?.inner.impl;
  if (!impl || impl.is_synthetic || impl.is_negative || impl.blanket_impl) continue;
  assert.equal(String(impl.for?.resolved_path?.id), String(companion.id));
  const trait = doc.paths[String(impl.trait?.id)]?.path.join('::');
  assert.ok(!['serde::de::Deserialize', 'serde_core::de::Deserialize'].includes(trait),
    'remote companion acquired an independent Deserialize obligation');
  if (!impl.trait) for (const method of impl.items.map(id => doc.index[String(id)]))
    assert.notEqual(method.visibility, 'public', 'remote companion acquired a public entrypoint');
}
console.log('Selected compiler inventory: one enum with actual Deserialize; private remote companion has no Deserialize/public entrypoint.');
console.log('Probe scope: one native/default owner only. Whole-crate receiver refusals, other owners, aliases, locals, construction and feature/wasm coverage remain open. Run accepted probes serially.');
const results = runProbes(workspace, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types,
  publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
assert.equal(results.length, selected.length);
for (const result of results) assert.equal(result.failures.length, 0,
  `${result.name}: ${result.failures.join('; ')}`);
assert.ok(results.every(result => !result.classificationOnly), 'actual runtime production probe required');
console.log('One string-union production probe passed; zero failures. Other Kraken owners remain open.');
