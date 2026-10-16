import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createHash } from 'node:crypto';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { aliasClaim, aliasContract, aliasTargetIdentity } from './accepted-aliases.mjs';

const workspace = path.resolve('sdk-rs');
const context = { packageName: 'consumer', crateRoot: path.join(workspace, 'consumer'), workspace, claims: { declarations: { 'consumer/src/lib.rs': [['Side', [1, 24]], ['nested::Side', [2, 42]], ['unmapped::Side', [3, 42]]] }, mapped: ['consumer/src/lib.rs::Side'], exempt: { 'consumer/src/lib.rs::nested::Side': 'Explicit control alias exemption.' } } };
const alias = { name: 'consumer::Side', span: { filename: 'consumer/src/lib.rs', end: [1, 24] } };
test('public alias claims require exact mapped declarations or explicit exemptions', () => {
  assert.deepEqual(aliasClaim(alias, context), { rs: 'consumer/src/lib.rs::Side' });
  assert.equal(aliasClaim({ ...alias, name: 'consumer::nested::Side', span: { ...alias.span, end: [2, 42] } }, context).rs, 'consumer/src/lib.rs::nested::Side');
  assert.throws(() => aliasClaim(alias, { ...context, claims: { ...context.claims, mapped: [] } }), /public alias needs a mapped row or exemption/);
  assert.throws(() => aliasClaim({ ...alias, span: { filename: 'other/src/lib.rs' } }, context), /alias source must belong to its compiler package/);
  assert.throws(() => aliasClaim({ ...alias, name: 'consumer::unmapped::Side', span: { ...alias.span, end: [3, 42] } }, context), /public alias needs a mapped row or exemption/);
  assert.throws(() => aliasClaim({ ...alias, span: { ...alias.span, end: [99, 42] } }, context), /unique public declaration at its compiler source span/);
});
test('native tooling alias exceptions cannot replace SDK mappings', () => {
  const native = { ...context, nativeTooling: true, claims: { declarations: context.claims.declarations, mapped: [], exempt: {} } };
  const exemption = { rs: 'consumer/src/lib.rs::Side', exemption: 'Native-only tool control alias.' };
  assert.equal(aliasClaim(alias, native, exemption).rs, exemption.rs);
  assert.throws(() => aliasClaim(alias, { ...native, nativeTooling: false }, exemption), /public alias needs a mapped row or exemption/);
  assert.throws(() => aliasClaim(alias, native, { ...exemption, rs: 'consumer/src/wrong.rs::Side' }), /native tooling alias needs an exact exemption/);
  assert.throws(() => aliasClaim({ ...alias, name: 'consumer::unmapped::Side', span: { ...alias.span, end: [3, 42] } }, native, exemption), /native tooling alias needs an exact exemption/);
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'native-tooling-alias-'));
  try {
    const packageRoot = path.join(directory, 'safe-cex-parity');
    fs.mkdirSync(path.join(packageRoot, 'src'), { recursive: true });
    const manifest = path.join(packageRoot, 'Cargo.toml');
    const source = '\n'.repeat(19) + 'pub type Result<T> = std::result::Result<T, String>;\n';
    fs.writeFileSync(manifest, 'compiler metadata fixture');
    fs.writeFileSync(path.join(packageRoot, 'src/lib.rs'), source);
    const item = {"name":"safe_cex_parity::Result","ident":"Result","span":{"filename":"safe-cex-parity/src/lib.rs","begin":[20,1],"end":[20,53]},"generics":{"params":[{"name":"T","kind":{"type":{"bounds":[],"default":null,"is_synthetic":false}}}],"where_predicates":[]},"rawTarget":{"resolved_path":{"path":"core::result::Result","args":{"angle_bracketed":{"args":[{"type":{"generic":"T"}},{"type":{"resolved_path":{"path":"alloc::string::String","args":null}}}],"constraints":[]}}}},"target":{"resolved_path":{"path":"core::result::Result","args":{"angle_bracketed":{"args":[{"type":{"generic":"T"}},{"type":{"resolved_path":{"path":"alloc::string::String","args":null}}}],"constraints":[]}}}}};
    const record = { category: 'tooling-exemption', evidence: 'Exact tooling source fixture.', exemption: 'Tooling scope only; no generic decoder claim.', file: item.span.filename, declarationEnd: item.span.end, rs: 'safe-cex-parity/src/lib.rs::Result', sourceSha256: createHash('sha256').update(source).digest('hex'), rawTarget: item.rawTarget, target: item.target, generics: item.generics };
    const pkg = { name: 'safe-cex-parity', id: 'actual-fixture-tooling-package', manifest_path: manifest };
    const claimContext = { packageName: pkg.name, crateRoot: packageRoot, workspace: directory, claims: { declarations: { [record.file]: [['Result', record.declarationEnd]] }, mapped: [], exempt: {} }, nativeTooling: true };
    item.claim = aliasClaim(item, claimContext, record);
    const scope = { packageMetadata: pkg, metadata: { packages: [pkg], workspace_members: [pkg.id] }, workspace: directory, target: 'native', nativePolicy: { nativeOnly: [{ package: pkg.name, reason: 'Build-time tooling.' }], wasm: [] } };
    const check = (entry = item, rule = record, context = scope) => aliasContract(entry, { [entry.name]: rule }, context);
    assert.equal(check(), record, 'exact generic tooling declaration yields only a scope classification');
    for (const change of [
      value => { value.rawTarget = { primitive: 'bool' }; },
      value => { value.target = { primitive: 'bool' }; },
      value => { value.generics = { params: [], where_predicates: [] }; },
      value => { value.span.filename = 'other/src/lib.rs'; },
      value => { value.span.end = [20, 54]; },
      value => { value.claim = { rs: record.rs }; },
    ]) { const changed = structuredClone(item); change(changed); assert.throws(() => check(changed), /native tooling alias exemption/); }
    assert.throws(() => check(item, { ...record, sourceSha256: 'stale' }), /reviewed source changed/);
    assert.throws(() => check(item, { ...record, instantiation: 'serde_json::Value' }), /unsupported exemption fields/);
    assert.throws(() => check(item, { ...record, nativeObservation: { cases: [] } }), /unsupported exemption fields/);
    assert.throws(() => check(item, record, { ...scope, packageMetadata: { ...pkg, name: 'safe-cex-types' } }), /SDK package cannot use tooling scope/);
    assert.throws(() => check(item, record, { ...scope, packageMetadata: { ...pkg, id: 'foreign' } }), /unique compiler package/);
    assert.throws(() => check(item, record, { ...scope, metadata: { ...scope.metadata, workspace_members: [] } }), /must belong to workspace/);
    assert.throws(() => check(item, record, { ...scope, nativePolicy: { nativeOnly: [], wasm: [] } }), /native-only policy required/);
    assert.throws(() => check(item, record, { ...scope, nativePolicy: { ...scope.nativePolicy, wasm: [{ package: pkg.name }] } }), /SDK wasm scope cannot be exempted/);
    assert.throws(() => check(item, record, { ...scope, target: 'wasm32-unknown-unknown' }), /native execution scope required/);
    assert.throws(() => check({ ...item, claim: aliasClaim(item, { ...claimContext, claims: { ...claimContext.claims, mapped: [record.rs] } }, record) }), /verified declaration exemption required/);
    assert.throws(() => aliasContract(item, { [item.name]: record }), /actual compiler package context required/);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
function fixture(run) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'accepted-alias-identity-'));
  try {
    const sysroot = path.join(root, 'sysroot');
    const target = path.join(sysroot, 'lib/rustlib/target/lib/liballoc.rmeta');
    const dependency = path.join(root, 'libdependency.rmeta');
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, 'compiler fixture');
    fs.writeFileSync(dependency, 'dependency fixture');
    run({ root, sysroot, target, dependency });
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}
test('public alias targets retain Cargo package identity despite crate spelling', () => fixture(({ sysroot, dependency }) => {
  const target = { name: alias.name, canonical: 'serde_json::ValueLike', compilerPath: dependency };
  const artifact = { package_id: 'workspace-vendor', filenames: [dependency], features: ['strict'] };
  assert.equal(aliasTargetIdentity(target, [artifact], [{ id: 'workspace-vendor' }], sysroot).kind, 'workspace');
  assert.equal(aliasTargetIdentity(target, [artifact], [], sysroot).kind, 'registry');
}));
test('sysroot alias identity follows actual compiler paths rather than names or symlinks', () => fixture(({ root, sysroot, target, dependency }) => {
  assert.equal(aliasTargetIdentity({ name: alias.name, canonical: 'alloc::string::String', compilerPath: target }, [], [], sysroot).kind, 'sysroot');
  const link = path.join(root, 'sysroot/lib/rustlib/target/lib/libfake.rmeta');
  fs.symlinkSync(dependency, link);
  const artifact = { package_id: 'workspace-std-spoof', filenames: [dependency], features: [] };
  assert.equal(aliasTargetIdentity({ name: alias.name, canonical: 'std::Fake', compilerPath: link }, [artifact], [{ id: artifact.package_id }], sysroot).kind, 'workspace');
}));
test('missing or ambiguous compiler alias artifacts fail closed', () => fixture(({ sysroot, dependency }) => {
  const target = { name: alias.name, canonical: 'dependency::Side', compilerPath: dependency };
  assert.throws(() => aliasTargetIdentity(target, [], [], sysroot), /unique Cargo compiler-artifact identity/);
  const artifact = { package_id: 'dependency', filenames: [dependency], features: [] };
  assert.throws(() => aliasTargetIdentity(target, [artifact, artifact], [], sysroot), /unique Cargo compiler-artifact identity/);
}));

test('alias contracts retain actual types, compiler roots and direct native witnesses', () => {
  const item = { ...alias, generics: { params: [] }, target: { resolved_path: { path: 'owner::Side' } } };
  const contract = { target: item.target, category: 'decoder', evidence: 'Control closed side domain.', shape: 'string', literals: ['buy', 'sell'], observation: { call: 'serde_json::json!(format!("{:?}", serde_json::from_value::<$type>($value.clone()).unwrap()))', cases: [{ input: 'buy', output: 'Buy' }, { input: 'sell', output: 'Sell' }] } };
  const check = value => aliasContract(item, { [item.name]: value });
  assert.equal(check(contract).canonical, 'owner::Side');
  assert.throws(() => check({ ...contract, nativeObservation: { call: '$type($value)', cases: [] } }), /aliases use their own direct native observation mechanism/);
  assert.throws(() => check({ ...contract, category: 'non-decoder', reason: 'Already typed alias.', observation: undefined, nativeObservation: { call: '$type($value)', cases: [] } }), /aliases use their own direct native observation mechanism/);

  assert.throws(() => check({ ...contract, instantiation: 'Unrelated' }), /cannot redirect its production type/);
  assert.throws(() => check({ ...contract, observation: { ...contract.observation, call: 'serde_json::to_value(serde_json::from_value::<$type>($value.clone()).unwrap()).unwrap()' } }), /must assert decoded native values/);
  assert.throws(() => aliasContract({ ...item, generics: { params: [{ name: 'T' }] } }, { [item.name]: contract }), /generic alias requires explicit compiler monomorphization coverage/);
  assert.throws(() => aliasContract({ ...item, target: { resolved_path: { path: 'owner::Side', args: { angle_bracketed: { args: [{ type: { primitive: 'bool' } }] } } } } }, { [item.name]: contract }), /alias compiler target changed/);
  assert.throws(() => check({ ...contract, observation: undefined }), /requires direct decoded-value observations/);
  assert.throws(() => check({ ...contract, entrypoints: [{ trait: 'FromStr', call: 'false', positive: ['buy'], negative: ['BUY'], reason: 'Control inert adapter.' }] }), /must use an executed trait path/);
  assert.throws(() => check({ ...contract, entrypoints: [{ trait: 'Unsupported', nativeCases: [], positive: ['buy'], negative: ['BUY'], reason: 'Control unsupported trait.' }] }), /needs explicit positive\/negative witnesses/);
});

test('alias native cases cover positive conversions without redecoding their input', () => {
  const item = { ...alias, generics: { params: [] }, target: { resolved_path: { path: 'owner::Side' } } };
  const entry = { trait: 'FromStr', reason: 'Control actual string decoder.', positive: ['buy', 'sell'], negative: ['BUY'], nativeCases: [{ input: 'buy', output: 'Buy' }, { input: 'sell', output: 'Sell' }] };
  const contract = { target: item.target, category: 'decoder', evidence: 'Control closed side domain.', shape: 'string', literals: ['buy', 'sell'], nativeProjection: 'serde_json::json!(match $decoded { Side::Buy => "Buy", Side::Sell => "Sell" })', entrypoints: [entry], observation: { call: 'match serde_json::from_value::<$type>($value.clone()).unwrap() { Side::Buy => serde_json::json!("Buy"), Side::Sell => serde_json::json!("Sell") }', cases: [{ input: 'buy', output: 'Buy' }, { input: 'sell', output: 'Sell' }] } };
  const check = value => aliasContract(item, { [item.name]: value });
  assert.equal(check(contract).canonical, 'owner::Side');
  assert.throws(() => check({ ...contract, nativeProjection: 'serde_json::from_value::<Side>($decoded.clone()).unwrap()' }), /must inspect the actual decoded value/);
  assert.throws(() => check({ ...contract, entrypoints: [{ ...entry, nativeCases: [{ input: 'buy', output: 'Buy' }] }] }), /needs direct native output witnesses/);
  assert.throws(() => check({ ...contract, entrypoints: [{ ...entry, nativeCases: [{ input: 'buy', output: 'Buy' }, { input: 'BUY', output: 'Buy' }] }] }), /must cover every declared positive input/);
  assert.throws(() => check({ ...contract, nativeProjection: 'serde_json::to_value($decoded).unwrap()' }), /must inspect the actual decoded value/);
});
