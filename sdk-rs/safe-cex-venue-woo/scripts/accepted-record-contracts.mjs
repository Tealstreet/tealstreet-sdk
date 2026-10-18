// Focused reviewed record evidence. This is not a whole-package accepted-set gate.
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

const root = fileURLToPath(new URL('../../', import.meta.url));
const names = [
  "safe_cex_venue_woo::types::AccountResponse",
  "safe_cex_venue_woo::types::PlaceOrderResponse",
  "safe_cex_venue_woo::types::PlaceAlgoOrderResponseDataRowsItem",
  "safe_cex_venue_woo::types::PlaceAlgoOrderResponseData",
  "safe_cex_venue_woo::types::PlaceAlgoOrderResponse",
  "safe_cex_venue_woo::types::GetFuturesLeverageResponseDataDetailsItem",
  "safe_cex_venue_woo::types::GetFuturesLeverageResponseData",
  "safe_cex_venue_woo::types::GetFuturesLeverageResponse",
  "safe_cex_venue_woo::types::GetKlineResponseDataRowsItem",
  "safe_cex_venue_woo::types::GetKlineResponseDataMeta",
  "safe_cex_venue_woo::types::GetKlineResponseData",
  "safe_cex_venue_woo::types::GetKlineResponse",
  "safe_cex_venue_woo::v3_contracts::WooV3MigrationRoute",
  "safe_cex_venue_woo::v3_contracts::WooV3PageMeta",
  "safe_cex_venue_woo::v3_contracts::WooV3Instrument",
  "safe_cex_venue_woo::v3_contracts::WooV3Balance",
  "safe_cex_venue_woo::v3_contracts::WooV3Position",
  "safe_cex_venue_woo::v3_contracts::WooV3AccountInfo",
  "safe_cex_venue_woo::v3_contracts::WooV3PlaceOrderRequest",
  "safe_cex_venue_woo::v3_contracts::WooV3PlaceOrderResult",
  "safe_cex_venue_woo::v3_contracts::WooV3ListenKeyRequest",
  "safe_cex_venue_woo::v3_contracts::WooV3ListenKeyResult",
  "safe_cex_venue_woo::v3_public::WooV3FuturesRow",
  "safe_cex_venue_woo::v3_public::WooV3OrderBookLevel",
  "safe_cex_venue_woo::v3_public::WooV3OrderBookSnapshot",
  "safe_cex_venue_woo::v3_public::WooV3KlineRow",
  "safe_cex_venue_woo::v3_public::WooV3FuturesStats"
];
const args = process.argv.slice(2);
assert.ok(args.every(arg => arg === '--control' || /^--target=(native|wasm32-unknown-unknown)$/.test(arg) || arg.startsWith('--out=')), 'unknown focused record argument');
const targetName = args.find(arg => arg.startsWith('--target='))?.slice(9) ?? 'native';
const control = args.includes('--control');
assert.ok(!control || targetName === 'native', 'compiled control is native');
function command(binary, arguments_) {
  const r = spawnSync(binary, arguments_, { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    env: { ...process.env, RUSTC_BOOTSTRAP: '1', CARGO_BUILD_JOBS: '1' } });
  assert.equal(r.status, 0, `${binary}: ${r.stderr}`); return r.stdout;
}
const contracts = loadContracts();
for (const name of names) assert.equal(contracts[name]?.shape, 'map', `${name}: reviewed record contract required`);
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
assert.equal(owners.length, 27, 'complete reviewed record cohort');
assert.ok(owners.every(owner => owner.deserialize && owner.kind === 'record' && !owner.methods.length && !owner.conversions.length && !owner.traitMethods.length), 'no indexed API may be omitted');
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
  globalThis.__wooRecordControl = { owners };
  const url = new URL('../../safe-cex-parity/scripts/accepted-probes.mjs', import.meta.url).href;
  registerHooks({ load(loadedUrl, context, next) {
    const result = next(loadedUrl, context); if (loadedUrl !== url) return result;
    const source = String(result.source), anchor = '    const command = wasm ?';
    assert.equal(source.split(anchor).length, 2, 'one post-validation source mutation');
    const injected = `    for (const owner of globalThis.__wooRecordControl.owners) {
      const file = path.join(sandbox, workspaceRelative, owner.span.filename); remember(file);
      let source = fs.readFileSync(file, 'utf8');
      const marker = "impl<'de> Deserialize<'de> for " + owner.ident + ' {';
      const begin = source.indexOf(marker); assert.ok(begin >= 0, owner.name + ': exact concrete frontdoor required');
      const end = source.indexOf('\\n}', begin) + 2; assert.ok(end > begin);
      const body = source.slice(begin, end);
      const guard = /safe_cex_types::strict_decode::object\\(\\s*decoder\\s*,?\\s*\\)/g;
      assert.equal([...body.matchAll(guard)].length, 1, owner.name + ': exact object guard required');
      source = source.slice(0, begin) + body.replace(guard, 'decoder') + source.slice(end);
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
  console.log(JSON.stringify({ target: targetName, owners: rows.length, failed: failed.map(row => ({ name: row.name, failures: row.failures })) }));
  assert.equal(rows.length, 27);
  if (control) {
    assert.equal(mutations, 1);
    assert.equal(failed.length, 27, 'every independent concrete object guard removal must fail');
    assert.ok(failed.every(row => row.failures.some(message => message.startsWith('native observation mismatch for '))), 'each complete positional array admission must change native field observations');
    console.log('27 concrete owner object guard removals: compiled named native refusals');
  } else assert.deepEqual(failed, [], 'reviewed actual WOO record fields and map-only admission');
} finally { for (const [file, bytes] of originals) assert.deepEqual(fs.readFileSync(path.join(root, file)), bytes, `${file}: real production source remains unchanged`); }
