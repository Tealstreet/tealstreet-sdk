const assert = require('node:assert/strict');
const { oracleRequire, verifyPackage } = require('./fixtures/oracle-packages.cjs');
verifyPackage('starknet');
const { typedData } = oracleRequire('starknet');

const tuples = [null, '()', '(,)', '(felt,)', '(,felt)'];
const variantTypes = ['( )', '(\t)', '(\ufeff)', '(\u0085)', '(felt, felt)', '(felt,\tfelt)', '(NoSuchType)', '(,NoSuchType)', '(\n)', '(\r)', '(\u2028)', '(\u2029)', 'NoSuchType', ' () '];
const rows = [];
for (const tuple of tuples) {
  const types = {
    StarkNetDomain: [{ name: 'name', type: 'felt' }, { name: 'version', type: 'felt' }, { name: 'chainId', type: 'felt' }],
    Message: [{ name: 'x', type: 'felt' }],
  };
  if (tuple !== null) types.Unused = [{ name: 'Never', type: tuple }];
  const data = { types, domain: { name: 'Fixture', version: '1', chainId: 'TEST' }, primaryType: 'Message', message: { x: '17' } };
  const row = {
    revision: 0,
    selected: false,
    accepted: true,
    tuple,
    raw: JSON.stringify(data),
    address: '0x1234',
    typeEncoding: typedData.encodeType(types, 'Message', '0'),
    hash: typedData.getMessageHash(data, '0x1234'),
  };
  assert.equal(row.typeEncoding, 'Message(x:felt)');
  assert.equal(row.hash, '0x6150946379f2b4213bbdd67b7f04c160007e5c7bb00748386d025152b9a8358');
  rows.push(row);
}
assert.equal(rows.length, 5, 'unused_enum_source_v0_case_floor');
assert.equal(new Set(rows.map(row => row.hash)).size, 1, 'unused_enum_source_unchanged_hash');
for (const tuple of variantTypes) for (const selected of [false, true]) {
  const types = {
    StarknetDomain: [{ name: 'name', type: 'shortstring' }, { name: 'version', type: 'shortstring' }, { name: 'chainId', type: 'shortstring' }, { name: 'revision', type: 'shortstring' }],
    Choice: [{ name: 'Live', type: '(felt)' }, { name: 'Dormant', type: tuple }],
    Message: [{ name: 'choice', type: 'enum', contains: 'Choice' }, { name: 'marker', type: 'felt' }],
  };
  const values = tuple.slice(1, -1).split(',').map(token => token === '' ? null : '17');
  const data = { types, domain: { name: 'Fixture', version: '1', chainId: 'TEST', revision: '1' }, primaryType: 'Message', message: { choice: selected ? { Dormant: values } : { Live: ['17'] }, marker: '99' } };
  const row = { revision: 1, tuple, selected, accepted: !selected, raw: JSON.stringify(data), address: '0x1234' };
  try {
    row.hash = typedData.getMessageHash(data, row.address);
    row.typeEncoding = typedData.encodeType(types, 'Message', '1');
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', !selected, `${tuple}/${selected}`);
  assert.equal(!!row.error, selected, `${tuple}/${selected}`);
  rows.push(row);
}
assert.equal(rows.length, 33, 'unused_enum_source_case_floor');
assert.equal(new Set(rows.filter(row => row.revision === 1 && !row.selected).map(row => row.hash)).size, 14, 'unused_enum_source_original_declaration_hashes');
process.stdout.write(JSON.stringify({ tuples, variantTypes, rows }));
