import assert from 'node:assert/strict';
import test from 'node:test';
import {featurePlan, requiredFeatures, requireFeatureReceipts} from './accepted-feature-contracts.mjs';

const name = 'control::Record';
const pkg = {package: 'control', packageId: 'control-id', features: [], types: [{name}],
  publicAliases: [], defaultWitnesses: [],
  locals: [{label: 'local'}], delegatedLocals: [{name: 'helper', runtimeOwner: name}]};
const metadata = {packages: [{id: pkg.packageId, features: {native: [], health: []}}]};
const configuration = {target: 'native', features: [], jobs: 1};
const contract = {shape: 'map', requiredFeatures: ['native']};

test('required feature declarations reject empty duplicate unknown and classification-only escapes', () => {
  assert.deepEqual(requiredFeatures({}, name), []);
  for (const value of [[], '', [''], ['native', 'native']]) {
    assert.throws(() => requiredFeatures({...contract, requiredFeatures: value}, name), /requiredFeatures|duplicate/);
  }
  for (const other of [{category: 'non-decoder'}, {shape: 'typed'}]) {
    assert.throws(() => requiredFeatures({...contract, ...other}, name), /executed decoder observations/);
  }
  assert.throws(() => featurePlan([pkg], {[name]: {...contract, requiredFeatures: ['missing']}}, metadata, configuration), /not declared by its Cargo package/);
});

test('feature planning preserves ordinary locals and visibly gates owners until their native build', () => {
  const original = structuredClone(pkg);
  const plan = featurePlan([pkg], {[name]: contract}, metadata, configuration);
  assert.deepEqual(plan.ordinary[0].types, []);
  assert.deepEqual(plan.ordinary[0].locals, pkg.locals);
  assert.deepEqual(plan.ordinary[0].delegatedLocals, []);
  assert.deepEqual(plan.groups, [{package: 'control', features: ['native'], owners: [name]}]);
  assert.equal(plan.gated[0].status, 'native-gated');
  assert.equal(plan.gated[0].observed, false);
  assert.deepEqual(pkg, original);
  const native = {...pkg, features: ['native']};
  const active = featurePlan([native], {[name]: contract}, metadata, {...configuration, features: ['native']});
  assert.deepEqual(active.ordinary, [native]);
  assert.deepEqual(active.groups, []);
  assert.deepEqual(active.gated, []);
});

test('default measurements and required native observations cannot be replaced by classification or missing receipts', () => {
  const group = {owners: [name], features: ['native']};
  const valid = {name, classificationOnly: false, configuration: {target: 'native', features: ['native']}, compilerFeatures: ['native'], failures: []};
  requireFeatureReceipts(group, [valid]);
  requireFeatureReceipts(group, [valid, valid]);
  for (const rows of [[], [{...valid, classificationOnly: true}], [{...valid, compilerFeatures: []}], [{...valid, configuration: {target: 'wasm32-unknown-unknown', features: ['native']}}]]) {
    assert.throws(() => requireFeatureReceipts(group, rows), /did not execute/);
  }
  const ordinary = featurePlan([pkg], {}, metadata, configuration);
  assert.deepEqual(ordinary.ordinary, [pkg]);
  assert.deepEqual(ordinary.gated, []);
});
