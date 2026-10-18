// Execute the six reviewed alias contracts using fresh normal compiler discovery.
// This bounded reproduction does not claim package/local/dependency/matrix coverage.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { aliasClaim, aliasTargetIdentity } from '../../safe-cex-parity/scripts/accepted-aliases.mjs';
import { verifiedMacroOrigins } from '../../safe-cex-parity/scripts/accepted-macro-origins.mjs';
import { buildScanner } from '../../safe-cex-parity/scripts/accepted-scanner.mjs';
import { runProbes } from '../../safe-cex-parity/scripts/accepted-probes.mjs';

process.env.RUSTUP_TOOLCHAIN = '1.99.0';
process.env.RUSTC_BOOTSTRAP = '1';
process.env.CARGO_BUILD_JOBS = '1';
const args = process.argv.slice(2);
assert.ok(args.length <= 1 && args.every(arg => arg === '--target=wasm32-unknown-unknown'), 'only the explicit Wasm target selection is supported');
const selected = args.length ? 'wasm32-unknown-unknown' : 'native';
const targetArgs = selected === 'native' ? [] : ['--target', selected];
const root = fileURLToPath(new URL('../../', import.meta.url));
const keys = [
  'private_class::PrivateValue', 'private_class::PrivateError',
  'public_class::PublicValue', 'public_class::PublicError',
  'core_exchange_class::ExchangeValue', 'core_exchange_class::ExchangeError',
].map(name => 'safe_cex_venue_hyperliquid::' + name);
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
// Run full inventory before selecting six; every actual owner/alias remains indexed.
const found = inventory(doc, {defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'), readSource: file => fs.readFileSync(path.resolve(root, file))});
assert.equal(found.types.length, 534);
assert.equal(found.publicAliases.length, 36);
const aliases = found.publicAliases.filter(alias => keys.includes(alias.name));
assert.deepEqual(aliases.map(alias => alias.name).sort(), [...keys].sort());
assert.ok(aliases.every(alias => alias.projectionIdentity), 'fresh compiler associated bindings required');
const externalTypeAliases = found.externalTypeAliases.map(alias => ({...alias, artifact: aliasTargetIdentity(alias, artifacts, members, sysroot)}));
const scanner = buildScanner('1');
const claims = JSON.parse(command(scanner, ['--alias-claims-json', root, ...new Set(aliases.map(alias => alias.span.filename))]));
const all = loadContracts();
const contracts = Object.fromEntries(keys.map(key => {assert.ok(all[key], key + ' reviewed contract absent');return [key, all[key]];}));
for (const alias of aliases) alias.claim = aliasClaim(alias, {packageName: pkg.name, crateRoot: path.dirname(pkg.manifest_path), workspace: root, claims}, contracts[alias.name]);
const rows = runProbes(root, metadata, [{package: pkg.name, target, features: own[0].features, types: [], locals: [], publicAliases: aliases, macroOrigins, externalTypeAliases}], contracts, {target: selected, features: [], jobs: 1});
assert.equal(rows.length, 6);
assert.deepEqual(rows.filter(row => row.failures.length), [], 'all six reviewed contracts must pass normal target-specific alias probes');
console.log('SIX_PROJECTION_CONTRACTS ' + JSON.stringify({target: selected, rows, allOwnersRetained: found.types.length, allAliasesRetained: found.publicAliases.length, packageClearance: false}));
