// Focused reviewed enum evidence. This is not a whole-package accepted-set gate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';
import { buildScanner } from '../../safe-cex-parity/scripts/accepted-scanner.mjs';
import { verifiedMacroOrigins } from '../../safe-cex-parity/scripts/accepted-macro-origins.mjs';
import { verifiedNumberOutputOrigin } from '../../safe-cex-parity/scripts/accepted-number-output.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const names = [
  "safe_cex_venue_woo::v3_contracts::WooV3AccountInfoPositionMode",
  "safe_cex_venue_woo::v3_contracts::WooV3Endpoint",
  "safe_cex_venue_woo::v3_contracts::WooV3HttpMethod",
  "safe_cex_venue_woo::v3_contracts::WooV3ListenKeyRequestType",
  "safe_cex_venue_woo::v3_contracts::WooV3MigrationGate",
  "safe_cex_venue_woo::v3_contracts::WooV3MigrationRouteRuntimeLifecycle",
  "safe_cex_venue_woo::v3_contracts::WooV3PlaceOrderRequestBidAskLevel",
  "safe_cex_venue_woo::v3_contracts::WooV3PlaceOrderRequestMarginMode",
  "safe_cex_venue_woo::v3_contracts::WooV3PlaceOrderRequestPositionSide",
  "safe_cex_venue_woo::v3_contracts::WooV3PlaceOrderRequestSide",
  "safe_cex_venue_woo::v3_contracts::WooV3PlaceOrderRequestType",
  "safe_cex_venue_woo::v3_contracts::WooV3PositionPositionSide",
];
const args = process.argv.slice(2);
assert.ok(args.every(arg => arg === '--control' || /^--target=(native|wasm32-unknown-unknown)$/.test(arg) || arg.startsWith('--out=')), 'unknown focused literal argument');
const targetName = args.find(arg => arg.startsWith('--target='))?.slice(9) ?? 'native';
const control = args.includes('--control');
assert.ok(!control || targetName === 'native', 'compiled control is native');
function command(binary, arguments_) {
  const r = spawnSync(binary, arguments_, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, RUSTC_BOOTSTRAP: '1', CARGO_BUILD_JOBS: '1' } });
  assert.equal(r.status, 0, `${binary}: ${r.stderr}`); return r.stdout;
}
const contracts = loadContracts();
for (const name of names) assert.equal(contracts[name]?.shape, 'string', `${name}: reviewed literal contract required`);
const metadata = JSON.parse(command('cargo', ['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(p => p.name === 'safe-cex-venue-woo');
const target = pkg.targets.find(t => t.kind.includes('lib'));
const targetArgs = targetName === 'native' ? [] : ['--target', targetName];
command('cargo', ['rustdoc', '--locked', '-p', pkg.name, '--jobs', '1', '--lib', ...targetArgs, '--', '-Z', 'unstable-options', '--output-format', 'json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all']);
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, ...(targetArgs.length ? [targetName] : []), 'doc', target.name + '.json')));
const artifacts = command('cargo', ['check', '--locked', '-p', pkg.name, '--jobs', '1', '--lib', ...targetArgs, '--message-format=json']).trim().split('\n').map(line => JSON.parse(line)).filter(item => item.reason === 'compiler-artifact');
const own = artifacts.filter(item => item.package_id === pkg.id && item.target.name === target.name);
assert.equal(own.length, 1, 'actual selected WOO compiler artifact');
const found = inventory(doc, { defaultTraits: loadPolicy('default-traits'), receiverTraits: loadPolicy('receiver-traits'), readSource: file => fs.readFileSync(path.resolve(root, file)) });
const owners = found.types.filter(owner => names.includes(owner.name));
assert.equal(owners.length, 12, 'complete reviewed literal cohort');
assert.ok(owners.every(owner => owner.deserialize && owner.kind === 'enum' && !owner.methods.length && !owner.conversions.length && !owner.traitMethods.length), 'no indexed API may be omitted');
const scanner = buildScanner('1');
const sysroot = command('rustc', ['--print', 'sysroot']).trim();
const origins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, sysroot);
origins.push(...verifiedNumberOutputOrigin(doc, pkg, metadata, artifacts, scanner));
const files = [...new Set(owners.map(owner => owner.span.filename))];
const scope = command(scanner, ['--macro-origins-json', JSON.stringify(origins), ...files.map(file => path.resolve(root, file))]);
console.log('Selected source scope findings: ' + JSON.stringify(scope.trim().split('\n').filter(Boolean)));
assert.equal(scope.trim(), '', 'selected literal source macros/local scopes remain enforced');
console.log('Full ordinary compiler discovery retained: ' + JSON.stringify({ owners: found.types.length, aliases: found.publicAliases.length, construction: found.construction.length }));
const originals = files.map(file => [file, fs.readFileSync(path.join(root, file))]);
let mutations = 0;
if (control) {
  globalThis.__wooLiteralControl = { owners };
  const url = new URL('../../safe-cex-parity/scripts/accepted-probes.mjs', import.meta.url).href;
  registerHooks({ load(loadedUrl, context, next) {
    const result = next(loadedUrl, context); if (loadedUrl !== url) return result;
    const source = String(result.source), anchor = '    const command = wasm ?';
    assert.equal(source.split(anchor).length, 2, 'one post-validation source mutation');
    const injected = `    for (const owner of globalThis.__wooLiteralControl.owners) {
      const file = path.join(sandbox, workspaceRelative, owner.span.filename); remember(file);
      let source = fs.readFileSync(file, 'utf8');
      const header = '#[serde(remote = "' + owner.ident + '")]';
      const remote = source.indexOf(header); assert.ok(remote >= 0, owner.name + ': exact remote attribute required');
      const definition = source.slice(remote + header.length).trimStart(); assert.ok(definition.startsWith('enum '));
      const wire = definition.slice(5).split(' ')[0]; assert.ok(wire, owner.name + ': exact remote companion required');
      const begin = source.indexOf("impl<'de> Deserialize<'de> for " + owner.ident + ' {');
      assert.ok(begin >= 0, owner.name + ': exact concrete frontdoor required');
      const end = source.indexOf('\\n}', begin) + 2; assert.ok(end > begin);
      const restoredGuard = source.slice(begin, end); assert.ok(restoredGuard.includes('String::deserialize'), owner.name + ': string guard required');
      const replacement = "impl<'de> Deserialize<'de> for " + owner.ident + " { fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> { " + wire + "::deserialize(deserializer) } }";
      source = source.slice(0, begin) + replacement + source.slice(end); fs.writeFileSync(file, source);
    }
`;
    mutations++; return { ...result, source: source.replace(anchor, injected + anchor) };
  } });
}
const { runProbes } = await import('../../safe-cex-parity/scripts/accepted-probes.mjs');
try {
  const rows = runProbes(root, metadata, [{ package: pkg.name, target, features: own[0].features, types: owners, locals: [] }], contracts, { target: targetName, features: [], jobs: 1 });
  const output = args.find(arg => arg.startsWith('--out='))?.slice(6);
  if (output) fs.writeFileSync(output, JSON.stringify(rows, null, 2) + '\n');
  const failed = rows.filter(row => row.failures.length);
  console.log(JSON.stringify({ target: targetName, owners: rows.length, failed: failed.map(row => ({ name: row.name, failures: row.failures })) }));
  assert.equal(rows.length, 12);
  if (control) {
    assert.equal(mutations, 1);
    assert.equal(failed.length, 12, 'every independent concrete string guard removal must fail');
    assert.ok(failed.every(row => row.failures.some(message => message.startsWith('Deserialize output mismatch for {'))), 'each actual tagged-map admission must change native variant observations');
    console.log('12 concrete owner string guard removals: compiled named native refusals');
  } else assert.deepEqual(failed, [], 'reviewed actual WOO enum fields and string-only admission');
} finally { for (const [file, bytes] of originals) assert.deepEqual(fs.readFileSync(path.join(root, file)), bytes, `${file}: real production source remains unchanged`); }
