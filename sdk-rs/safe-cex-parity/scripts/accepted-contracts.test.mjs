import { loadPolicy } from './accepted-policy-files.mjs';
import assert from 'node:assert/strict';
import test from 'node:test';
import { contractFor } from './accepted-contracts.mjs';
import { parseJsonWithSource, stringifyJsonWithSource } from './accepted-json-source.mjs';
const owner = { name: 'production::Side', methods: [], conversions: [], generics: { params: [] } };
const strict = { shape: 'string', literals: ['buy', 'sell'], evidence: 'TypeScript Side union', entrypoints: [] };
test('multi-entry dispatch carries authored raw case tokens into native contracts', () => {
  const actual = { ...owner, deserialize: false, methods: [{ name: 'first', signature: {} }, { name: 'second', signature: {} }], rawEntrypoints: [true, true] };
  const contract = parseJsonWithSource(`{"shape":"observed","evidence":"Actual separately observed methods.","entrypoints":[${actual.methods.map(method => `{"api":${JSON.stringify({ kind: 'inherent', ...method })},"input":"observed","reason":"Real native output.","call":"$type::${method.name}($value)","cases":[{"input":-0,"output":0},{"input":{"9":9,"2":2,"x":1},"output":1}]}`).join(',')}]}`);
  const generated = contractFor(actual, { [actual.name]: contract });
  const serialized = stringifyJsonWithSource(generated);
  assert.equal((serialized.match(/"entrypoint":\d,"input":-0/g) ?? []).length, 2, 'raw primitive tokens survive generated dispatch');
  assert.equal((serialized.match(/"entrypoint":\d,"input":\{"9":9,"2":2,"x":1\}/g) ?? []).length, 2, 'raw object tokens survive generated dispatch');
});
test('new public or private Deserialize owner fails closed by name', () => {
  assert.throws(() => contractFor(owner, {}), /production::Side: accepted-set assertion: unclassified deserializable type/);
});
test('new conversion is not discharged by a strict serde contract', () => {
  const changed = { ...owner, conversions: [{ trait: 'core::str::traits::FromStr' }] };
  assert.throws(() => contractFor(changed, { [owner.name]: strict }), /production::Side: accepted-set assertion: unclassified or changed decoder/);
});
test('new inherent decoder fails even if its name is unrelated to parsing', () => {
  const changed = { ...owner, methods: [{ name: 'alternate', signature: {} }] };
  assert.throws(() => contractFor(changed, { [owner.name]: strict }), /unclassified or changed decoder/);
});
test('the declared closed string contract resolves', () => {
  assert.equal(contractFor(owner, { [owner.name]: strict }), strict);
  const native = { call: '$type($value)', nativeFields: ['left', 'right'], cases: [{ input: 'left', output: { ok: true, left: 1, right: 2 } }, { input: 'right', output: { ok: true, left: 3, right: 4 } }] };
  const supplemented = { ...strict, nativeObservation: native };
  assert.equal(contractFor(owner, { [owner.name]: supplemented }), supplemented);
  assert.throws(() => contractFor(owner, { [owner.name]: { ...supplemented, category: 'non-decoder' } }), /non-decoder cannot claim native decoder observations/);
  assert.throws(() => contractFor({ ...owner, deserialize: false }, { [owner.name]: supplemented }), /native observations need an executed decoder owner/);
  assert.throws(() => contractFor(owner, { [owner.name]: { ...supplemented, nativeObservation: { ...native, call: '$type($value); serde_json::to_value(decoded)' } } }), /cannot serialize decoded owners/);
  assert.throws(() => contractFor(owner, { [owner.name]: { ...supplemented, nativeObservation: { ...native, nativeFields: ['left', 'left'] } } }), /distinct named fields/);
  const missing = structuredClone(native); delete missing.cases[1].output.right;
  assert.throws(() => contractFor(owner, { [owner.name]: { ...supplemented, nativeObservation: missing } }), /successful expected field values/);

  assert.throws(() => contractFor(owner, { [owner.name]: { ...strict, observation: { call: '$value.is_ok()', cases: [] } } }), /Deserialize output witnesses require direct input\/output adapters/);
});

test('FromStr cannot be classified as an already-typed constructor', () => {
  const api = { kind: 'conversion', trait: 'core::str::traits::FromStr' };
  assert.throws(() => contractFor({ ...owner, conversions: [{ trait: api.trait }] }, { [owner.name]: { ...strict, entrypoints: [{ api, input: 'typed', reason: 'pretend typed' }] } }), /raw decoder cannot be classified typed/);
});

test('alternate-only raw adapters cannot omit forbidden inputs or claim typed shape', () => {
  const api = { kind: 'conversion', trait: 'core::convert::From<Value>' };
  const alternate = { ...owner, deserialize: false, conversions: [{ trait: api.trait }] };
  const contract = { shape: 'map', evidence: 'object contract', entrypoints: [{ api, input: 'value', reason: 'raw input', call: '$value.is_object()', positive: [{}] }] };
  assert.throws(() => contractFor(alternate, { [owner.name]: contract }), /requires explicit forbidden inputs/);
  assert.throws(() => contractFor(alternate, { [owner.name]: { ...contract, shape: 'typed' } }), /raw adapter needs a shape contract/);
});

test('all inherent input positions and compiler-expanded aliases reject typed decoder claims', () => {
  const api = { kind: 'inherent', name: 'decode', signature: { inputs: [['raw', { borrowed_ref: { type: { primitive: 'str' } } }], ['flag', { primitive: 'bool' }]] } };
  const typed = { shape: 'typed', evidence: 'Control typed constructor claim.', entrypoints: [{ api, input: 'typed', reason: 'Control deliberately incorrect raw classification.' }] };
  const actual = { ...owner, deserialize: false, methods: [{ name: api.name, signature: api.signature }] };
  assert.throws(() => contractFor(actual, { [owner.name]: typed }), /raw decoder cannot be classified typed/);
  const signature = { inputs: [['raw', { resolved_path: { path: 'production::PrivateWire' } }], ['flag', { primitive: 'bool' }]] };
  assert.throws(() => contractFor({ ...actual, methods: [{ name: api.name, signature }], rawEntrypoints: [true] }, { [owner.name]: { ...typed, entrypoints: [{ ...typed.entrypoints[0], api: { ...api, signature } }] } }), /raw decoder cannot be classified typed/);
});

test('byte conversion inputs cannot bypass typed constructor classification', () => {
  const api = { kind: 'conversion', trait: 'core::convert::From', args: { angle_bracketed: { args: [{ type: { resolved_path: { path: 'alloc::vec::Vec', args: { angle_bracketed: { args: [{ type: { primitive: 'u8' } }] } } } } }] } } };
  const actual = { ...owner, deserialize: false, conversions: [{ trait: api.trait, args: api.args }] };
  const contract = { shape: 'typed', evidence: 'Control byte constructor claim.', entrypoints: [{ api, input: 'typed', reason: 'Control deliberately incorrect byte classification.' }] };
  assert.throws(() => contractFor(actual, { [owner.name]: contract }), /raw decoder cannot be classified typed/);
});

test('multiple observed entrypoints execute independently and cannot claim non-decoder receipts', async () => {
  const methods = [{ name: 'first', signature: {} }, { name: 'second', signature: {} }];
  const actual = { ...owner, deserialize: false, methods, rawEntrypoints: [true, true] };
  const entrypoints = methods.map((api, i) => ({ api: { kind: 'inherent', ...api }, input: 'observed', reason: 'Executed opaque forwarding, not a typed-input claim.', call: `$type::${api.name}($value)`, cases: [{ input: 'left', output: { value: i } }, { input: 'right', output: { value: i + 2 } }] }));
  const contract = { shape: 'observed', evidence: 'Independent exact compiler entrypoints.', entrypoints };
  const resolve = changed => contractFor(actual, { [owner.name]: changed });
  const generated = resolve(contract);
  assert.deepEqual(generated.cases, [
    { input: { entrypoint: 0, input: 'left' }, output: { value: 0 } },
    { input: { entrypoint: 0, input: 'right' }, output: { value: 2 } },
    { input: { entrypoint: 1, input: 'left' }, output: { value: 1 } },
    { input: { entrypoint: 1, input: 'right' }, output: { value: 3 } },
  ]);
  assert.match(generated.call, /0 =>.*\$type::first\(__accepted_input\)/);
  assert.match(generated.call, /1 =>.*\$type::second\(__accepted_input\)/);
  assert.throws(() => resolve({ ...contract, category: 'non-decoder' }), /observed API cannot skip executed witnesses/);
  assert.throws(() => resolve({ ...contract, call: '$value', cases: [] }), /cannot override generated dispatch/);
  assert.throws(() => resolve({ ...contract, entrypoints: entrypoints.slice(0, 1) }), /unclassified or changed decoder/);
  const missing = structuredClone(contract); delete missing.entrypoints[1].cases;
  assert.throws(() => resolve(missing), /every observed entrypoint requires executed/);
  const repeated = structuredClone(contract); repeated.entrypoints[1].cases[1] = repeated.entrypoints[1].cases[0];
  assert.throws(() => resolve(repeated), /distinct inputs and outputs/);
  const single = { ...contract, entrypoints: entrypoints.slice(0, 1), call: '$value', cases: entrypoints[0].cases, category: 'non-decoder' };
  assert.throws(() => contractFor({ ...actual, methods: methods.slice(0, 1), rawEntrypoints: [true] }, { [owner.name]: single }), /observed API cannot skip executed witnesses/);
  const constructor = { name: 'new', signature: { inputs: [], output: { generic: 'Self' } } };
  const mixedOwner = { ...actual, methods: [methods[0], constructor], rawEntrypoints: [true, false] };
  const mixed = { ...contract, entrypoints: [entrypoints[0], { api: { kind: 'inherent', ...constructor }, input: 'typed', reason: 'Zero-argument construction does not decode a serialized input.' }] };
  const resolved = contractFor(mixedOwner, { [owner.name]: mixed });
  assert.deepEqual(resolved.cases, entrypoints[0].cases.map(row => ({ input: { entrypoint: 0, input: row.input }, output: row.output })), 'only the raw entrypoint gets generated runtime dispatch');
  assert.doesNotMatch(resolved.call, /::new/, 'typed constructors do not receive fabricated varying-input witnesses');
  assert.throws(() => contractFor({ ...mixedOwner, rawEntrypoints: [true, true] }, { [owner.name]: mixed }), /raw decoder cannot be classified typed/);
  const rawSignature = structuredClone(mixedOwner);
  rawSignature.methods[1].signature.inputs = [['raw', { borrowed_ref: { type: { primitive: 'str' } } }]];
  const disguised = structuredClone(mixed); disguised.entrypoints[1].api.signature = rawSignature.methods[1].signature;
  assert.throws(() => contractFor(rawSignature, { [owner.name]: disguised }), /raw decoder cannot be classified typed/);
  const noWitness = { ...mixed, entrypoints: mixed.entrypoints.map(entry => ({ ...entry, input: 'typed' })) };
  assert.throws(() => contractFor({ ...mixedOwner, rawEntrypoints: [false, false] }, { [owner.name]: noWitness }), /must execute an actual observed method/);
  const { spawnSync } = await import('node:child_process');
  const url = new URL('./accepted-contracts.mjs', import.meta.url).href;
  const anchor = "      if (entry.input === 'typed') validateTypedEntrypoint(type, entry, index);";
  const execute = mutate => {
    const hook = mutate ? `import {registerHooks} from 'node:module'; registerHooks({load(url,context,next){const loaded=next(url,context);if(url!==${JSON.stringify(url)})return loaded;const source=String(loaded.source);const anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2,'one mixed-observation typed validation site');return {...loaded,source:source.replace(anchor,'')};}});` : '';
    const program = `import assert from 'node:assert/strict'; ${hook} const {contractFor}=await import(${JSON.stringify(url)}); const owner=${JSON.stringify({ ...mixedOwner, rawEntrypoints: [true, true] })}; const contract=${JSON.stringify(mixed)}; assert.throws(()=>contractFor(owner,{[owner.name]:contract}),/raw decoder cannot be classified typed/,'observed typed raw compiler role must refuse');`;
    return spawnSync(process.execPath, ['--input-type=module', '--eval', program], { encoding: 'utf8' });
  };
  let result = execute(false); assert.equal(result.status, 0, result.stderr);
  result = execute(true); assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, /observed typed raw compiler role must refuse/);
  console.log('observed typed entrypoint raw compiler role: named assertion; process exit 1');
  result = execute(false); assert.equal(result.status, 0, result.stderr);
});

test('open JSON and supplemental methods require native witnesses without replacing shape checks', () => {
  const actual = { ...owner, deserialize: true, methods: [{ name: 'insert', signature: {} }], rawEntrypoints: [true] };
  const native = { call: '$type::insert($value)', nativeFields: ['first', 'second'], cases: [{ input: 'left', output: { ok: true, first: 1, second: 2 } }, { input: 'right', output: { ok: true, first: 3, second: 4 } }] };
  const entry = { api: { kind: 'inherent', ...actual.methods[0] }, input: 'observed', reason: 'Direct native mutator output.', ...native };
  const contract = { shape: 'map', evidence: 'Actual map admission, with independently observed mutator.', entrypoints: [entry] };
  assert.equal(contractFor(actual, { [owner.name]: contract }), contract);
  const open = { ...contract, shape: 'json', nativeObservation: native };
  assert.equal(contractFor(actual, { [owner.name]: open }), open);
  assert.throws(() => contractFor(actual, { [owner.name]: { ...open, nativeObservation: undefined } }), /open JSON needs direct native observations/);
  const missing = structuredClone(contract); delete missing.entrypoints[0].cases[0].output.second;
  assert.throws(() => contractFor(actual, { [owner.name]: missing }), /successful expected field values/);
  const wire = structuredClone(contract); wire.entrypoints[0].call += '; serde_json::to_value(decoded)';
  assert.throws(() => contractFor(actual, { [owner.name]: wire }), /cannot serialize decoded owners/);
  const typed = structuredClone(contract); typed.entrypoints[0].input = 'typed';
  assert.throws(() => contractFor(actual, { [owner.name]: typed }), /raw decoder cannot be classified typed/);
  const absent = { ...contract, entrypoints: [] };
  assert.throws(() => contractFor(actual, { [owner.name]: absent }), /unclassified or changed decoder/);
});

test('self-only Visitor unit witnesses cannot invent varying input or exempt data methods', () => {
  const unit = { trait: 'serde_core::de::Visitor', name: 'visit_unit', signature: { inputs: [['self', { generic: 'Self' }]] } };
  const other = { trait: 'serde_core::de::Visitor', name: 'visit_bool', signature: { inputs: [['self', { generic: 'Self' }], ['v', { primitive: 'bool' }]] } };
  const actual = { ...owner, deserialize: false, traitMethods: [unit, other] };
  const entrypoints = [
    { api: { kind: 'trait', ...unit }, input: 'observed', reason: 'The actual method has no data argument.', unitInput: true, call: '$type::visit_unit()', cases: [{ input: null, output: { kind: 'null' } }] },
    { api: { kind: 'trait', ...other }, input: 'observed', reason: 'Actual data argument varies.', call: '$type::visit_bool($value)', cases: [{ input: false, output: false }, { input: true, output: true }] },
  ];
  const contract = { shape: 'observed', evidence: 'Direct compiler-owned methods.', entrypoints };
  assert.equal(contractFor(actual, { [owner.name]: contract }).cases.length, 3);
  const fake = structuredClone(contract); fake.entrypoints[1].unitInput = true;
  assert.throws(() => contractFor(actual, { [owner.name]: fake }), /exact compiler self-only Visitor method/);
  const invented = structuredClone(contract); invented.entrypoints[0].cases.push({ input: 1, output: { kind: 'null' } });
  assert.throws(() => contractFor(actual, { [owner.name]: invented }), /exactly one honest constant-input witness/);
});

test('reviewed host operations bind exact compiler bodies and cannot exempt cache decoders', async () => {
  const { createHash } = await import('node:crypto'); const { fileURLToPath } = await import('node:url');
  const bytes = Buffer.from('host operation review fixture');
  const source = { filename: 'safe-cex-candle-cache/src/native.rs', begin: [1, 1], end: [4, 2] };
  const api = { kind: 'trait', trait: 'safe_cex_candle_cache::types::CandleCacheKeyValueStore', name: 'get', signature: {} };
  const actual = { ...owner, name: 'safe_cex_candle_cache::native::FileStore', deserialize: false, traitMethods: [{ trait: api.trait, name: api.name, signature: api.signature }], rawEntrypoints: [true], entrypointSources: [source] };
  const entry = { api, input: 'non-decoder', reason: 'Host reads an opaque stored string.', evidence: 'Fixture review.', source: { file: source.filename, begin: source.begin, end: source.end, sha256: createHash('sha256').update(bytes).digest('hex') } };
  const contract = { shape: 'typed', evidence: 'Host I/O classification, no execution claim.', entrypoints: [entry] };
  const context = { packageMetadata: { name: 'safe-cex-candle-cache', manifest_path: fileURLToPath(new URL('../../safe-cex-candle-cache/Cargo.toml', import.meta.url)) }, readSource: () => bytes };
  assert.equal(contractFor(actual, { [actual.name]: contract }, context), contract);
  assert.throws(() => contractFor(actual, { [actual.name]: contract }, { ...context, readSource: () => Buffer.concat([bytes, Buffer.from(' changed')]) }), /reviewed host operation source changed/);
  assert.throws(() => contractFor(actual, { [actual.name]: contract }, { ...context, packageMetadata: { ...context.packageMetadata, manifest_path: 'impostor/Cargo.toml' } }), /actual SDK compiler package/);
  const shifted = structuredClone(contract); shifted.entrypoints[0].source.begin = [2, 1];
  assert.throws(() => contractFor(actual, { [actual.name]: shifted }, context), /reviewed host body span changed/);
  const wrong = structuredClone(actual); wrong.entrypointSources[0].filename = 'redirect.rs';
  assert.throws(() => contractFor(wrong, { [actual.name]: contract }, context), /supplied compiler body/);
  const loader = { ...actual, name: 'safe_cex_candle_cache::native::RedisCandleCacheBackend' };
  assert.throws(() => contractFor(loader, { [loader.name]: contract }, context), /unreviewed host non-decoder binding/);
  const changed = structuredClone(contract); changed.entrypoints[0].api.name = 'load_chunks';
  assert.throws(() => contractFor({ ...actual, traitMethods: [{ ...actual.traitMethods[0], name: 'load_chunks' }] }, { [actual.name]: changed }, context), /unreviewed host non-decoder binding/);
});

test('reviewed serializer constants pin compiler source adapters and witnesses', async () => {
  const fs = await import('node:fs');
  const { fileURLToPath } = await import('node:url');
  const { spawnSync } = await import('node:child_process');
  const specs = Object.values(loadPolicy('constant-outputs')).filter(spec => spec.owner === 'safe_cex_num::numeric_ingress::output::Capture');
  assert.equal(specs.length, 1, 'only tuple-struct metadata remains constant');
  assert.deepEqual(specs.map(s => s.api.name), ['serialize_tuple_struct']);
  const { loadContracts } = await import('./accepted-contract-files.mjs');
  const unit = loadContracts()[specs[0].owner].entrypoints.find(entry => entry.api.name === 'serialize_unit_struct');
  assert.equal(unit.constantOutput, undefined, 'name-sensitive unit capture uses ordinary observations');
  assert.deepEqual(unit.cases.map(row => row.output), [['null'], ['null'], ['undefined']]);
  const apis = [specs[0].api, unit.api];
  const actual = { name: specs[0].owner, ident: 'Capture', kind: 'record', deserialize: false, generics: { params: [] }, methods: [], conversions: [], traitMethods: apis.map(({kind, ...api}) => api), rawEntrypoints: [true, true], entrypointSources: [{ filename: specs[0].source.file, begin: specs[0].source.begin, end: specs[0].source.end }, { filename: specs[0].source.file, begin: [95, 5], end: [101, 6] }] };
  const contract = { shape: 'observed', evidence: 'Only tuple metadata is constant; unit metadata preserves Undefined.', entrypoints: [{ api: specs[0].api, input: 'observed', constantOutput: true, reason: 'The actual tuple constructor ignores its metadata.', call: specs[0].call, cases: specs[0].cases }, unit] };
  const bytes = fs.readFileSync(new URL('../../safe-cex-num/src/numeric_ingress/output.rs', import.meta.url));
  const packageMetadata = { name: 'safe-cex-num', manifest_path: fileURLToPath(new URL('../../safe-cex-num/Cargo.toml', import.meta.url)) };
  const context = { packageMetadata, readSource: () => bytes };
  const resolve = (type = actual, c = contract, ctx = context) => contractFor(type, { [type.name]: c }, ctx);
  assert.equal(resolve().cases.length, 5, 'tuple metadata and all three unit metadata observations execute');
  const ordinary = structuredClone(contract); delete ordinary.entrypoints[0].constantOutput;
  assert.throws(() => resolve(actual, ordinary), /distinct inputs and outputs/);
  assert.throws(() => resolve({ ...actual, name: 'unreviewed::Capture' }), /unreviewed owner or method/);
  assert.throws(() => resolve(actual, contract, { ...context, packageMetadata: { ...packageMetadata, name: 'other' } }), /actual SDK compiler package/);
  const changedSpan = structuredClone(actual); changedSpan.entrypointSources[0].end[0]++;
  assert.throws(() => resolve(changedSpan), /compiler source span changed/);
  assert.throws(() => resolve(actual, contract, { ...context, readSource: () => Buffer.concat([bytes, Buffer.from(' changed')]) }), /reviewed source changed/);
  const changedCall = structuredClone(contract); changedCall.entrypoints[0].call = '$type; $value.clone()';
  assert.throws(() => resolve(actual, changedCall), /reviewed native adapter changed/);
  const changedCases = structuredClone(contract); changedCases.entrypoints[0].cases[1].output = 'invented echo';
  assert.throws(() => resolve(actual, changedCases), /reviewed native witnesses changed/);
  const repeated = structuredClone(contract); repeated.entrypoints[0].cases[1].input = 'First';
  assert.throws(() => resolve(actual, repeated), /reviewed native witnesses changed/);
  const changedApi = structuredClone(actual); changedApi.traitMethods[0].signature.output = { primitive: 'bool' };
  const matching = structuredClone(contract); matching.entrypoints[0].api.signature.output = { primitive: 'bool' };
  assert.throws(() => resolve(changedApi, matching), /compiler API changed/);
  for (const [label, anchor, badType, badContract, changedSource, message] of [
    ['constant serializer source pin', "  assert.equal(createHash('sha256').update(context.readSource(spec.source.file)).digest('hex'), spec.source.sha256, `${label}: reviewed source changed`);", actual, contract, true, 'reviewed source changed'],
    ['constant serializer compiler API pin', '  assert.deepEqual(entry.api, spec.api, `${label}: compiler API changed`);', changedApi, matching, false, 'compiler API changed'],
    ['constant serializer adapter pin', '  assert.deepEqual(entry.call, spec.call, `${label}: reviewed native adapter changed`);', actual, changedCall, false, 'reviewed native adapter changed'],
    ['constant serializer native witness pin', '  assert.deepEqual(entry.cases, spec.cases, `${label}: reviewed native witnesses changed`);', actual, changedCases, false, 'reviewed native witnesses changed'],
  ]) {
    const execute = mutate => {
      const target = new URL('./accepted-constant-output.mjs', import.meta.url).href;
      const hook = mutate ? `import {registerHooks} from 'node:module'; registerHooks({load(url,ctx,next){const r=next(url,ctx);if(url!==${JSON.stringify(target)})return r;const s=String(r.source),a=${JSON.stringify(anchor)};assert.equal(s.split(a).length,2,'one reviewed constant-output guard');return {...r,source:s.replace(a,'')};}});` : '';
      const source = `import assert from 'node:assert/strict'; import fs from 'node:fs'; ${hook} const {contractFor}=await import(${JSON.stringify(new URL('./accepted-contracts.mjs', import.meta.url).href)}); const owner=${JSON.stringify(badType)}; const contract=${JSON.stringify(badContract)}; const context={packageMetadata:${JSON.stringify(packageMetadata)},readSource:()=>${changedSource ? 'Buffer.from("unreviewed source")' : `fs.readFileSync(${JSON.stringify(fileURLToPath(new URL('../../safe-cex-num/src/numeric_ingress/output.rs', import.meta.url)))})`}}; assert.throws(()=>contractFor(owner,{[owner.name]:contract},context),new RegExp(${JSON.stringify(message)}),${JSON.stringify(label + ' must refuse')});`;
      return spawnSync(process.execPath, ['--input-type=module', '--eval', source], { encoding: 'utf8' });
    };
    let result = execute(false); assert.equal(result.status, 0, result.stderr);
    result = execute(true); assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, /must refuse/);
    console.log(`${label}: named assertion; process exit 1`);
    result = execute(false); assert.equal(result.status, 0, result.stderr);
  }
});

test('reviewed direct transport constant pins raw compiler source and native capability observations', async () => {
  const fs = await import('node:fs');
  const { fileURLToPath } = await import('node:url');
  const { spawnSync } = await import('node:child_process');
  const spec = Object.values(loadPolicy('constant-outputs')).find(spec => spec.owner === 'safe_cex_transport::direct::DirectTransport');
  assert.ok(spec);
  const { loadContracts } = await import('./accepted-contract-files.mjs');
  const routing = loadContracts()[spec.owner].entrypoints.find(entry => entry.api.name === 'ws_routing');
  assert.ok(routing);
  // Policy fixture only; the native control inventories the complete real owner.
  const { kind, ...method } = spec.api;
  const { kind: routingKind, ...routingMethod } = routing.api;
  const actual = { name: spec.owner, ident: spec.ident, kind: 'record', deserialize: false, generics: { params: [] }, methods: [], conversions: [], traitMethods: [method, routingMethod], rawEntrypoints: [true, false], entrypointSources: [{ filename: spec.source.file, begin: spec.source.begin, end: spec.source.end }] };
  const contract = { shape: 'observed', evidence: 'Exact source-pinned native capability observation.', entrypoints: [{ api: spec.api, input: 'observed', constantOutput: true, reason: 'Actual no-connection result on distinct URL inputs.', call: spec.call, cases: spec.cases }, routing] };
  const root = fileURLToPath(new URL('../..', import.meta.url));
  const packageMetadata = { name: 'safe-cex-transport', manifest_path: fileURLToPath(new URL('../../safe-cex-transport/Cargo.toml', import.meta.url)) };
  const context = { packageMetadata, readSource: file => fs.readFileSync(new URL(file, new URL('../../', import.meta.url))) };
  const resolve = (type = actual, value = contract, ctx = context) => contractFor(type, { [type.name]: value }, ctx);
  assert.equal(resolve().cases.length, 2);
  const ordinary = structuredClone(contract); delete ordinary.entrypoints[0].constantOutput;
  assert.throws(() => resolve(actual, ordinary), /distinct inputs and outputs/);
  assert.throws(() => resolve({ ...actual, name: 'unreviewed::DirectTransport' }), /unreviewed owner or method/);
  assert.throws(() => resolve({ ...actual, rawEntrypoints: [false, false] }), /actual indexed raw method/);
  assert.throws(() => resolve(actual, contract, { ...context, packageMetadata: { ...packageMetadata, name: 'safe-cex-num' } }), /actual SDK compiler package/);
  const shifted = structuredClone(actual); shifted.entrypointSources[0].begin[0]++;
  assert.throws(() => resolve(shifted), /compiler source span changed/);
  const changed = structuredClone(contract); changed.entrypoints[0].call = '$type; $value.clone()';
  assert.throws(() => resolve(actual, changed), /native adapter changed/);
  const echoes = structuredClone(contract); echoes.entrypoints[0].cases[1].output = 'invented echo';
  assert.throws(() => resolve(actual, echoes), /native witnesses changed/);
  const actualRead = context.readSource;
  for (const [file, message] of [[spec.source.file, /reviewed source changed/], [spec.typescript.file, /maintained direct source changed/]]) {
    assert.throws(() => resolve(actual, contract, { ...context, readSource: name => name === file ? Buffer.concat([actualRead(name), Buffer.from('\n')]) : actualRead(name) }), message);
  }
  for (const [label, anchor, raw, file, refusal] of [
    ['direct constant maintained source pin', "    assert.equal(createHash('sha256').update(bytes).digest('hex'), spec.typescript.sha256, `${label}: maintained direct source changed`);", true, spec.typescript.file, 'maintained direct source changed'],
    ['direct constant indexed raw method pin', "  assert.equal(type.rawEntrypoints?.[index], true, `${label}: actual indexed raw method required`);", false, null, 'actual indexed raw method'],
  ]) {
    const type = { ...actual, rawEntrypoints: [raw, false] };
    const run = mutant => {
      const moduleUrl = new URL('./accepted-constant-output.mjs', import.meta.url).href;
      const hook = mutant ? `import {registerHooks} from 'node:module';registerHooks({load(url,ctx,next){const loaded=next(url,ctx);if(url!==${JSON.stringify(moduleUrl)})return loaded;const source=String(loaded.source),anchor=${JSON.stringify(anchor)};assert.equal(source.split(anchor).length,2);return {...loaded,source:source.replace(anchor,'')};}});` : '';
      return spawnSync(process.execPath, ['--input-type=module', '--eval', `import assert from 'node:assert/strict';import fs from 'node:fs';import path from 'node:path';${hook}const {contractFor}=await import(${JSON.stringify(new URL('./accepted-contracts.mjs', import.meta.url).href)});const type=${JSON.stringify(type)},contract=${JSON.stringify(contract)};const context={packageMetadata:${JSON.stringify(packageMetadata)},readSource:file=>{const bytes=fs.readFileSync(path.resolve(${JSON.stringify(root)},file));return file===${JSON.stringify(file)}?Buffer.concat([bytes,Buffer.from([10])]):bytes;}};assert.throws(()=>contractFor(type,{[type.name]:contract},context),new RegExp(${JSON.stringify(refusal)}),${JSON.stringify(label + ' must refuse')});`], { encoding: 'utf8' });
    };
    let result = run(false); assert.equal(result.status, 0, result.stderr);
    result = run(true); assert.equal(result.status, 1, result.stderr); assert.match(result.stderr, new RegExp(label + ' must refuse'));
    console.log(`${label}: named assertion; process exit 1`);
    result = run(false); assert.equal(result.status, 0, result.stderr);
  }
});
