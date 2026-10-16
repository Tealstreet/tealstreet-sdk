const assert = require('node:assert/strict');
const { oracleRequire, verifyPackage } = require('./fixtures/oracle-packages.cjs');
verifyPackage('starknet');
const { typedData } = oracleRequire('starknet');

const tuples = ['(,)', '(felt,)', '(,felt)'];
const shapes = ['scalar', 'nested', 'array', 'enum', 'merkle'];
const ignoredKinds = ['null', 'object', 'huge'];
const variants = [0, 1];
const schemas = ['unselected', 'presets', 'dependencies', 'utf16-order', 'raw-quote'];
const whitespace = ['( )', '(\t)', '(\ufeff)', '(\u0085)', '(felt, felt)', '(felt,\tfelt)', ' () ', '(, felt)'];
const prime = (1n << 251n) + 17n * (1n << 192n) + 1n;
const rows = [];

function ignored(kind) {
  if (kind === 'null') return null;
  if (kind === 'object') return { mustNotVisit: prime.toString(), nested: [null, {}] };
  assert.equal(kind, 'huge');
  return prime.toString();
}

function dataFor(tuple, shape, ignoredKind, variantIndex) {
  const selected = { name: 'Chosen', type: tuple };
  const other = { name: 'Other', type: '(felt)' };
  const types = {
    StarknetDomain: [
      { name: 'name', type: 'shortstring' },
      { name: 'version', type: 'shortstring' },
      { name: 'chainId', type: 'shortstring' },
      { name: 'revision', type: 'shortstring' },
    ],
    Choice: variantIndex === 0 ? [selected, other] : [other, selected],
    Wrapper: [{ name: 'marker', type: 'felt' }, { name: 'choice', type: 'enum', contains: 'Choice' }],
  };
  const choice = live => ({ Chosen: tuple.slice(1, -1).split(',').map(token => token === '' ? ignored(ignoredKind) : String(live)) });
  const wrapper = live => ({ marker: String(live + 100), choice: choice(live) });
  let fields, message;
  if (shape === 'scalar') {
    fields = types.Wrapper;
    message = wrapper(17);
  } else if (shape === 'nested') {
    types.Outer = [{ name: 'wrapped', type: 'Wrapper' }, { name: 'tail', type: 'felt' }];
    fields = [{ name: 'outer', type: 'Outer' }];
    message = { outer: { wrapped: wrapper(17), tail: '43' } };
  } else if (shape === 'array') {
    fields = [{ name: 'items', type: 'Wrapper*' }];
    message = { items: [wrapper(17), wrapper(29)] };
  } else if (shape === 'enum') {
    types.Branch = [{ name: 'Unused', type: '(felt)' }, { name: 'Wrapped', type: '(Wrapper)' }];
    fields = [{ name: 'branch', type: 'enum', contains: 'Branch' }];
    message = { branch: { Wrapped: [wrapper(17)] } };
  } else {
    assert.equal(shape, 'merkle');
    fields = [{ name: 'tree', type: 'merkletree', contains: 'Wrapper' }];
    message = { tree: [wrapper(17), wrapper(29), wrapper(41)] };
  }
  types.Message = fields;
  return { types, domain: { name: 'Fixture', version: '1', chainId: 'TEST', revision: '1' }, primaryType: 'Message', message };
}

function schemaFor(tuple, schema) {
  const data = dataFor(tuple, 'scalar', 'null', 1);
  if (schema === 'unselected') {
    data.types.Choice = [{ name: 'Live', type: '(felt)' }, { name: 'Dormant', type: tuple }];
    data.message.choice = { Live: ['17'] };
  } else if (schema === 'presets') {
    data.types.Message = [...data.types.Message, { name: 'token', type: 'TokenAmount' }, { name: 'nft', type: 'NftId' }];
    data.message.token = { token_address: '0x123', amount: { low: '19', high: '23' } };
    data.message.nft = { collection_address: '0x456', token_id: { low: '29', high: '31' } };
  } else if (schema === 'dependencies') {
    data.types.Zeta = [{ name: 'alpha', type: 'Alpha' }, { name: 'tail', type: 'felt' }];
    data.types.Alpha = [{ name: 'n', type: 'felt' }];
    data.types.Message = [...data.types.Message, { name: 'z', type: 'Zeta' }, { name: 'a', type: 'Alpha' }];
    data.message.z = { alpha: { n: '37' }, tail: '41' };
    data.message.a = { n: '43' };
  } else if (schema === 'utf16-order') {
    const names = ['\ue000', '\ud800\udc00'];
    for (const [index, name] of names.entries()) {
      data.types[name] = [{ name: 'n', type: 'felt' }];
      data.types.Message = [...data.types.Message, { name: 'f' + index, type: name }];
      data.message['f' + index] = { n: String(47 + index) };
    }
  } else {
    assert.equal(schema, 'raw-quote');
    data.types.Message = data.types.Message.map(field => field.name === 'marker' ? { ...field, name: 'marker"slash\\' } : field);
    data.message['marker"slash\\'] = data.message.marker;
    delete data.message.marker;
  }
  return data;
}

function capture(id, tuple, data, accepted) {
  const row = { id, tuple, accepted, raw: JSON.stringify(data), address: '0x1234' };
  try {
    row.hash = typedData.getMessageHash(data, row.address);
    row.typeEncoding = typedData.encodeType(data.types, 'Message', '1');
    row.typeHash = typedData.getTypeHash(data.types, 'Message', '1');
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', accepted, id);
  assert.equal(!!row.error, !accepted, id);
  rows.push(row);
}

for (const tuple of tuples) for (const shape of shapes) for (const kind of ignoredKinds) for (const index of variants) {
  capture(`slot/${tuple}/${shape}/${kind}/${index}`, tuple, dataFor(tuple, shape, kind, index), true);
}
for (const tuple of tuples) for (const schema of schemas) {
  capture(`schema/${tuple}/${schema}`, tuple, schemaFor(tuple, schema), true);
}
for (const tuple of whitespace) for (const length of [0, 1, 2]) {
  const data = dataFor('(,)', 'scalar', 'null', 0);
  data.types.Choice[0].type = tuple;
  data.message.choice.Chosen = Array.from({ length }, () => '17');
  capture(`whitespace/${tuple}/${length}`, tuple, data, false);
}
assert.equal(rows.length, 129, 'source_empty_slots_case_floor');
assert.equal(new Set(rows.map(row => row.id)).size, 129, 'source_empty_slots_unique_inventory');
const counterMeasurements = tuples.map(tuple => {
  const original = dataFor(tuple, 'scalar', 'object', 1);
  const zeroCopy = structuredClone(original);
  tuple.slice(1, -1).split(',').forEach((token, index) => {
    if (token === '') zeroCopy.message.choice.Chosen[index] = '0';
  });
  const normalizedDefinition = structuredClone(zeroCopy);
  normalizedDefinition.types.Choice[1].type = '(' + tuple.slice(1, -1).split(',').map(token => token || 'felt').join(',') + ')';
  const hash = typedData.getMessageHash(original, '0x1234');
  const zeroHash = typedData.getMessageHash(zeroCopy, '0x1234');
  const normalizedHash = typedData.getMessageHash(normalizedDefinition, '0x1234');
  assert.equal(hash, zeroHash, tuple);
  assert.notEqual(hash, normalizedHash, tuple);
  return {tuple, hash, zeroHash, normalizedHash};
});
assert.equal(counterMeasurements.length, 3);
process.stdout.write(JSON.stringify({ tuples, shapes, ignoredKinds, variants, schemas, whitespace, rows, counterMeasurements }));
