import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import { loadContracts } from './accepted-contract-files.mjs';
import fs from 'node:fs';
import test from 'node:test';
import { reviewedLocalHelper, requireNativeDecoderObservations } from './accepted-local-helpers.mjs';

const helpers = loadPolicy('local-type-helpers');
const contracts = loadContracts();
const source = fs.readFileSync(new URL('../../safe-cex-types/src/ws_auth.rs', import.meta.url));
const local = { label: 'safe-cex-types/src/ws_auth.rs::Wire', line: 170, column: 9 };
const owners = [{ name: 'safe_cex_types::ws_auth::WsAuthResponse', deserialize: true }];
const check = (item = local, bytes = source, inventory = owners, rules = contracts) => reviewedLocalHelper(item, helpers, bytes, inventory, rules);

test('private map companion requires exact lexical source and inventoried parent', () => {
  assert.equal(check(), true);
  assert.equal(check({ ...local, label: 'new::Wire' }), false);
  assert.throws(() => check({ ...local, line: 171 }), /helper lexical declaration changed/);
  assert.throws(() => check(local, Buffer.concat([source, Buffer.from('\n')])), /reviewed helper source changed/);
  assert.throws(() => check(local, source, []), /inventoried production Deserialize owner/);
});
test('private map companion cannot use wire projections or omit native fields', () => {
  const changed = structuredClone(contracts);
  changed[owners[0].name].observation.call += '; serde_json::to_string(&decoded)';
  assert.throws(() => check(local, source, owners, changed), /native helper observations cannot be wire projections/);
  const missing = structuredClone(contracts);
  delete missing[owners[0].name].observation.cases[0].output.error;
  assert.throws(() => check(local, source, owners, missing), /native values for every successful observation/);
  // Canonical Response already has a direct legacy observation; Request/Payload
  // may use a separate native-first supplement while retaining wire witnesses.
  requireNativeDecoderObservations(owners[0], contracts[owners[0].name], 'ResponseVisitor');
  const native = { call: '$type($value)', nativeFields: ['left', 'right'], cases: [{ input: 'left', output: { ok: true, left: 1, right: 2 } }, { input: 'right', output: { ok: true, left: 3, right: 4 } }] };
  requireNativeDecoderObservations(owners[0], { nativeObservation: native, observation: { call: 'serde_json::to_string($type($value))' } }, 'RequestVisitor');
  assert.throws(() => requireNativeDecoderObservations(owners[0], { observation: { ...native, call: 'serde_json::to_string($type($value))' } }, 'RequestVisitor'), /cannot serialize decoded owners/);
  assert.throws(() => requireNativeDecoderObservations(owners[0], { observation: { ...native, call: 'serde_json::to_value($type($value))' } }, 'PayloadVisitor'), /cannot serialize decoded owners/);
  assert.throws(() => requireNativeDecoderObservations(owners[0], { observation: { ...native, nativeFields: [] } }, 'PayloadVisitor'), /distinct native fields/);
  assert.throws(() => requireNativeDecoderObservations(owners[0], { category: 'non-decoder', nativeObservation: native }, 'PayloadVisitor'), /cannot use a non-decoder contract/);
  // Even a Deserialize-capable owner cannot use legacy observation fields when
  // dispatch selects the function/observed auditor. contractFor independently
  // rejects this observed owner; keep the helper's own eligibility exact too.
  const observedDeserializeOwner = { name: 'canonical::ObservedDeserialize', kind: 'record', deserialize: true };
  assert.throws(() => requireNativeDecoderObservations(observedDeserializeOwner, { category: 'decoder', shape: 'observed', call: '$type($value)', observation: native }, 'ObservedDeserializeVisitor'), /legacy native observation requires its actual Deserialize owner/);
  requireNativeDecoderObservations(observedDeserializeOwner, { category: 'decoder', shape: 'observed', call: '$type($value)', nativeObservation: native }, 'ObservedDeserializeVisitor');
  const functionDeserializeOwner = { name: 'canonical::FunctionDeserialize', kind: 'function', deserialize: true };
  assert.throws(() => requireNativeDecoderObservations(functionDeserializeOwner, { category: 'decoder', call: '$type($value)', observation: native }, 'FunctionDeserializeVisitor'), /legacy native observation requires its actual Deserialize owner/);
  const functionOwner = { name: 'canonical::payload', kind: 'function', deserialize: false };
  const seedOwner = { name: 'canonical::Seed', kind: 'record', deserialize: false };
  const legacy = { category: 'decoder', call: 'serde_json::to_string($type($value))', observation: native };
  // These legacy fields are ignored by their actual function/observed auditors.
  assert.throws(() => requireNativeDecoderObservations(functionOwner, legacy, 'PayloadVisitor'), /legacy native observation requires its actual Deserialize owner/);
  assert.throws(() => requireNativeDecoderObservations(seedOwner, { ...legacy, shape: 'observed' }, 'SeedVisitor'), /legacy native observation requires its actual Deserialize owner/);
  requireNativeDecoderObservations(functionOwner, { ...legacy, nativeObservation: native }, 'PayloadVisitor');
  requireNativeDecoderObservations(seedOwner, { ...legacy, shape: 'observed', nativeObservation: native }, 'SeedVisitor');
  const incomplete = structuredClone(native); delete incomplete.cases[1].output.right;
  assert.throws(() => requireNativeDecoderObservations(owners[0], { nativeObservation: incomplete, observation: native }, 'PayloadVisitor'), /expected native field values/);

});
test('private map companion requires the public sequence rejection witness', () => {
  const changed = structuredClone(contracts);
  changed[owners[0].name].observation.cases = changed[owners[0].name].observation.cases.filter(row => row.input !== '[]');
  assert.throws(() => check(local, source, owners, changed), /rejected public sequence witness/);
});
