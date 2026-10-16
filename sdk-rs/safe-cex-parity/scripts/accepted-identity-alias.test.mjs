import assert from 'node:assert/strict';
import test from 'node:test';
import { identityAliasProbe } from './accepted-identity-alias.mjs';

const alias = {
  name: 'production::Writable',
  generics: { params: [{ name: 'T', kind: { type: { bounds: [], default: null, is_synthetic: false } } }], where_predicates: [] },
  rawTarget: { generic: 'T' }, target: { generic: 'T' },
};
test('identity aliases select one internal actual-alias monomorphization', () => {
  assert.deepEqual(identityAliasProbe(alias), { suffix: '<::serde_json::Value>', canonical: 'serde_json::value::Value', monomorphization: 'one fixed serde_json::Value identity-alias probe' });
  assert.equal(identityAliasProbe({ ...alias, generics: { params: [] } }), undefined);
  assert.equal(identityAliasProbe({ ...alias, suffix: '<Unrelated>', instantiation: 'Unrelated' }).suffix, '<::serde_json::Value>');
});
test('generic identity probes reject bounds defaults synthetic parameters and where clauses', () => {
  for (const change of [
    value => { value.generics.params[0].kind.type.bounds = [{ trait: 'Copy' }]; },
    value => { value.generics.params[0].kind.type.default = { primitive: 'bool' }; },
    value => { value.generics.params[0].kind.type.is_synthetic = true; },
    value => { value.generics.where_predicates = [{ bound_predicate: {} }]; },
    value => { value.generics.params.push({ name: 'U', kind: { type: { bounds: [], default: null, is_synthetic: false } } }); },
    value => { value.generics.params[0].kind = { const: { type: { primitive: 'usize' } } }; },
    value => { value.generics.params[0] = { name: "'a", kind: { lifetime: { outlives: [] } } }; },
  ]) {
    const changed = structuredClone(alias); change(changed);
    assert.throws(() => identityAliasProbe(changed), /generic alias requires explicit compiler monomorphization coverage/);
  }
});
test('expanded identity cannot conceal a nonidentity source target', () => {
  assert.throws(() => identityAliasProbe({ ...alias, rawTarget: { resolved_path: { path: 'Indirect', args: { generic: 'T' } } } }), /source must directly name its type parameter/);
  assert.throws(() => identityAliasProbe({ ...alias, target: { primitive: 'bool' } }), /compiler expansion must preserve its type parameter/);
});
