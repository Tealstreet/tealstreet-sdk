const assert = require('node:assert/strict');
const { oracleRequire, verifyPackage } = require('./fixtures/oracle-packages.cjs');
verifyPackage('starknet');
const { typedData } = oracleRequire('starknet');

const tokens = ['17', '9007199254740993', '18446744073709551615', '340282366920938463463374607431768211455', '1e20', '1e30', '1e3', '1.25e2', '1.0000000000000001', '-1e-4000', '-0', '-0.0', '1e-4000', '1.5', '1e400', '-1e400', '-1'];
const rounded = ['17', '9007199254740992', '18446744073709551616', '340282366920938463463374607431768211456', '100000000000000000000', '1000000000000000019884624838656', '1000', '125', '1', '0', '0', '0', '0', null, null, null, '-1'];
const shapesV0 = ['scalar', 'nested', 'array', 'domain', 'ignored', 'ignored-nested'];
const shapesV1 = [...shapesV0, 'preset', 'enum', 'ignored-slot', 'u128', 'i128'];
const placeholder = '__RAW_NUMBER__';
const rows = [];

function dataFor(revision, shape) {
  const domainName = revision === 0 ? 'StarkNetDomain' : 'StarknetDomain';
  const domainKind = revision === 0 ? 'felt' : 'shortstring';
  const types = {
    [domainName]: [{ name: 'name', type: domainKind }, { name: 'version', type: domainKind }, { name: 'chainId', type: domainKind }],
    Message: [{ name: 'x', type: 'felt' }],
  };
  const domain = { name: 'Fixture', version: '1', chainId: 'TEST' };
  if (revision === 1) {
    types[domainName].push({ name: 'revision', type: 'shortstring' });
    domain.revision = '1';
  }
  const data = { types, domain, primaryType: 'Message', message: { x: placeholder } };
  let path = ['message', 'x'];
  if (shape === 'nested' || shape === 'ignored-nested') {
    types.Box = [{ name: 'x', type: 'felt' }];
    types.Message = [{ name: 'box', type: 'Box' }];
    data.message = { box: { x: shape === 'nested' ? placeholder : '17' } };
    path = ['message', 'box', 'x'];
    if (shape === 'ignored-nested') data.message.box.extra = placeholder;
  } else if (shape === 'array') {
    types.Message = [{ name: 'items', type: 'felt*' }];
    data.message = { items: ['19', placeholder, '23'] };
    path = ['message', 'items', 1];
  } else if (shape === 'domain') {
    data.message.x = '17';
    domain.version = placeholder;
    path = ['domain', 'version'];
  } else if (shape === 'preset') {
    assert.equal(revision, 1);
    types.Message = [{ name: 'token', type: 'TokenAmount' }];
    data.message = { token: { token_address: '0x123', amount: { low: placeholder, high: '23' } } };
    path = ['message', 'token', 'amount', 'low'];
  } else if (shape === 'enum' || shape === 'ignored-slot') {
    assert.equal(revision, 1);
    types.Choice = [{ name: 'Other', type: '(felt)' }, { name: 'Chosen', type: shape === 'enum' ? '(felt)' : '(,felt)' }];
    types.Message = [{ name: 'choice', type: 'enum', contains: 'Choice' }];
    data.message = { choice: { Chosen: shape === 'enum' ? [placeholder] : [placeholder, '17'] } };
    path = ['message', 'choice', 'Chosen', 0];
  } else if (shape === 'u128' || shape === 'i128') {
    assert.equal(revision, 1);
    types.Message[0].type = shape;
  } else if (shape === 'ignored') {
    data.message = { x: '17', extra: placeholder };
    domain.extra = placeholder;
    path = ['message', 'extra'];
  } else {
    assert.equal(shape, 'scalar');
  }
  return { data, path };
}

function leaf(data, path) {
  return path.reduce((value, key) => value[key], data);
}

function replaceLeaf(data, path, value) {
  const parent = path.slice(0, -1).reduce((value, key) => value[key], data);
  parent[path.at(-1)] = value;
}

for (const revision of [0, 1]) for (const shape of revision === 0 ? shapesV0 : shapesV1) for (const [index, token] of tokens.entries()) {
  const { data, path } = dataFor(revision, shape);
  const raw = JSON.stringify(data).replaceAll(JSON.stringify(placeholder), token);
  const parsed = JSON.parse(raw);
  const ignored = shape.startsWith('ignored');
  const number = leaf(parsed, path);
  const observed = shape === 'ignored-nested' ? parsed.message.box.extra : number;
  assert.equal(typeof observed, 'number');
  const bits = Buffer.alloc(8);
  bits.writeDoubleBE(observed);
  const bounded = ['preset', 'u128', 'i128'].includes(shape);
  const expected = ignored || (index < 13 && !(bounded && index === 3)) || (shape === 'i128' && index === 16);
  const row = { id: `json/${revision}/${shape}/${index}`, revision, shape, token, raw, address: '0x1234', accepted: expected, numberText: String(observed), numberBits: bits.toString('hex') };
  if (!ignored && rounded[index] !== null) assert.equal(BigInt(number).toString(), rounded[index]);
  try {
    row.hash = typedData.getMessageHash(parsed, row.address);
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', expected, row.id);
  assert.equal(!!row.error, !expected, row.id);
  if (expected && !ignored) {
    const converted = structuredClone(parsed);
    replaceLeaf(converted, path, rounded[index]);
    row.roundedHash = typedData.getMessageHash(converted, row.address);
    assert.equal(row.hash, row.roundedHash, row.id);
  } else if (ignored) {
    const baseline = JSON.parse(JSON.stringify(data).replaceAll(JSON.stringify(placeholder), '17'));
    row.ignoredBaselineHash = typedData.getMessageHash(baseline, row.address);
    assert.equal(row.hash, row.ignoredBaselineHash, row.id);
  }
  rows.push(row);
}
assert.equal(rows.length, 289, 'numeric_json_source_case_floor');
assert.equal(new Set(rows.map(row => row.id)).size, 289, 'numeric_json_source_unique_inventory');
assert.equal(rows.filter(row => row.accepted).length, 239, 'numeric_json_source_accepted_floor');
assert.equal(rows.filter(row => !row.accepted).length, 50, 'numeric_json_source_refused_floor');
const coercionRaw = ['true', 'false', 'null', '[]', '[1]', '["0x10"]', '[1,2]', '{}', '"0x10"', '"0b10"', '"0o10"', '" 17 "', '"\\ufeff17\\ufeff"', '"+17"', '"-1"', '"1e3"', '""'];
const coercionKinds = [[0, 'felt'], [0, 'shortstring'], [1, 'felt'], [1, 'shortstring'], [1, 'u128'], [1, 'i128']];
const coercionRows = [];
for (const [revision, kind] of coercionKinds) for (const [index, valueRaw] of coercionRaw.entries()) {
  const { data } = dataFor(revision, 'scalar');
  data.types.Message[0].type = kind;
  const raw = JSON.stringify(data).replace(JSON.stringify(placeholder), valueRaw);
  const parsed = JSON.parse(raw);
  const accepted = ![2, 6, 7].includes(index) && !(index === 14 && kind !== 'i128') && !(index === 15 && ['u128', 'i128'].includes(kind));
  const row = { id: `coercion/${revision}/${kind}/${index}`, revision, kind, valueRaw, raw, address: '0x1234', accepted };
  try {
    row.hash = typedData.getMessageHash(parsed, row.address);
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', accepted, row.id);
  assert.equal(!!row.error, !accepted, row.id);
  coercionRows.push(row);
}
assert.equal(coercionRows.length, 102, 'numeric_json_source_coercion_floor');
assert.equal(new Set(coercionRows.map(row => row.id)).size, 102, 'numeric_json_source_coercion_unique_inventory');
assert.equal(coercionRows.filter(row => row.accepted).length, 77, 'numeric_json_source_coercion_accepted_floor');
assert.equal(coercionRows.filter(row => !row.accepted).length, 25, 'numeric_json_source_coercion_refused_floor');
const recursiveInputs = [
  ['i128/enum/number', 'i128', '/message/choice/Chosen/0', '9007199254740993'],
  ['i128/enum/radix', 'i128', '/message/choice/Chosen/0', '"0x10"'],
  ['i128/merkle/number', 'i128', '/message/tree/0/x', '9007199254740993'],
  ['i128/merkle/radix', 'i128', '/message/tree/0/x', '"0x10"'],
  ['u256/root-extra', 'u256', '/message/value/extra', '1e400'],
  ['u256/nested-extra', 'u256', '/message/holder/value/extra', '1e400'],
  ['TokenAmount/root-extra', 'TokenAmount', '/message/value/extra', '1e400'],
  ['TokenAmount/nested-extra', 'TokenAmount', '/message/value/amount/extra', '1e400'],
  ['NftId/root-extra', 'NftId', '/message/value/extra', '1e400'],
  ['NftId/nested-extra', 'NftId', '/message/value/token_id/extra', '1e400'],
];
const recursiveRows = [];
for (const [id, kind, pointer, valueRaw] of recursiveInputs) {
  const { data } = dataFor(1, 'scalar');
  if (id.startsWith('i128/enum/')) {
    data.types.Choice = [{ name: 'Chosen', type: '(i128)' }];
    data.types.Message = [{ name: 'choice', type: 'enum', contains: 'Choice' }];
    data.message = { choice: { Chosen: [placeholder] } };
  } else if (id.startsWith('i128/merkle/')) {
    data.types.SignedBox = [{ name: 'x', type: 'i128' }];
    data.types.Message = [{ name: 'tree', type: 'merkletree', contains: 'SignedBox' }];
    data.message = { tree: [{ x: placeholder }, { x: '23' }] };
  } else {
    const words = { low: '17', high: '23' };
    const value = kind === 'u256' ? words : kind === 'TokenAmount' ? { token_address: '0x123', amount: words } : { collection_address: '0x123', token_id: words };
    if (id === 'u256/nested-extra') {
      data.types.Holder = [{ name: 'value', type: 'u256' }];
      data.types.Message = [{ name: 'holder', type: 'Holder' }];
      data.message = { holder: { value } };
    } else {
      data.types.Message = [{ name: 'value', type: kind }];
      data.message = { value };
    }
    if (id.endsWith('root-extra') || kind === 'u256') value.extra = placeholder;
    else words.extra = placeholder;
  }
  const raw = JSON.stringify(data).replace(JSON.stringify(placeholder), valueRaw);
  const parsed = JSON.parse(raw);
  const hash = typedData.getMessageHash(parsed, '0x1234');
  const baseline = structuredClone(parsed);
  const path = pointer.slice(1).split('/');
  if (kind === 'i128') replaceLeaf(baseline, path, BigInt(leaf(parsed, path)).toString());
  else delete path.slice(0, -1).reduce((value, key) => value[key], baseline)[path.at(-1)];
  const baselineHash = typedData.getMessageHash(baseline, '0x1234');
  assert.equal(hash, baselineHash, id);
  recursiveRows.push({ id, kind, pointer, valueRaw, raw, address: '0x1234', accepted: true, hash, baselineHash });
}
assert.equal(recursiveRows.length, 10, 'numeric_json_recursive_source_floor');
assert.equal(new Set(recursiveRows.map(row => row.id)).size, 10, 'numeric_json_recursive_unique_inventory');
process.stdout.write(JSON.stringify({ tokens, rounded, shapesV0, shapesV1, rows, coercionRaw, coercionKinds, coercionRows, recursiveInputs, recursiveRows }));
