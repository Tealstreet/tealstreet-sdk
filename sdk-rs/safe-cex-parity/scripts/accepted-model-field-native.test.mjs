import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import test from 'node:test';
import {inventory} from './accepted-inventory.mjs';
import {loadContracts} from './accepted-contract-files.mjs';
import {loadPolicy} from './accepted-policy-files.mjs';
import {reviewedGenericDecoderHelper, requireNativeDecoderObservations} from './accepted-local-helpers.mjs';
import {buildScanner} from './accepted-scanner.mjs';
import {runProbes} from './accepted-probes.mjs';

const root = fileURLToPath(new URL('../..', import.meta.url));
const names = ['safe_cex_types::model_field::ModelField', 'safe_cex_types::model_field::deserialize_info',
  ...['PreparedOrderOpts', 'PreparedWire', 'deserialize_optional'].map(name => `safe_cex_types::prepared_order::${name}`)];
function cargo(args, env = {}) {
  const result = spawnSync('cargo', args, {cwd: root, encoding: 'utf8', timeout: 180000,
    maxBuffer: 64 * 1024 * 1024, env: {...process.env, CARGO_BUILD_JOBS: '1', ...env}});
  assert.equal(result.status, 0, `production compiler preparation: ${result.stderr}`);
  return result.stdout;
}

test('generic model field visitor has executed native decoder observations', {timeout: 300000}, () => {
  const metadata = JSON.parse(cargo(['metadata', '--locked', '--no-deps', '--format-version=1']));
  const pkg = metadata.packages.find(row => row.name === 'safe-cex-types');
  const target = pkg.targets.find(row => row.kind.includes('lib'));
  cargo(['rustdoc', '--locked', '-p', pkg.name, '--lib', '--jobs', '1', '--',
    '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
    '--document-hidden-items', '-A', 'rustdoc::all'], {RUSTC_BOOTSTRAP: '1'});
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
  const found = inventory(doc, {onlyOwners: names});
  assert.deepEqual(found.types.map(row => row.name).sort(), [...names].sort());
  const sourceFile = path.join(path.dirname(pkg.manifest_path), 'src/model_field.rs');
  const scan = spawnSync(buildScanner(1), ['--macro-origins-json', '[]', sourceFile], {cwd: root, encoding: 'utf8'});
  assert.equal(scan.status, 0, scan.stderr);
  const rows = scan.stdout.trim().split('\n').map(row => row.split('\t'));
  const generic = rows.filter(([,ident]) => ident === 'UNSUPPORTED_GENERIC:FieldVisitor');
  assert.equal(generic.length, 1, 'actual scanner must retain the generic local declaration');
  const [file, ident, line, column] = generic[0];
  const local = {file, ident, line: Number(line), column: Number(column), label: `${path.relative(root, file)}::${ident}`};
  const helpers = loadPolicy('local-type-helpers');
  const contracts = loadContracts(fileURLToPath(new URL('../accepted-set/contracts/safe-cex-types.json', import.meta.url)));
  const source = fs.readFileSync(sourceFile);
  const check = (localRow = local, policy = helpers, bytes = source, values = contracts) =>
    reviewedGenericDecoderHelper(localRow, policy, bytes, found.types, values);
  assert.equal(check(), true);
  assert.equal(check(local, {}), false);
  assert.throws(() => check({...local, line: local.line + 1}), /lexical declaration changed/);
  assert.throws(() => check(local, helpers, Buffer.concat([source, Buffer.from('\n')])), /generic helper source changed/);
  const noInstantiation = structuredClone(contracts);
  delete noInstantiation[names[0]].instantiation;
  assert.throws(() => check(local, helpers, source, noInstantiation), /fixed decoder monomorphization/);
  const noNative = structuredClone(contracts);
  delete noNative[names[0]].nativeObservation;
  assert.throws(() => check(local, helpers, source, noNative), /direct native observations/);
  const wrongFields = structuredClone(contracts);
  wrongFields[names[0]].nativeObservation.nativeFields.reverse();
  assert.throws(() => check(local, helpers, source, wrongFields), /generic helper native fields changed/);
  for (const name of names) requireNativeDecoderObservations(found.types.find(owner => owner.name === name), contracts[name], name);
  const results = runProbes(root, metadata, [{...found, package: pkg.name, packageId: pkg.id,
    target, features: [], publicAliases: [], defaultWitnesses: [], locals: [], delegatedLocals: [],
  }], contracts, {target: 'native', features: [], jobs: 1});
  assert.deepEqual(results.map(row => row.name).sort(), [...names].sort());
  for (const result of results) assert.deepEqual(result.failures, [], result.name);
});
