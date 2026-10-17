// Focused reviewed validator evidence. This is not a whole-package accepted-set gate.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
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

// Keep rustdoc, selected checks and native/wasm probes in one compiler environment.
process.env.RUSTC_BOOTSTRAP = '1';
const root = fileURLToPath(new URL('../../', import.meta.url));
const names = [
  "safe_cex_venue_woo::v3_public::require_record",
  "safe_cex_venue_woo::v3_public::require_array",
  "safe_cex_venue_woo::v3_public::require_string",
  "safe_cex_venue_woo::v3_public::optional_string",
  "safe_cex_venue_woo::v3_public::require_integer",
  "safe_cex_venue_woo::v3_public::require_timestamp",
  "safe_cex_venue_woo::v3_public::require_boolean",
  "safe_cex_venue_woo::v3_public::require_decimal_string",
  "safe_cex_venue_woo::v3_public::require_instrument_decimal",
  "safe_cex_venue_woo::v3_public::optional_instrument_decimal",
  "safe_cex_venue_woo::v3_public::require_finite_number",
  "safe_cex_venue_woo::v3_private::require_record",
  "safe_cex_venue_woo::v3_private::require_array",
  "safe_cex_venue_woo::v3_private::require_string",
  "safe_cex_venue_woo::v3_private::require_boolean",
  "safe_cex_venue_woo::v3_private::require_integer",
  "safe_cex_venue_woo::v3_private::require_finite_number",
  "safe_cex_venue_woo::v3_private::require_decimal_string",
  "safe_cex_venue_woo::v3_private::optional_decimal_string",
  "safe_cex_venue_woo::v3_private::require_enum",
  "safe_cex_venue_woo::v3_private::decimal_to_number",
  "safe_cex_venue_woo::v3_regular_writes::require_record",
  "safe_cex_venue_woo::v3_regular_writes::require_boolean",
  "safe_cex_venue_woo::v3_regular_writes::require_integer",
  "safe_cex_venue_woo::v3_regular_writes::require_string",
  "safe_cex_venue_woo::v3_regular_writes::require_enum",
  "safe_cex_venue_woo::v3_regular_writes::require_decimal",
  "safe_cex_venue_woo::v3_regular_writes::require_perpetual_symbol",
  "safe_cex_venue_woo::v3_public::required_stats_string",
  "safe_cex_venue_woo::types::deserialize_present"
];
const args = process.argv.slice(2);
assert.ok(args.every(arg => arg === '--control' || /^--target=(native|wasm32-unknown-unknown)$/.test(arg) || arg.startsWith('--out=')), 'unknown focused validator argument');
const targetName = args.find(arg => arg.startsWith('--target='))?.slice(9) ?? 'native';
const control = args.includes('--control');
assert.ok(!control || targetName === 'native', 'compiled control is native');
function command(binary, arguments_) {
  const r = spawnSync(binary, arguments_, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, RUSTC_BOOTSTRAP: '1', CARGO_BUILD_JOBS: '1' } });
  assert.equal(r.status, 0, `${binary}: ${r.stderr}`); return r.stdout;
}
const contracts = loadContracts();
for (const name of names) assert.equal(contracts[name]?.category, 'decoder', `${name}: reviewed validator contract required`);
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
assert.equal(owners.length, 30, 'complete reviewed validator cohort');
assert.ok(owners.every(owner => !owner.deserialize && owner.kind === 'function' && owner.methods.length === 1 && !owner.conversions.length && !owner.traitMethods.length), 'no indexed API may be omitted');
const scanner = buildScanner('1');
const sysroot = command('rustc', ['--print', 'sysroot']).trim();
const origins = verifiedMacroOrigins(doc, pkg, metadata, artifacts, sysroot);
origins.push(...verifiedNumberOutputOrigin(doc, pkg, metadata, artifacts, scanner));
const files = [...new Set(owners.map(owner => owner.span.filename))];
const scope = command(scanner, ['--macro-origins-json', JSON.stringify(origins), ...files.map(file => path.resolve(root, file))]);
console.log('Selected source scope findings: ' + JSON.stringify(scope.trim().split('\n').filter(Boolean)));
const localFunctions = loadPolicy('local-functions');
for (const event of scope.trim().split('\n').filter(Boolean)) {
  const [file, ident] = event.split('\t');
  const key = path.relative(root, file) + '::' + ident;
  assert.ok(ident.startsWith('LOCAL_FUNCTION:'), key + ': unsupported source scope remains refused');
  const reviewed = localFunctions[key];
  assert.ok(reviewed?.category === 'non-decoder' && reviewed.reason?.trim(), key + ': exact reviewed non-decoder local role required');
  assert.equal(reviewed.sourceSha256, createHash('sha256').update(fs.readFileSync(file)).digest('hex'), key + ': reviewed nested-function source changed');
}
console.log('Full ordinary compiler discovery retained: ' + JSON.stringify({ owners: found.types.length, aliases: found.publicAliases.length, construction: found.construction.length }));
const originals = files.map(file => [file, fs.readFileSync(path.join(root, file))]);
let mutations = 0;
if (control) {
  const items = new Map(owners.map(owner => {
    const source = fs.readFileSync(path.join(root, owner.span.filename), 'utf8'), lines = source.split('\n');
    const offset = ([line, column]) => lines.slice(0, line - 1).reduce((n, s) => n + s.length + 1, 0) + Array.from(lines[line - 1]).slice(0, column - 1).join('').length;
    return [owner.name, source.slice(offset(owner.span.begin), offset(owner.span.end))];
  }));
  globalThis.__wooValidatorControl = { owners, items };
  const url = new URL('../../safe-cex-parity/scripts/accepted-probes.mjs', import.meta.url).href;
  registerHooks({ load(loadedUrl, context, next) {
    const result = next(loadedUrl, context); if (loadedUrl !== url) return result;
    const source = String(result.source), anchor = '    const command = wasm ?';
    assert.equal(source.split(anchor).length, 2, 'one post-validation source mutation');
    const injected = `    const changes = new Map();
    for (const owner of globalThis.__wooValidatorControl.owners) {
      const file = path.join(sandbox, workspaceRelative, owner.span.filename); remember(file);
      if (!changes.has(file)) changes.set(file, []);
      changes.get(file).push(owner);
    }
    for (const [file, owners] of changes) {
      let source = fs.readFileSync(file, 'utf8');
      for (const owner of owners) {
        const item = globalThis.__wooValidatorControl.items.get(owner.name), body = item.indexOf('{');
        assert.ok(body > 0 && item.includes('fn ' + owner.ident), owner.name + ': exact compiler function item');
        assert.equal(source.split(item).length, 2, owner.name + ': one unchanged production function after instrumentation');
        const begin = source.indexOf(item), end = begin + item.length;
        const error = ['required_stats_string','deserialize_present'].includes(owner.ident) ? '<D::Error as serde::de::Error>::custom("validator source mutation")' : 'String::from("validator source mutation")';
        const replacement = item.slice(0, body + 1) + ' if std::hint::black_box(true) { return Err(' + error + '); } ' + item.slice(body + 1);
        source = source.slice(0, begin) + replacement + source.slice(end);
      }
      fs.writeFileSync(file, source);
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
  console.log(JSON.stringify({ target: targetName, owners: rows.length, failed: failed.map(row => ({ name: row.name, failures: control ? row.failures.length : row.failures })) }));
  assert.equal(rows.length, 30);
  if (control) {
    assert.equal(mutations, 1);
    assert.equal(failed.length, 30, 'every independent native validator body removal must fail');
    assert.ok(failed.every(row => row.failures.some(message => message.startsWith('function output mismatch for '))), 'each actual native validator must distinguish its reviewed valid inputs and outputs');
    console.log('30 concrete native validator body removals: compiled named observation failures');
  } else assert.deepEqual(failed, [], 'reviewed actual WOO validator outputs and admission');
} finally { for (const [file, bytes] of originals) assert.deepEqual(fs.readFileSync(path.join(root, file)), bytes, `${file}: real production source remains unchanged`); }
