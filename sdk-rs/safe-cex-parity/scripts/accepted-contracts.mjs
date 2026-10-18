import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { reviewedConstantOutput } from './accepted-constant-output.mjs';
import { copyJsonSourceProperty } from './accepted-json-source.mjs';

export function contractFor(type, contracts, context = {}) {
  const contract = contracts[type.name];
  assert.ok(contract, `${type.name}: accepted-set assertion: unclassified deserializable type`);
  if (contract.nativeObservation !== undefined) {
    const native = contract.nativeObservation;
    assert.notEqual(contract.category, 'non-decoder', `${type.name}: accepted-set assertion: non-decoder cannot claim native decoder observations`);
    assert.ok(type.deserialize !== false || (type.kind === 'function' && contract.category === 'decoder') || contract.shape === 'observed', `${type.name}: accepted-set assertion: native observations need an executed decoder owner`);
    validateNative(type.name, native);
  }
  if (type.kind === 'function') {
    assert.deepEqual(contract.api, type.methods[0], `${type.name}: accepted-set assertion: unclassified or changed free-function decoder`);
    assert.ok(contract.evidence?.trim(), `${type.name}: accepted-set assertion: function classification requires evidence`);
    assert.ok(['decoder', 'non-decoder'].includes(contract.category), `${type.name}: accepted-set assertion: function requires explicit decoder classification`);
    if (contract.category === 'decoder') {
      assert.ok(contract.call?.includes('$value') && contract.cases?.length >= 2 && contract.cases.every(row => Object.hasOwn(row, 'input') && Object.hasOwn(row, 'output')), `${type.name}: accepted-set assertion: free decoder requires executed input/output witnesses`);
    } else assert.ok(contract.reason?.trim(), `${type.name}: accepted-set assertion: non-decoder classification requires its specific reason`);
    return contract;
  }
  assert.ok((['string', 'map', 'union', 'uninhabited'].includes(contract.shape) || (type.deserialize !== false && contract.shape === 'json') || (type.deserialize === false && ['typed', 'observed'].includes(contract.shape))), `${type.name}: accepted-set assertion: unknown shape classification`);
  if (contract.shape === 'json') assert.ok(contract.nativeObservation, `${type.name}: accepted-set assertion: open JSON needs direct native observations`);
  assert.ok(typeof contract.evidence === 'string' && contract.evidence.trim(), `${type.name}: accepted-set assertion: classification needs contract evidence`);
  if (contract.shape === 'string') {
    assert.ok(Array.isArray(contract.literals) && contract.literals.length && contract.literals.every(value => typeof value === 'string'), `${type.name}: accepted-set assertion: string contract needs expected literals`);
  }
  if (contract.shape === 'union') {
    assert.ok(Array.isArray(contract.positive) && contract.positive.length && Array.isArray(contract.negative) && contract.negative.length, `${type.name}: accepted-set assertion: union requires positive and negative corpus`);
  }
  if (contract.observation !== undefined) {
    assert.notEqual(type.deserialize, false, `${type.name}: accepted-set assertion: Deserialize output witnesses require a Deserialize owner`);
    const observation = contract.observation;
    assert.ok(observation.call?.includes('$type') && observation.call?.includes('$value') && observation.cases?.length >= 2 && observation.cases.every(row => Object.hasOwn(row, 'input') && Object.hasOwn(row, 'output')), `${type.name}: accepted-set assertion: Deserialize output witnesses require direct input/output adapters`);
  }
  const actual = [
    ...type.methods.map(method => ({ kind: 'inherent', ...method })),
    ...type.conversions.map(conversion => ({ kind: 'conversion', ...conversion })),
    ...(type.traitMethods ?? []).map(method => ({ kind: 'trait', ...method })),
  ];
  const expected = contract.entrypoints ?? [];
  assert.deepEqual(actual, expected.map(entry => entry.api), `${type.name}: accepted-set assertion: unclassified or changed decoder/conversion entrypoint`);
  for (const entry of expected) if (entry.constantOutput !== undefined) assert.ok(contract.shape === 'observed' && expected.length > 1 && entry.input === 'observed' && entry.unitInput === undefined, `${type.name}: accepted-set assertion: constant output requires a reviewed multi-entry observed API`);
  if (type.rawEntrypoints) assert.ok(type.rawEntrypoints.length === actual.length && type.rawEntrypoints.every(value => typeof value === 'boolean'), `${type.name}: accepted-set assertion: compiler raw-input roles must cover every entrypoint`);
  for (const [index, entry] of expected.entries()) if (entry.input === 'non-decoder') validateHostEntrypoint(type, entry, index, context);
  if (contract.shape === 'observed') {
    assert.equal(type.deserialize, false, `${type.name}: accepted-set assertion: observed API cannot replace a Deserialize shape contract`);
    assert.ok(contract.category === undefined || contract.category === 'decoder', `${type.name}: accepted-set assertion: observed API cannot skip executed witnesses as non-decoder`);
    assert.ok(expected.length > 0, `${type.name}: accepted-set assertion: observed API requires exact entrypoints`);
    for (const [index, entry] of expected.entries()) {
      assert.ok(['observed', 'typed', 'non-decoder'].includes(entry.input) && entry.reason?.trim(), `${type.name}: accepted-set assertion: observed API classification requires reason`);
      if (entry.input === 'typed') validateTypedEntrypoint(type, entry, index);
    }
    const observed = expected.filter(entry => entry.input === 'observed');
    assert.ok(observed.length, `${type.name}: accepted-set assertion: observed API must execute an actual observed method`);
    if (expected.length === 1) {
      assert.ok(contract.call?.includes('$value') && contract.cases?.length >= 2 && contract.cases.every(row => Object.hasOwn(row, 'input') && Object.hasOwn(row, 'output')), `${type.name}: accepted-set assertion: observed API requires executed input/output witnesses`);
      return contract;
    }
    assert.ok(!Object.hasOwn(contract, 'call') && !Object.hasOwn(contract, 'cases'), `${type.name}: accepted-set assertion: multi-entry observed witnesses cannot override generated dispatch`);
    for (const entry of observed) {
      if (entry.unitInput === true) {
        assert.deepEqual([entry.api.kind, entry.api.trait, entry.api.name, entry.api.signature?.inputs], ['trait', 'serde_core::de::Visitor', 'visit_unit', [['self', { generic: 'Self' }]]], `${type.name}: accepted-set assertion: unit witness requires the exact compiler self-only Visitor method`);
        assert.ok(entry.call?.includes('$type') && entry.cases?.length === 1 && entry.cases[0].input === null && Object.hasOwn(entry.cases[0], 'output'), `${type.name}: accepted-set assertion: unit method needs exactly one honest constant-input witness`);
        assert.doesNotMatch(entry.call, /serde_json::to_(?:string|value|vec)/, `${type.name}: accepted-set assertion: observed methods cannot serialize decoded owners`);
        continue;
      }
      assert.ok(entry.call?.includes('$type') && entry.call.includes('$value') && entry.cases?.length >= 2 && entry.cases.every(row => Object.hasOwn(row, 'input') && Object.hasOwn(row, 'output')), `${type.name}: accepted-set assertion: every observed entrypoint requires executed input/output witnesses`);
      assert.doesNotMatch(entry.call, /serde_json::to_(?:string|value|vec)/, `${type.name}: accepted-set assertion: observed methods cannot serialize decoded owners`);
      const constant = reviewedConstantOutput(type, entry, expected.indexOf(entry), context);
      assert.ok(new Set(entry.cases.map(row => JSON.stringify(row.input))).size >= 2 && (constant || new Set(entry.cases.map(row => JSON.stringify(row.output))).size >= 2), `${type.name}: accepted-set assertion: observed entrypoint requires distinct inputs and outputs`);
    }
    const call = `{ match $value["entrypoint"].as_u64().expect("internally generated observed entrypoint") { ${observed.map((entry, index) => `${index} => { ${entry.call.includes('$value') ? 'let __accepted_input = &$value["input"];' : ''} ${entry.call.replaceAll('$value', '__accepted_input')} },`).join(' ')} _ => panic!("unclassified observed entrypoint") } }`;
    const cases = observed.flatMap((entry, index) => entry.cases.map(row => {
      const input = copyJsonSourceProperty({ entrypoint: index, input: row.input }, 'input', row);
      return copyJsonSourceProperty({ input, output: row.output }, 'output', row);
    }));
    return { ...contract, call, cases };
  }
  for (const [index, entry] of expected.entries()) {
    if (entry.input === 'non-decoder') continue;
    if (entry.input === 'observed') {
      assert.notEqual(type.deserialize, false, `${type.name}: accepted-set assertion: supplemental observations require the ordinary Deserialize audit`);
      assert.ok(entry.reason?.trim() && entry.call?.includes('$type') && entry.call.includes('$value') && entry.cases?.length >= 2, `${type.name}: accepted-set assertion: supplemental entrypoint needs executed native witnesses`);
      validateNative(type.name, entry);
      assert.ok(new Set(entry.cases.map(row => JSON.stringify(row.input))).size >= 2 && new Set(entry.cases.map(row => JSON.stringify(row.output))).size >= 2, `${type.name}: accepted-set assertion: supplemental entrypoint needs distinct inputs and outputs`);
      continue;
    }
    assert.ok(['typed', 'value', 'string', 'json-text', 'map', 'number'].includes(entry.input), `${type.name}: accepted-set assertion: unknown entrypoint input contract`);
    assert.ok(entry.reason?.trim(), `${type.name}: accepted-set assertion: entrypoint requires a reason`);
    if (entry.input === 'typed') validateTypedEntrypoint(type, entry, index);
    if (entry.input !== 'typed') {
      assert.ok(entry.call?.includes('$value') && entry.positive?.length, `${type.name}: accepted-set assertion: entrypoint requires runtime adapter`);
      assert.notEqual(contract.shape, 'typed', `${type.name}: accepted-set assertion: raw adapter needs a shape contract`);
      if (type.deserialize === false && contract.shape !== 'string') assert.ok(Array.isArray(contract.negative) && contract.negative.length, `${type.name}: accepted-set assertion: alternate decoder requires explicit forbidden inputs`);
    }
  }
  if (type.deserialize !== false && type.generics.params.length) assert.ok(contract.instantiation, `${type.name}: accepted-set assertion: generic type needs classified monomorphization`);
  return contract;
}

function validateTypedEntrypoint(type, entry, index) {
  const signature = JSON.stringify({ trait: entry.api.trait, inputs: entry.api.kind === 'conversion' ? entry.api.args : entry.api.signature?.inputs });
  assert.ok(!type.rawEntrypoints?.[index] && !/FromStr|serde_json::(?:value::)?(?:Value|Map)|"primitive":"(?:str|u8)"|alloc::string::String/.test(signature), `${type.name}: accepted-set assertion: raw decoder cannot be classified typed`);
}

function validateNative(name, native) {
    assert.ok(native.call?.includes('$type') && native.call.includes('$value') && native.cases?.length >= 2 && native.cases.every(row => Object.hasOwn(row, 'input') && Object.hasOwn(row, 'output')), `${name}: accepted-set assertion: native observations need direct input/output witnesses`);
    assert.doesNotMatch(native.call, /serde_json::to_(?:string|value|vec)/, `${name}: accepted-set assertion: native observations cannot serialize decoded owners`);
    assert.ok(Array.isArray(native.nativeFields) && native.nativeFields.length >= 2 && native.nativeFields.every(field => typeof field === 'string' && field.trim()) && new Set(native.nativeFields).size === native.nativeFields.length, `${name}: accepted-set assertion: native observations need distinct named fields`);
    const positives = native.cases.filter(row => row.output?.ok === true);
    assert.ok(positives.length >= 2 && positives.every(row => native.nativeFields.every(field => Object.hasOwn(row.output, field))), `${name}: accepted-set assertion: native observations need successful expected field values`);
}

// Finite reviewed host bindings. Parsing cache methods and unknown bindings never
// inherit permission from their receiver, String arguments, or neighboring API.
function validateHostEntrypoint(type, entry, index, { readSource, packageMetadata }) {
  const key = `${type.name}::${entry.api.kind === 'trait' ? `${entry.api.trait}::` : ''}${entry.api.name}`;
  const permitted = new Set([
    ...['FileStore', 'RedisStore'].flatMap(owner => ['get', 'set', 'delete', 'list_keys'].map(method => `safe_cex_candle_cache::native::${owner}::safe_cex_candle_cache::types::CandleCacheKeyValueStore::${method}`)),
    'safe_cex_candle_cache::native::FilesystemCandleCacheBackend::open',
    'safe_cex_candle_cache::native::RedisCandleCacheBackend::connect',
  ]);
  assert.ok(permitted.has(key) && type.deserialize === false && entry.api.kind !== 'conversion', `${type.name}: accepted-set assertion: unreviewed host non-decoder binding`);
  assert.ok(packageMetadata?.name === 'safe-cex-candle-cache' && packageMetadata.manifest_path === fileURLToPath(new URL('../../safe-cex-candle-cache/Cargo.toml', import.meta.url)), `${type.name}: accepted-set assertion: host binding requires the actual SDK compiler package`);
  const source = type.entrypointSources?.[index];
  assert.ok(source?.filename === 'safe-cex-candle-cache/src/native.rs' && entry.source?.file === source.filename && entry.reason?.trim() && entry.evidence?.trim(), `${type.name}: accepted-set assertion: host binding needs its supplied compiler body and specific review`);
  assert.deepEqual([entry.source.begin, entry.source.end], [source.begin, source.end], `${type.name}: accepted-set assertion: reviewed host body span changed`);
  assert.equal(typeof readSource, 'function', `${type.name}: accepted-set assertion: host source reader required`);
  assert.equal(entry.source.sha256, createHash('sha256').update(readSource(source.filename)).digest('hex'), `${type.name}: accepted-set assertion: reviewed host operation source changed`);
}
