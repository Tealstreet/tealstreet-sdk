import assert from 'node:assert/strict';
import test from 'node:test';
import { reviewedGenericAliasProbe } from './accepted-reviewed-aliases.mjs';
import { genericAliasProbe } from './accepted-callable-alias.mjs';

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
});
