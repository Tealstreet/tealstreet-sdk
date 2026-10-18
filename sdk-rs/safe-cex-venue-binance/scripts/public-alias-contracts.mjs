// Selected public aliases discovered by the maintained compiler inventory.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['Dictionary', 'TickerReference']
  .map(name => `safe_cex_venue_binance::ticker_capture::${name}`).sort();
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--expect-unclassified'));
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {
    cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, CARGO_BUILD_JOBS: '1', ...env },
  });
  assert.equal(result.status, 0, `${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(pkg => pkg.name === 'safe-cex-venue-binance');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
const target = pkg.targets.find(target => target.kind.includes('lib'));
const discovery = path.join(metadata.target_directory, 'accepted-inventory');
cargo(['rustdoc', '--locked', '-p', pkg.name, '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all'], {
  RUSTC_BOOTSTRAP: '1', CARGO_TARGET_DIR: discovery,
});
const found = inventory(JSON.parse(fs.readFileSync(path.join(discovery, 'doc', `${target.name}.json`))), {
  defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'),
  readSource: file => fs.readFileSync(path.join(root, file)),
});
const aliases = found.publicAliases.filter(alias => names.includes(alias.name));
assert.deepEqual(aliases.map(alias => alias.name).sort(), names);
assert.ok(aliases.every(alias => alias.generics.params.length === 0));
fs.writeFileSync(path.join(metadata.target_directory, 'public-alias-inventory.json'),
  `${JSON.stringify(aliases, null, 2)}\n`);
const results = runProbes(root, metadata, [{
  package: pkg.name, packageId: pkg.id, target, types: [], publicAliases: aliases,
  defaultWitnesses: [], locals: [], delegatedLocals: [], macroOrigins: [],
  externalTypeAliases: found.externalTypeAliases,
}], loadContracts(), { target: 'native', features: [], jobs: 1 });
fs.writeFileSync(path.join(metadata.target_directory, 'public-alias-results.json'),
  `${JSON.stringify(results, null, 2)}\n`);
assert.deepEqual(results.map(result => result.name).sort(), names);
if (args.includes('--expect-unclassified')) {
  assert.ok(results.every(result => result.failures.some(failure =>
    failure.includes('public alias needs an accepted-set contract'))));
  console.log('Both ticker-capture aliases fail missing classification by name.');
} else {
  for (const result of results) assert.deepEqual(result.failures, [], result.name);
  console.log('Both actual ticker-capture aliases pass native compiler trait probes.');
}
