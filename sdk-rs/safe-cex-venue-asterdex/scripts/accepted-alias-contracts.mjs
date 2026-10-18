// Selected alias evidence through ordinary discovery and authenticated artifacts.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { fileURLToPath } from 'node:url';
import { inventory } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { aliasTargetIdentity } from '../../safe-cex-parity/scripts/accepted-aliases.mjs';
import { verifiedMacroOrigins } from '../../safe-cex-parity/scripts/accepted-macro-origins.mjs';
import { loadContracts } from '../../safe-cex-parity/scripts/accepted-contract-files.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

const control = process.argv.length === 3 && process.argv[2] === '--root-control';
assert.ok(process.argv.length === 2 || control, 'only the named native root control is accepted');
// Keep proc-macro2's pinned-toolchain configuration consistent across doc/check/probes.
process.env.RUSTC_BOOTSTRAP = '1';
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const selected = [
  'public_subscriptions::BookCallback', 'public_subscriptions::CandleCallback',
  'public_subscriptions::BookCleanup', 'builder_approval::ApprovalFuture',
  'runtime_host::ResultValue', 'runtime_host::Task', 'runtime_host::Function',
  'native_exchange::AsterDEXHttpFactory',
].map(name => `safe_cex_venue_asterdex::${name}`).sort();
function command(binary, args) {
  const result = spawnSync(binary, args, { cwd: workspace, encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024, env: { ...process.env, CARGO_BUILD_JOBS: '1' } });
  assert.equal(result.status, 0, `${binary} preparation failed; compiler errors are not controls\n${result.error ?? ''}\n${result.stderr}`);
  return result.stdout;
}
const metadata = JSON.parse(command('cargo', ['metadata', '--locked', '--format-version=1']));
const pkg = metadata.packages.find(row => row.name === 'safe-cex-venue-asterdex');
assert.ok(pkg && metadata.workspace_members.includes(pkg.id));
const target = pkg.targets.find(row => row.kind.includes('lib'));
assert.ok(target);
command('cargo', ['rustdoc', '--locked', '-p', pkg.name, '--lib', '--jobs=1', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items',
  '--document-hidden-items', '-A', 'rustdoc::all']);
const doc = JSON.parse(fs.readFileSync(path.join(metadata.target_directory, 'doc', `${target.name}.json`)));
assert.equal(doc.index[String(doc.root)].name, target.name);
const artifacts = command('cargo', ['check', '--locked', '-p', pkg.name, '--lib', '--jobs=1',
  '--message-format=json']).trim().split('\n').map(line => JSON.parse(line))
  .filter(row => row.reason === 'compiler-artifact');
assert.ok(artifacts.some(row => row.package_id === pkg.id));
const sysroot = command('rustc', ['--print', 'sysroot']).trim();
// No onlyOwners shortcut: source/receiver refusals remain failures before alias selection.
const found = inventory(doc, { defaultTraits: loadPolicy('default-traits'),
  receiverTraits: loadPolicy('receiver-traits'),
  readSource: file => fs.readFileSync(path.resolve(workspace, file)) });
const aliases = found.publicAliases.filter(row => selected.includes(row.name));
assert.deepEqual(aliases.map(row => row.name).sort(), selected);
const members = metadata.packages.filter(row => metadata.workspace_members.includes(row.id));
const externalTypeAliases = found.externalTypeAliases.filter(row => selected.includes(row.name))
  .map(row => ({ ...row, artifact: aliasTargetIdentity(row, artifacts, members, sysroot) }));
for (const name of selected) assert.ok(externalTypeAliases.some(row => row.name === name), name);
const macroOrigins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, sysroot);
assert.ok(macroOrigins.includes('serde_json'));
const contracts = loadContracts();
for (const alias of aliases) {
  for (const key of ['generics', 'rawTarget', 'target']) {
    assert.deepEqual(contracts[alias.name]?.[key], alias[key], `${alias.name}: exact compiler ${key}`);
  }
}

// This is the maintained native-pointer test's post-validation emitter control.
// It does not edit production Rust, docs, receiver rows or accepted permissions.
const state = { names: [] };
globalThis.__asterAliasRootControl = state;
const emitter = new URL('../../safe-cex-parity/scripts/accepted-probes.mjs', import.meta.url).href;
const hook = registerHooks({ load(url, context, next) {
  const result = next(url, context);
  if (url !== emitter) return result;
  let source = String(result.source);
  const anchor = '  const alternate = `|value|';
  assert.equal(source.split(anchor).length, 2, 'one actual post-validation alias emitter');
  const capture = `  if (${JSON.stringify(selected)}.includes(type.name)) { globalThis.__asterAliasRootControl.names.push(type.name); ${control ? 'canonical = literal("asterdex-wrong-native-root");' : ''} }\n`;
  if (control) {
    assert.equal(source.split('  const canonical = unsized').length, 2);
    source = source.replace('  const canonical = unsized', '  let canonical = unsized');
  }
  return { ...result, source: source.replace(anchor, capture + anchor) };
} });
try {
  const { runProbes } = await import(emitter);
  const rows = runProbes(workspace, metadata, [{ package: pkg.name, packageId: pkg.id,
    target, types: [], publicAliases: aliases, externalTypeAliases, macroOrigins,
    locals: [], delegatedLocals: [], defaultWitnesses: [] }], contracts,
  { target: 'native', features: [], jobs: 1 });
  fs.writeFileSync(path.join(metadata.target_directory, 'asterdex-alias-results.json'), `${JSON.stringify(rows, null, 2)}\n`);
  console.log(`ASTER_ALIAS_ROWS ${JSON.stringify({ names: state.names, rows })}`);
  assert.deepEqual(state.names.sort(), selected, 'all eight verified aliases execute the native emitter');
  assert.deepEqual(rows.map(row => row.name).sort(), selected);
  for (const row of rows) assert.deepEqual(row.failures, [], row.name);
  const decoders = rows.filter(row => !row.classificationOnly);
  assert.deepEqual(decoders.map(row => row.name), ['safe_cex_venue_asterdex::runtime_host::ResultValue']);
  for (const row of rows.filter(row => row.classificationOnly)) {
    assert.equal(row.category, 'non-decoder-alias', row.name);
    assert.equal(row.deserialize, false, row.name);
  }
  console.log('Aster aliases: one native decoder and seven native non-decoder alias receipts; eight authenticated aliases, zero failures.');
  console.log('Scope is the original eight native/default aliases; other owners, locals, features and wasm remain separate.');
} finally {
  hook.deregister();
  delete globalThis.__asterAliasRootControl;
}
