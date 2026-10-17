import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import test from 'node:test';
import {inventory} from './accepted-inventory.mjs';
import {loadContracts} from './accepted-contract-files.mjs';
import {requireNativeDecoderObservations} from './accepted-local-helpers.mjs';
import {runProbes} from './accepted-probes.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['Order', 'Position', 'OrderRiskProfile'].map(name => `safe_cex_types::${name}`);
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {
    cwd: root, encoding: 'utf8', timeout: 180000, maxBuffer: 64 * 1024 * 1024,
    env: {...process.env, CARGO_BUILD_JOBS: '1', ...env},
  });
  assert.equal(result.status, 0, `production compiler preparation: ${result.stderr}`);
  return result.stdout;
}

test('model record visitors have executed native field observations', {timeout: 300000}, () => {
  const metadata = JSON.parse(cargo(['metadata', '--locked', '--no-deps', '--format-version=1']));
  const pkg = metadata.packages.find(row => row.name === 'safe-cex-types');
  const target = pkg.targets.find(row => row.kind.includes('lib'));
  cargo(['rustdoc', '--locked', '-p', pkg.name, '--lib', '--jobs', '1', '--',
    '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
    '--document-hidden-items', '-A', 'rustdoc::all'], {RUSTC_BOOTSTRAP: '1'});
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
  const found = inventory(doc, {onlyOwners: names});
  assert.deepEqual(found.types.map(row => row.name).sort(), [...names].sort());
  const contracts = loadContracts(fileURLToPath(new URL('../accepted-set/contracts/safe-cex-types.json', import.meta.url)));
  for (const name of names) {
    const owner = found.types.find(row => row.name === name);
    requireNativeDecoderObservations(owner, contracts[owner.name], `RecordVisitor@${owner.ident}`);
  }
  const results = runProbes(root, metadata, [{...found, package: pkg.name, packageId: pkg.id,
    target, features: [], publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
  }], contracts, {target: 'native', features: [], jobs: 1});
  assert.deepEqual(results.map(row => row.name).sort(), [...names].sort());
  for (const result of results) assert.deepEqual(result.failures, [], result.name);
});
