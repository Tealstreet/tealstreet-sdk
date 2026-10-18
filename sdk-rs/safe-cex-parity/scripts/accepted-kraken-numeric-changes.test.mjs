import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import { fileURLToPath } from 'node:url';

const name = 'safe_cex_venue_kraken::stream_updates::NumericChanges';
const root = fileURLToPath(new URL('../..', import.meta.url));
const target = { resolved_path: { path: 'alloc::vec::Vec', args: { angle_bracketed: { args: [{ type: { tuple: [{ borrowed_ref: { lifetime: "'static", is_mutable: false, type: { primitive: 'str' } } }, { primitive: 'f64' }] } }], constraints: [] } } } };
const generics = { params: [], where_predicates: [] };
const alias = { name, ident: 'NumericChanges', span: { filename: 'safe-cex-venue-kraken/src/stream_updates.rs', begin: [11, 1], end: [11, 52] }, generics, rawTarget: target, target };
const context = { packageMetadata: { name: 'safe-cex-venue-kraken', manifest_path: path.join(root, 'safe-cex-venue-kraken/Cargo.toml') }, macroOrigins: ['serde_json'], externalTypeAliases: [{ name, canonical: 'alloc::vec::Vec', kind: 'struct', artifact: { kind: 'sysroot' } }] };

async function pure() {
  const { krakenNumericChangesContract, krakenNumericChangesWitnesses } = await import('./accepted-kraken-numeric-changes.mjs');
  const { aliasContract } = await import('./accepted-aliases.mjs');
  const contract = { category: 'decoder', shape: 'borrowed-sequence', evidence: 'Static key decoder, direct routes and bits', generics, rawTarget: target, target, entrypoints: [], borrowedRoutes: krakenNumericChangesWitnesses() };
  assert.equal(aliasContract(alias, { [name]: contract }, context).aliasProbeExpected, "::std::vec::Vec<(&'static str, f64)>");
  assert.equal(contract.borrowedRoutes.length, 68);
  assert.equal(contract.borrowedRoutes.filter(row => row.output.accepted).length, 14);
  for (const change of [a => a.ident = 'Other', a => a.span.end[1]++, a => a.generics = { params: ['Other'] }, a => a.rawTarget = { primitive: 'str' }, a => a.target = { primitive: 'str' }]) {
    const changed = structuredClone(alias); change(changed);
    assert.throws(() => krakenNumericChangesContract(changed, contract, context), /reviewed borrowed NumericChanges/);
  }
  for (const change of [c => c.category = 'non-decoder', c => c.borrowedRoutes.pop(), c => c.borrowedRoutes[0].output.members.reverse(), c => c.borrowedRoutes[0].output.members[0][1] = '0000000000000000', c => c.observation = { call: 'redirect' }, c => c.entrypoints = [{}]]) {
    const changed = structuredClone(contract); change(changed);
    assert.throws(() => krakenNumericChangesContract(alias, changed, context), /reviewed borrowed NumericChanges/);
  }
  for (const field of ['aliasProbeExpected', 'aliasProbeKind', 'instantiation']) assert.throws(() => aliasContract(alias, { [name]: { ...contract, [field]: 'redirect' } }, context), /cannot be supplied|cannot redirect/);
  assert.throws(() => krakenNumericChangesContract(alias, contract, { ...context, externalTypeAliases: [] }), /authenticated compiler Vec root/);
  assert.throws(() => krakenNumericChangesContract(alias, contract, { ...context, macroOrigins: [] }), /verified serde_json/);
  assert.throws(() => krakenNumericChangesContract(alias, contract, { ...context, packageMetadata: { ...context.packageMetadata, name: 'other' } }), /actual compiler package/);
  assert.throws(() => aliasContract({ ...alias, name: 'other::NumericChanges' }, { 'other::NumericChanges': contract }, context), /borrowed routes require the exact reviewed alias/);
}

async function child(mode) {
  let injected = 0;
  const helper = new URL('./accepted-kraken-numeric-changes.mjs', import.meta.url).href;
  const probes = new URL('./accepted-probes.mjs', import.meta.url).href;
  const hook = registerHooks({ load(url, context, next) {
    const result = next(url, context);
    if (![helper, probes].includes(url)) return result;
    let source = String(result.source);
    if (url === helper && mode === 'identity') {
      const anchor = 'Some(${literal(contract.canonical)})'; assert.equal(source.split(anchor).length, 2);
      source = source.replace(anchor, 'Some("wrong-vec-root")'); injected++;
    }
    if (url === helper && mode === 'members') {
      const anchor = 'decoded.iter().map(|(key, value)|'; assert.equal(source.split(anchor).length, 2);
      source = source.replace(anchor, 'decoded.iter().rev().map(|(key, value)|'); injected++;
    }
    if (url === probes && mode === 'route') {
      const line = source.split('\n').filter(row => row.startsWith("  if (contract.aliasProbeKind === 'reviewed-kraken-numeric-changes') return"));
      assert.equal(line.length, 1);
      source = source.replace(line[0], "  if (contract.aliasProbeKind === 'reviewed-kraken-numeric-changes') return `{ ${equality} use crate::__accepted_set_runtime::DetectAlias as _; let probe = crate::__accepted_set_runtime::AliasProbe::<${targetType}>(std::marker::PhantomData); (&&probe).probe(${literal(type.name)}, ${literal(contract.canonical)}, \"{}\", |_| serde_json::Value::Null, |_, _| None, |_| Vec::new()); }`;"); injected++;
    }
    return { ...result, source };
  } });
  try {
    process.argv = process.argv.slice(0, 2);
    await import('../../safe-cex-venue-kraken/scripts/numeric-changes-contracts.mjs');
  } finally {
    hook.deregister();
    assert.equal(injected, mode === 'baseline' ? 0 : 1);
  }
}
function native(mode) {
  const run = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--numeric-native', mode], { cwd: root, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, env: { ...process.env, CARGO_INCREMENTAL: '0', RUSTC_BOOTSTRAP: '1' } });
  const output = run.stdout + '\n' + run.stderr;
  assert.doesNotMatch(output, /error\[E\d+\]|could not compile|compile failure is not a control/);
  const receipts = output.split('\n').filter(row => row.startsWith('NUMERIC_CHANGES_NATIVE ')).map(row => JSON.parse(row.slice('NUMERIC_CHANGES_NATIVE '.length)));
  assert.equal(receipts.length, 1, output);
  assert.equal(receipts[0].length, 1, output);
  return { ...run, output, receipt: receipts[0][0] };
}
if (process.argv[2] === '--numeric-native') await child(process.argv[3]);
else {
  test('Kraken borrowed NumericChanges pins compiler identity and every route without a general exemption', pure);
  test('Kraken borrowed NumericChanges executes native routes and detects identity member and route drift', { timeout: 1800000 }, () => {
    let run = native('baseline'); assert.equal(run.status, 0, run.output); assert.deepEqual(run.receipt.failures, []); assert.ok(!run.receipt.classificationOnly);
    for (const mode of ['identity', 'members', 'route']) {
      run = native(mode); assert.equal(run.status, 1, run.output);
      assert.match(run.receipt.failures.join(';'), mode === 'route' ? /borrowed alias Deserialize requires an explicit production probe/ : /output mismatch/);
      console.log(`NumericChanges ${mode}: named assertion; process exit 1`);
      run = native('baseline'); assert.equal(run.status, 0, run.output); assert.deepEqual(run.receipt.failures, []);
    }
  });
}
