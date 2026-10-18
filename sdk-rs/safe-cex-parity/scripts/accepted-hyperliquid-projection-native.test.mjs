import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { registerHooks } from 'node:module';
import test from 'node:test';
import { inventory } from './accepted-inventory.mjs';
import { loadPolicy } from './accepted-policy-files.mjs';
import { aliasClaim, aliasTargetIdentity } from './accepted-aliases.mjs';
import { verifiedMacroOrigins } from './accepted-macro-origins.mjs';
import { buildScanner } from './accepted-scanner.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const projectionNames = [
  'safe_cex_venue_hyperliquid::private_class::PrivateValue',
  'safe_cex_venue_hyperliquid::private_class::PrivateError',
  'safe_cex_venue_hyperliquid::public_class::PublicValue',
  'safe_cex_venue_hyperliquid::public_class::PublicError',
  'safe_cex_venue_hyperliquid::core_exchange_class::ExchangeValue',
  'safe_cex_venue_hyperliquid::core_exchange_class::ExchangeError',
];
const specs = Object.fromEntries(Object.entries(loadPolicy('generic-aliases')).filter(([name]) => projectionNames.includes(name)));
assert.deepEqual(Object.keys(specs).sort(), [...projectionNames].sort(), 'all six reviewed associated projections remain selected');
function command(binary, args) {
  const result = spawnSync(binary, args, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, RUSTC_BOOTSTRAP: '1', CARGO_BUILD_JOBS: '1' } });
  assert.equal(result.status, 0, result.stderr); return result.stdout;
}
async function production(mode, selected) {
  const original = fs.readFileSync(path.join(root, 'safe-cex-venue-hyperliquid/src/realm/value.rs'));
  const metadata = JSON.parse(command('cargo', ['metadata', '--locked', '--format-version=1']));
  const pkg = metadata.packages.find(p => p.name === 'safe-cex-venue-hyperliquid');
  const target = pkg.targets.find(t => t.kind.includes('lib'));
  const targetArgs = selected === 'native' ? [] : ['--target', selected];
  command('cargo', ['rustdoc', '--locked', '-p', pkg.name, '--jobs', '1', '--lib', ...targetArgs, '--', '-Z', 'unstable-options', '--output-format', 'json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all']);
  const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, ...(targetArgs.length ? [selected] : []), 'doc', target.name + '.json')));
  const artifacts = command('cargo', ['check', '--locked', '-p', pkg.name, '--jobs', '1', '--lib', ...targetArgs, '--message-format=json']).trim().split('\n').map(line => JSON.parse(line)).filter(item => item.reason === 'compiler-artifact');
  const own = artifacts.filter(item => item.package_id === pkg.id && item.target.name === target.name);
  assert.equal(own.length, 1, 'selected actual host artifact');
  const members = metadata.packages.filter(p => metadata.workspace_members.includes(p.id));
  const sysroot = command('rustc', ['--print', 'sysroot']).trim();
  const macroOrigins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, sysroot);
  // Ordinary inventory runs before selecting the finite cohort: unsupported
  // receiver/trait obligations cannot be removed to manufacture alias evidence.
  const found = inventory(doc, { defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'), readSource: file => fs.readFileSync(path.resolve(root, file)) });
  const aliases = found.publicAliases.filter(alias => Object.hasOwn(specs, alias.name));
  assert.deepEqual(aliases.map(alias => alias.name).sort(), Object.keys(specs).sort());
  const externalTypeAliases = found.externalTypeAliases.map(alias => ({ ...alias, artifact: aliasTargetIdentity(alias, artifacts, members, sysroot) }));
  const scanner = buildScanner('1');
  const claims = JSON.parse(command(scanner, ['--alias-claims-json', root, ...new Set(aliases.map(alias => alias.span.filename))]));
  const contracts = Object.fromEntries(aliases.map(alias => [alias.name, {
    category: 'non-decoder', evidence: 'Focused compiler alias control; actual selected Realm Serde and conversion probes must establish this classification.',
    reason: `${alias.ident}<Realm> is the reviewed native Realm value/error capability. This control claims no callback execution or other host instantiation.`,
    generics: specs[alias.name].generics, rawTarget: specs[alias.name].rawTarget, target: specs[alias.name].target,
  }]));
  for (const alias of aliases) alias.claim = aliasClaim(alias, { packageName: pkg.name, crateRoot: path.dirname(pkg.manifest_path), workspace: root, claims }, contracts[alias.name]);
  if (mode === 'renamed') {
    const url = new URL('./accepted-probes.mjs', import.meta.url).href;
    registerHooks({ load(urlToLoad, context, next) {
      const loaded = next(urlToLoad, context); if (urlToLoad !== url) return loaded;
      const source = String(loaded.source), anchor = '    const command = node ?';
      assert.equal(source.split(anchor).length, 2, 'one post-validation copied-production mutation');
      const injected = `    const mutationFile = path.join(sandbox, workspaceRelative, 'safe-cex-venue-hyperliquid/src/realm/value.rs');
    remember(mutationFile); const before = fs.readFileSync(mutationFile, 'utf8');
    assert.equal(before.split('pub(crate) enum Value {').length, 2);
    fs.writeFileSync(mutationFile, before.replace('pub(crate) enum Value {', 'pub(crate) enum NativeValue {') + '\\npub(crate) type Value = NativeValue;\\n');
`;
      return { ...loaded, source: source.replace(anchor, injected + anchor) };
    } });
  }
  const { runProbes } = await import('./accepted-probes.mjs');
  try {
    const rows = runProbes(root, metadata, [{ package: pkg.name, target, features: own[0].features, types: [], locals: [], publicAliases: aliases, macroOrigins, externalTypeAliases }], contracts, { target: selected, features: [], jobs: 1 });
    console.log('PROJECTION_NATIVE ' + JSON.stringify({ mode, target: selected, rows }));
    assert.equal(rows.length, 6);
    assert.deepEqual(rows.filter(row => row.failures.length), [], 'projection native compiler owner identity');
  } finally { assert.deepEqual(fs.readFileSync(path.join(root, 'safe-cex-venue-hyperliquid/src/realm/value.rs')), original, 'actual production source remains unchanged'); }
}
function child(mode, target) {
  const result = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--projection-native', mode, target], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, RUSTC_BOOTSTRAP: '1' } });
  const output = `${result.stdout}\n${result.stderr}`;
  assert.doesNotMatch(output, /compiler failures are not controls|compile failure is not a control|error\[E\d+\]|could not compile/);
  const receipts = [...output.matchAll(/^PROJECTION_NATIVE (.+)$/gm)].map(match => JSON.parse(match[1]));
  assert.equal(receipts.length, 1, output);
  return { result, output, receipt: receipts[0] };
}
if (process.argv[2] === '--projection-native') await production(process.argv[3], process.argv[4]);
else test('six associated aliases execute ordinary native and wasm traits and reject concrete owner drift', { timeout: 1800000 }, () => {
  let result = child('restored', 'native'); assert.equal(result.result.status, 0, result.output);
  result = child('renamed', 'native'); assert.equal(result.result.status, 1, result.output);
  assert.match(result.output, /projection native compiler owner identity/);
  assert.ok(result.receipt.rows.every(row => row.failures.some(message => message === 'non-decoder alias compiler root identity mismatch')), result.output);
  console.log('projection native concrete owner identity: named assertion; process exit 1');
  result = child('restored', 'native'); assert.equal(result.result.status, 0, result.output);
  result = child('restored', 'wasm32-unknown-unknown'); assert.equal(result.result.status, 0, result.output);
});
