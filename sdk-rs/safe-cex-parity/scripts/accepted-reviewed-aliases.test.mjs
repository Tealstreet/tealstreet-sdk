import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import test from 'node:test';
import { reviewedGenericAliasProbe } from './accepted-reviewed-aliases.mjs';
import { genericAliasProbe } from './accepted-callable-alias.mjs';
import { aliasContract } from './accepted-aliases.mjs';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const pointerSpecs = loadPolicy('function-pointer-aliases');
function pointerFixture(name) {
  const spec = pointerSpecs[name];
  const workspace = fileURLToPath(new URL('../..', import.meta.url));
  const alias = { name, ident: name.split('::').at(-1), span: { filename: spec.file }, generics: spec.generics, rawTarget: spec.rawTarget, target: spec.target };
  const contract = { category: 'non-decoder', evidence: 'Synthetic compiler-role control; native execution has its own control.', reason: 'Typed callback capability.', generics: spec.generics, rawTarget: spec.rawTarget, target: spec.target };
  const context = { workspace, macroOrigins: ['serde_json'], externalTypeAliases: spec.roots.map(root => ({
    name, canonical: root.canonical, kind: root.kind, artifact: {
      kind: root.artifactKind, ...(root.package ? { packageId: root.package, owner: { name: root.package, id: root.package, manifest_path: path.join(workspace, root.package, 'Cargo.toml') } } : {}),
    },
  })) };
  return { alias, contract, context };
}

test('reviewed function pointers retain exact signatures and every nested compiler role', () => {
  assert.deepEqual(Object.keys(pointerSpecs).sort(), ['safe_cex_venue_bitunix::api::RetryCondition', 'safe_cex_venue_blofin::exchange::BlofinApiFactory']);
  for (const name of Object.keys(pointerSpecs)) {
    const { alias, contract, context } = pointerFixture(name);
    const probe = genericAliasProbe(alias, contract, context);
    assert.equal(probe.expectedType, pointerSpecs[name].expectedType);
    assert.equal(probe.suffix, '');
    assert.equal(probe.probeKind, 'reviewed-function-pointer');
    assert.equal(genericAliasProbe({ ...alias, name: 'unreviewed::Pointer' }, contract, context), undefined);
    assert.throws(() => aliasContract({ ...alias, name: 'unreviewed::Pointer' }, { 'unreviewed::Pointer': contract }, context), /alias target requires explicit compiler root identity coverage/);
    for (const mutate of [
      item => { item.span.filename = 'other.rs'; },
      item => { item.ident = 'Other'; },
      item => { item.generics.params.push({ name: 'T' }); },
      item => { item.rawTarget.function_pointer.header.is_unsafe = true; },
      item => { item.target.function_pointer.sig.inputs.reverse(); item.target.function_pointer.sig.is_c_variadic = true; },
    ]) {
      const item = structuredClone(alias); mutate(item);
      assert.throws(() => genericAliasProbe(item, contract, context), /reviewed function pointer/);
    }
    for (const key of ['instantiation', 'aliasProbeExpected', 'aliasProbeKind', 'aliasProbeSuffix', 'nativeCanonical']) assert.throws(() => genericAliasProbe(alias, { ...contract, [key]: 'redirect' }, context), /caller cannot supply/);
    assert.throws(() => genericAliasProbe(alias, { ...contract, category: 'decoder' }, context), /capability classification/);
    assert.throws(() => genericAliasProbe(alias, { ...contract, target: {} }, context), /contract compiler target changed/);
    assert.throws(() => genericAliasProbe(alias, contract, { ...context, macroOrigins: [] }), /actual compiler context/);
    for (let i = 0; i < context.externalTypeAliases.length; i++) {
      const missing = structuredClone(context); missing.externalTypeAliases.splice(i, 1);
      assert.throws(() => genericAliasProbe(alias, contract, missing), /nested compiler root identity/);
      const foreign = structuredClone(context); foreign.externalTypeAliases[i].artifact.kind = 'registry';
      assert.throws(() => genericAliasProbe(alias, contract, foreign), /nested compiler root identity/);
      const wrongKind = structuredClone(context); wrongKind.externalTypeAliases[i].kind = 'function';
      assert.throws(() => genericAliasProbe(alias, contract, wrongKind), /nested compiler root identity/);
    }
    if (name.includes('blofin')) {
      const foreign = structuredClone(context);
      foreign.externalTypeAliases[0].artifact.owner.manifest_path = path.join(context.workspace, 'safe-cex-num/Cargo.toml');
      assert.throws(() => genericAliasProbe(alias, contract, foreign), /selected workspace source changed/);
    } else {
      const lifetimes = structuredClone(alias);
      lifetimes.target.function_pointer.sig.inputs[0][1].borrowed_ref.type.dyn_trait.lifetime = "'static";
      assert.throws(() => genericAliasProbe(lifetimes, { ...contract, target: lifetimes.target }, context), /compiler target changed/);
    }
  }
});

test('reviewed function pointer source signature roots and caller guards are load bearing', () => {
  const { alias, contract, context } = pointerFixture('safe_cex_venue_bitunix::api::RetryCondition');
  const url = new URL('./accepted-function-pointer-alias.mjs', import.meta.url).href;
  const runner = new URL('./accepted-callable-alias.mjs', import.meta.url).href;
  const cases = [
    ['function pointer source identity', '  assert.equal(alias.span?.filename, spec.file, `${label}: compiler source identity changed`);', item => { item.alias.span.filename = 'other.rs'; }, /compiler source identity changed/],
    ['function pointer expanded signature', '    assert.deepEqual(alias[key], spec[key], `${label}: compiler ${key} changed`);', item => { item.alias.target.function_pointer.header.is_unsafe = true; item.contract.target = item.alias.target; }, /compiler target changed/],
    ['function pointer nested root identity', '  assert.deepEqual(roots, spec.roots, `${label}: every nested compiler root identity required`);', item => { item.context.externalTypeAliases.pop(); }, /nested compiler root identity/],
    ['function pointer caller redirection', '  for (const key of callerFields) assert.equal(contract[key], undefined, `${label}: caller cannot supply ${key}`);', item => { item.contract.aliasProbeExpected = 'bool'; }, /caller cannot supply/],
  ];
  for (const [label, anchor, change, refusal] of cases) {
    const item = structuredClone({ alias, contract, context }); change(item);
    const execute = mutant => {
      const hook = mutant ? `import {registerHooks} from 'node:module'; registerHooks({load(url,context,next){const r=next(url,context);if(url!==${JSON.stringify(url)})return r;const source=String(r.source),anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);return {...r,source:source.replace(anchor,'')};}});` : '';
      return spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';${hook}const {genericAliasProbe}=await import(${JSON.stringify(runner)});assert.throws(()=>genericAliasProbe(${JSON.stringify(item.alias)},${JSON.stringify(item.contract)},${JSON.stringify(item.context)}),${refusal},${JSON.stringify(label + ' must refuse')});`], { encoding: 'utf8' });
    };
    assert.equal(execute(false).status, 0);
    const mutant = execute(true); assert.equal(mutant.status, 1, mutant.stderr);
    assert.match(mutant.stderr, new RegExp(label + ' must refuse'));
    console.log(`${label}: named assertion; process exit 1`);
    assert.equal(execute(false).status, 0);
  }
});

const future = { resolved_path: { path: 'core::pin::Pin', args: { angle_bracketed: { args: [{ type: { generic: 'T' } }], constraints: [] } } } };
const alias = {
  name: 'control::Pending',
  span: { filename: 'control/src/lib.rs' },
  generics: { params: [{ name: "'a", kind: { lifetime: { outlives: [] } } }, { name: 'T', kind: { type: { bounds: [], default: null, is_synthetic: false } } }], where_predicates: [] },
  rawTarget: future,
  target: future,
};
const spec = {
  canonical: 'core::pin::Pin', expectedType: '::core::pin::Pin<::serde_json::Value>', file: 'control/src/lib.rs',
  generics: alias.generics, rawTarget: future, suffix: "<'static, ::serde_json::Value>", target: future, unsized: false,
};
const context = { macroOrigins: ['serde_json'], externalTypeAliases: [{ name: 'control::Pending', canonical: 'core::pin::Pin', artifact: { kind: 'sysroot' } }] };

test('reviewed generic aliases return only their spec-owned fixed instantiation', () => {
  const specs = { [alias.name]: spec };
  assert.deepEqual(reviewedGenericAliasProbe(alias, undefined, context, specs), {
    suffix: "<'static, ::serde_json::Value>", canonical: 'core::pin::Pin', expectedType: '::core::pin::Pin<::serde_json::Value>', probeKind: 'reviewed-fixed',
    monomorphization: "one reviewed control::Pending<'static, ::serde_json::Value> instantiation; no claim for other generic arguments",
  });
  assert.equal(reviewedGenericAliasProbe({ ...alias, name: 'control::Unknown' }, undefined, context, specs), undefined, 'unknown names keep the original fail-closed families');
  assert.throws(() => genericAliasProbe({ ...alias, name: 'control::Unknown' }, undefined, context), /accepted-set assertion: (?!reviewed generic alias)/, 'unreviewed shapes still reach an original family refusal');
  for (const [mutate, message] of [
    [row => { row.target = { generic: 'T' }; }, /compiler target changed/],
    [row => { row.generics = { ...row.generics, where_predicates: [{}] }; }, /compiler generics changed/],
    [row => { row.span = { filename: 'control/src/other.rs' }; }, /compiler source identity changed/],
  ]) {
    const changed = structuredClone(alias); mutate(changed);
    assert.throws(() => reviewedGenericAliasProbe(changed, undefined, context, specs), message);
  }
  assert.throws(() => reviewedGenericAliasProbe(alias, { aliasProbeExpected: 'Other' }, context, specs), /caller cannot supply aliasProbeExpected/);
  assert.throws(() => reviewedGenericAliasProbe(alias, undefined, { ...context, externalTypeAliases: [] }, specs), /canonical root needs actual compiler identity/);
  assert.throws(() => reviewedGenericAliasProbe(alias, undefined, { ...context, macroOrigins: [] }, specs), /serde_json compiler dependency/);
  assert.throws(() => reviewedGenericAliasProbe(alias, undefined, context, { [alias.name]: { ...spec, canonical: 'core::option::Option' } }), /canonical root must be the compiler target root/);
  assert.throws(() => reviewedGenericAliasProbe(alias, undefined, context, { [alias.name]: { ...spec, unsized: true } }), /only rootless dyn targets are unsized/);
  assert.throws(() => reviewedGenericAliasProbe(alias, undefined, context, { [alias.name]: { ...spec, extra: 1 } }), /unsupported spec fields/);

  const canonical = 'indexmap::map::IndexMap';
  const target = { resolved_path: { path: canonical, args: { angle_bracketed: {
    args: [{ type: { resolved_path: { path: 'alloc::string::String', args: null } } },
      { type: { generic: 'T' } }], constraints: [],
  } } } };
  const registered = { ...alias, name: 'control::RegisteredMap',
    generics: { params: [structuredClone(alias.generics.params[1])], where_predicates: [] },
    rawTarget: target, target };
  const selected = { ...spec, canonical, generics: registered.generics, suffix: '<::serde_json::Value>', rawTarget: target, target,
    expectedType: '::indexmap::IndexMap<::std::string::String, ::serde_json::Value>' };
  const registrySpecs = { [registered.name]: selected };
  const compilerPath = '/compiled/libindexmap.rmeta';
  const registryContext = { macroOrigins: ['serde_json'],
    packageMetadata: { dependencies: [{ name: 'indexmap', rename: null, kind: null, req: '=2.14.2', source: 'registry+https://github.com/rust-lang/crates.io-index' }] },
    externalTypeAliases: [{ name: registered.name, canonical, kind: 'struct', compilerPath,
      artifact: { kind: 'registry', compilerPath,
        packageId: 'registry+https://github.com/rust-lang/crates.io-index#indexmap@2.14.2' } }],
  };
  assert.equal(reviewedGenericAliasProbe(registered, undefined, registryContext, registrySpecs).expectedType, selected.expectedType);
  for (const mutate of [
    item => { item.packageMetadata.dependencies = []; },
    item => { item.packageMetadata.dependencies[0].req = '^2.14'; },
    item => { item.packageMetadata.dependencies[0].source = 'registry+https://foreign.test'; },
    item => { item.externalTypeAliases[0].artifact.packageId = 'registry+https://example.test#indexmap@2.14.1'; },
    item => { item.externalTypeAliases[0].artifact.packageId = 'registry+https://example.test#other@2.14.2'; },
    item => { item.externalTypeAliases[0].artifact.compilerPath = '/other/libindexmap.rmeta'; },
    item => { item.externalTypeAliases[0].kind = 'trait'; },
    item => { item.externalTypeAliases[0].artifact.kind = 'unknown'; },
  ]) {
    const changed = structuredClone(registryContext); mutate(changed);
    assert.throws(() => reviewedGenericAliasProbe(registered, undefined, changed, registrySpecs), /canonical root needs actual compiler identity/);
  }
  assert.equal(reviewedGenericAliasProbe({ ...registered, name: 'control::UnregisteredMap' }, undefined, registryContext, registrySpecs), undefined);
});
