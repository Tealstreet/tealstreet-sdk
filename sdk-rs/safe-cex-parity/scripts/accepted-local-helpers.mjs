import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

export function reviewedGenericDecoderHelper(local, helpers, source, owners, contracts) {
  if (!local.ident.startsWith('UNSUPPORTED_GENERIC:')) return false;
  const helper = helpers[local.label];
  if (!helper) return false;
  const label = `${local.label}: accepted-set assertion`;
  assert.equal(helper.category, 'private-generic-decoder', `${label}: unsupported generic helper category`);
  assert.deepEqual([local.line, local.column], helper.declarationEnd, `${label}: helper lexical declaration changed`);
  assert.equal(createHash('sha256').update(source).digest('hex'), helper.sourceSha256, `${label}: reviewed generic helper source changed`);
  assert.ok(helper.reason?.trim(), `${label}: generic helper needs reviewed evidence`);
  const owner = owners.find(owner => owner.name === helper.runtimeOwner);
  assert.ok(owner?.deserialize && owner.generics?.params?.length, `${label}: generic helper needs its production generic Deserialize owner`);
  const contract = contracts[helper.runtimeOwner];
  assert.ok(contract?.instantiation?.trim() && contract.monomorphization?.trim(), `${label}: generic helper needs a fixed decoder monomorphization`);
  requireNativeDecoderObservations(owner, contract, local.label);
  assert.deepEqual(contract.nativeObservation.nativeFields, helper.nativeFields, `${label}: generic helper native fields changed`);
  return true;
}

export function reviewedLocalHelper(local, helpers, source, owners, contracts) {
  const helper = helpers[local.label];
  if (!helper) return false;
  const label = `${local.label}: accepted-set assertion`;
  assert.equal(helper.category, 'private-map-companion', `${label}: unsupported helper category`);
  assert.deepEqual([local.line, local.column], helper.declarationEnd, `${label}: helper lexical declaration changed`);
  assert.equal(createHash('sha256').update(source).digest('hex'), helper.sourceSha256, `${label}: reviewed helper source changed`);
  assert.ok(helper.reason?.trim(), `${label}: helper classification needs evidence`);
  const owner = owners.find(owner => owner.name === helper.runtimeOwner);
  assert.ok(owner?.deserialize, `${label}: helper needs its inventoried production Deserialize owner`);
  const contract = contracts[helper.runtimeOwner];
  assert.equal(contract?.shape, 'map', `${label}: helper requires a public map boundary`);
  const observation = contract.observation;
  assert.ok(observation?.call?.includes('$type') && observation.call.includes('$value'), `${label}: helper needs production observations`);
  assert.doesNotMatch(observation.call, /serde_json::to_string/, `${label}: native helper observations cannot be wire projections`);
  assert.ok(helper.nativeFields?.length >= 2, `${label}: helper needs independent expected native fields`);
  assert.deepEqual(observation.nativeFields, helper.nativeFields, `${label}: helper native fields changed`);
  const positives = observation.cases?.filter(row => row.output?.ok === true) ?? [];
  assert.ok(positives.length >= 2 && positives.every(row => helper.nativeFields.every(field => Object.hasOwn(row.output, field))), `${label}: helper needs native values for every successful observation`);
  assert.ok(observation.cases.some(row => row.input === '[]' && row.output?.ok === false), `${label}: helper needs a rejected public sequence witness`);
  return true;
}

// Decoder helper classifications require independent native values in addition
// to their exact source review. Older direct Response observations remain valid.
export function requireNativeDecoderObservations(owner, contract, label) {
  const message = `${label}: accepted-set assertion`;
  assert.notEqual(contract?.category, 'non-decoder', `${message}: decoder helper cannot use a non-decoder contract`);
  const hasNativePhase = contract?.nativeObservation !== undefined;
  const legacyRuns = owner?.deserialize === true && owner.kind !== 'function' && contract?.shape !== 'observed';
  assert.ok(hasNativePhase || legacyRuns, `${message}: legacy native observation requires its actual Deserialize owner; function and observed APIs need nativeObservation`);
  const observation = hasNativePhase ? contract.nativeObservation : contract?.observation;
  assert.ok(observation?.call?.includes('$type') && observation.call.includes('$value'), `${message}: decoder helper needs direct native observations`);
  assert.doesNotMatch(observation.call, /serde_json::to_(?:string|value|vec)/, `${message}: decoder helper native observations cannot serialize decoded owners`);
  assert.ok(Array.isArray(observation.nativeFields) && observation.nativeFields.length >= 2 && observation.nativeFields.every(field => typeof field === 'string' && field.trim()) && new Set(observation.nativeFields).size === observation.nativeFields.length, `${message}: decoder helper needs distinct native fields`);
  const positives = observation.cases?.filter(row => row.output?.ok === true) ?? [];
  assert.ok(positives.length >= 2 && positives.every(row => observation.nativeFields.every(field => Object.hasOwn(row.output, field))), `${message}: decoder helper needs expected native field values`);
}
