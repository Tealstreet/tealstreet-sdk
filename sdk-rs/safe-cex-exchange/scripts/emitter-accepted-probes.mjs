import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import {inventory} from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import {loadContracts} from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import {loadPolicy} from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
import {runProbes} from '../../safe-cex-parity/scripts/accepted-probes.mjs';
import {verifiedMacroOrigins} from '../../safe-cex-parity/scripts/accepted-macro-origins.mjs';
import {aliasTargetIdentity} from '../../safe-cex-parity/scripts/accepted-aliases.mjs';

const workspace = fileURLToPath(new URL('../..', import.meta.url));
const registry = process.argv.includes('--registry');
const packageName = registry ? 'safe-cex-registry' : 'safe-cex-exchange';
const owners = registry ? ['safe_cex_registry::native_account::host_error'] : [
  'safe_cex_exchange::exchange::BaseExchange',
  'safe_cex_exchange::exchange::emit_listeners',
  'safe_cex_exchange::error_reporting::raw_formatted_context',
  'safe_cex_exchange::runtime::ExchangeError',
];
const aliases = registry ? ['safe_cex_registry::native_account::NativeAccountFuture'] :
  ['safe_cex_exchange::runtime::Listener', 'safe_cex_exchange::runtime::ArgsListener'];
function cargo(args, extra = {}) {
  const result = spawnSync('cargo', ['+1.99.0', ...args], {
    cwd: workspace, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 900_000,
    env: {...process.env, CARGO_INCREMENTAL: '0', CARGO_BUILD_JOBS: '1', ...extra},
  });
  assert.equal(result.status, 0, `emitter probe compiler failure\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const source = spawnSync('node', [fileURLToPath(new URL('../tests/core-emitter-source.cjs', import.meta.url))], {
  encoding: 'utf8', timeout: 30_000,
  env: {...process.env, SAFE_CEX_TYPESCRIPT_PATH: process.env.SAFE_CEX_TYPESCRIPT_PATH ??
    path.join(workspace, 'target/asterdex-oracle-deps/node_modules/typescript')},
});
assert.equal(source.status, 0, `maintained emitter/formatter witness must execute\n${source.stderr}`);
const metadata = JSON.parse(cargo(['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(pkg => pkg.name === packageName);
assert.ok(pkg && metadata.workspace_members.includes(pkg.id), 'actual production package required');
cargo(['rustdoc', '--locked', '--jobs=1', '-p', pkg.name, '--lib', '--', '-Z', 'unstable-options',
  '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], {RUSTC_BOOTSTRAP: '1'});
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, `doc/${packageName.replaceAll('-', '_')}.json`)));
const found = inventory(doc, {
  defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'),
  readSource: filename => fs.readFileSync(path.join(workspace, filename)),
});
const artifacts = cargo(['check', '--locked', '--jobs=1', '-p', pkg.name, '--lib', '--message-format=json'])
  .trim().split('\n').filter(Boolean).map(line => JSON.parse(line)).filter(item => item.reason === 'compiler-artifact');
assert.ok(artifacts.some(item => item.package_id === pkg.id), 'actual selected package artifact required');
const sysroot = spawnSync('rustc', ['+1.99.0', '--print', 'sysroot'], {cwd: workspace, encoding: 'utf8'});
assert.equal(sysroot.status, 0, `actual compiler sysroot required\n${sysroot.stderr}`);
const macroOrigins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, sysroot.stdout.trim());
const members = metadata.packages.filter(item => metadata.workspace_members.includes(item.id));
for (const alias of found.externalTypeAliases.filter(alias => aliases.includes(alias.name))) {
  alias.artifact = aliasTargetIdentity(alias, artifacts, members, sysroot.stdout.trim());
}
const selected = {
  ...found, package: pkg.name, scope: {onlyOwners: owners}, defaultWitnesses: [], macroOrigins,
  types: found.types.filter(owner => owners.includes(owner.name)),
  publicAliases: found.publicAliases.filter(owner => aliases.includes(owner.name)),
};
assert.deepEqual(selected.types.map(owner => owner.name).sort(), [...owners].sort(), 'all selected production owners required');
assert.deepEqual(selected.publicAliases.map(owner => owner.name).sort(), [...aliases].sort(), 'all selected actual aliases required');
const receipts = runProbes(workspace, metadata, [selected], loadContracts(), {target: 'native', features: [], jobs: 1});
const failures = receipts.filter(receipt => receipt.failures.length);
for (const receipt of failures) console.error(JSON.stringify(receipt));
assert.equal(receipts.length, owners.length + aliases.length, 'all selected APIs and alias traits must execute');
assert.equal(failures.length, 0, 'selected emitter native/default contracts must pass');
console.log(`${receipts.length} selected ${packageName} emitter owner/alias receipts pass; this is not a full-package or feature-matrix claim.`);
