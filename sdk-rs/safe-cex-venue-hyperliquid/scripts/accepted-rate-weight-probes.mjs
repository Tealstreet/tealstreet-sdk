// Execute the reviewed request-weight function contract using fresh normal compiler discovery.
// This bounded reproduction does not claim package/local/dependency/matrix coverage.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { aliasTargetIdentity } from '../../safe-cex-parity/scripts/accepted-aliases.mjs';
import { verifiedMacroOrigins } from '../../safe-cex-parity/scripts/accepted-macro-origins.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';

process.env.RUSTUP_TOOLCHAIN = '1.99.0';
process.env.RUSTC_BOOTSTRAP = '1';
process.env.CARGO_BUILD_JOBS = '1';
assert.equal(process.argv.length, 2, 'native default profile only');
const root = fileURLToPath(new URL('../../', import.meta.url));
const selected = 'native', targetArgs = [];
const key = 'safe_cex_venue_hyperliquid::rate_limit::get_hyperliquid_rest_request_weight';
function command(binary, args) {
  const result = spawnSync(binary, args, {cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: {...process.env, RUSTC_BOOTSTRAP: '1', CARGO_BUILD_JOBS: '1', RUSTUP_TOOLCHAIN: '1.99.0'}});
  assert.equal(result.status, 0, `${binary} ${args.join(' ')} failed\n${result.stdout}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(command('cargo', ['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(p => p.name === 'safe-cex-venue-hyperliquid');
const target = pkg.targets.find(t => t.kind.includes('lib'));
command('cargo', ['rustdoc', '--locked', '-p', pkg.name, '--jobs', '1', '--lib', ...targetArgs, '--', '-Z', 'unstable-options', '--output-format', 'json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all']);
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, ...(targetArgs.length ? [selected] : []), 'doc', target.name + '.json')));
const artifacts = command('cargo', ['check', '--locked', '-p', pkg.name, '--jobs', '1', '--lib', ...targetArgs, '--message-format=json']).trim().split('\n').filter(Boolean).map(line => JSON.parse(line)).filter(item => item.reason === 'compiler-artifact');
const own = artifacts.filter(item => item.package_id === pkg.id && item.target.name === target.name);
assert.equal(own.length, 1, 'unique actual production host artifact');
const members = metadata.packages.filter(p => metadata.workspace_members.includes(p.id));
const sysroot = command('rustc', ['--print', 'sysroot']).trim();
const macroOrigins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, sysroot);
// Retain the complete actual compiler inventory before selecting one owner.
const found = inventory(doc, {defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'), readSource: file => fs.readFileSync(path.resolve(root, file))});
assert.equal(found.types.length, 534);
assert.equal(found.publicAliases.length, 36);
const owners = found.types.filter(owner => owner.name === key);
assert.equal(owners.length, 1);
assert.equal(owners[0].kind, 'function');
const externalTypeAliases = found.externalTypeAliases.map(alias => ({...alias, artifact: aliasTargetIdentity(alias, artifacts, members, sysroot)}));
const all = loadContracts();
assert.ok(all[key]);
const contracts = {[key]:all[key]};
const rows = runProbes(root, metadata, [{package: pkg.name, target, features: own[0].features, types: owners, locals: [], publicAliases: [], macroOrigins, externalTypeAliases}], contracts, {target: selected, features: [], jobs: 1});
assert.equal(rows.length, 1);
assert.deepEqual(rows.filter(row => row.failures.length), [], 'actual request-weight owner must pass the ordinary native runner');
assert.ok(rows.every(row => !row.classificationOnly), 'real runtime receipt required');
console.log('REQUEST_WEIGHT_CONTRACT ' + JSON.stringify({rows, allOwnersRetained: found.types.length, allAliasesRetained: found.publicAliases.length, packageClearance:false}));
