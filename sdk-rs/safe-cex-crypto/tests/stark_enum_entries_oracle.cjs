const assert = require('node:assert/strict');
const { oracleRequire, verifyPackage } = require('./fixtures/oracle-packages.cjs');
verifyPackage('starknet');
const { typedData } = oracleRequire('starknet');
const prime = '3618502788666131213697322783095070105623107215331596699973092056135872020481';
const schemas = {
  normal: [['One', '(felt)'], ['Two', '(felt)']],
  numeric: [['2', '(felt)'], ['1', '(felt)']],
  index: [['0', '(felt)'], ['1', '(felt)']],
  zero: [['Zero', '()'], ['One', '(felt)']],
  pair: [['Pair', '(felt,felt)'], ['Other', '(felt)']],
  partial: [['Partial', '(,felt)'], ['Other', '(felt)']],
};
const inputs = [
  ['single', 'normal', '{"One":["17"]}', 'One', 'hash'],
  ['extra-known', 'normal', '{"One":["17"],"Two":["23"]}', 'One', 'hash'],
  ['extra-null', 'normal', '{"One":["17"],"Unknown":null}', 'One', 'hash'],
  ['extra-malformed', 'normal', '{"One":["17"],"Unknown":{"bad":true}}', 'One', 'hash'],
  ['extra-range', 'normal', `{"One":["17"],"Two":["${prime}"]}`, 'One', 'hash'],
  ['key-order', 'numeric', '{"2":["23"],"1":["17"]}', '1', 'hash'],
  ['array-enum', 'index', '[["17"],["23"]]', '0', 'hash'],
  ['string-enum', 'index', '"17"', '0', 'hash'],
  ['bool-enum', 'index', 'true', null, 'refuse'],
  ['number-enum', 'index', '17', null, 'refuse'],
  ['empty-object', 'normal', '{}', null, 'refuse'],
  ['empty-array', 'index', '[]', null, 'refuse'],
  ['empty-string', 'index', '""', null, 'refuse'],
  ['null-enum', 'normal', 'null', null, 'refuse'],
  ['zero-null', 'zero', '{"Zero":null}', 'Zero', 'hash'],
  ['zero-bool', 'zero', '{"Zero":true}', 'Zero', 'hash'],
  ['zero-object', 'zero', '{"Zero":{"bad":true}}', 'Zero', 'hash'],
  ['zero-string', 'zero', '{"Zero":"ignored"}', 'Zero', 'hash'],
  ['zero-number', 'zero', '{"Zero":1e400}', 'Zero', 'hash'],
  ['pair-object', 'pair', '{"Pair":{"0":"17","1":"23","extra":null}}', 'Pair', 'hash'],
  ['pair-string', 'pair', '{"Pair":"17"}', 'Pair', 'hash'],
  ['pair-short-string', 'pair', '{"Pair":"1"}', 'Pair', 'refuse'],
  ['pair-bool', 'pair', '{"Pair":true}', 'Pair', 'refuse'],
  ['partial-object', 'partial', '{"Partial":{"0":null,"1":"23","extra":null}}', 'Partial', 'hash'],
  ['partial-string', 'partial', '{"Partial":"12"}', 'Partial', 'hash'],
  ['extra-zero-array', 'zero', '{"Zero":["1"]}', 'Zero', 'chosen-extra-array'],
  ['extra-pair-array', 'pair', `{"Pair":["17","23","${prime}"]}`, 'Pair', 'chosen-extra-array'],
];
const rows = [];
for (const [id, schema, inputRaw, firstKey, policy] of inputs) {
  const types = {
    StarknetDomain: [{ name: 'name', type: 'shortstring' }, { name: 'version', type: 'shortstring' }, { name: 'chainId', type: 'shortstring' }, { name: 'revision', type: 'shortstring' }],
    Choice: schemas[schema].map(([name, type]) => ({ name, type })),
    Message: [{ name: 'x', type: 'enum', contains: 'Choice' }],
  };
  const raw = JSON.stringify({ types, domain: { name: 'Fixture', version: '1', chainId: 'TEST', revision: '1' }, primaryType: 'Message', message: { x: '__ENUM_INPUT__' } }).replace('"__ENUM_INPUT__"', inputRaw);
  const parsed = JSON.parse(raw);
  const row = { id, schema, inputRaw, firstKey: null, policy, raw, address: '0x1234', accepted: policy !== 'refuse' };
  if (parsed.message.x !== null) row.firstKey = Object.keys(parsed.message.x)[0] ?? null;
  assert.equal(row.firstKey, firstKey, id);
  try {
    row.hash = typedData.getMessageHash(parsed, row.address);
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', row.accepted, id);
  assert.equal(!!row.error, !row.accepted, id);
  rows.push(row);
}
assert.equal(inputs.length, 27, 'enum_entries_source_input_floor');
assert.equal(rows.length, 27, 'enum_entries_source_case_floor');
assert.equal(new Set(rows.map(row => row.id)).size, 27);
assert.equal(rows.filter(row => row.accepted).length, 19);
assert.equal(rows.filter(row => !row.accepted).length, 8);
assert.equal(new Set(rows.slice(0, 5).map(row => row.hash)).size, 1, 'enum_entries_source_extra_keys_ignored');
assert.equal(new Set(rows.slice(14, 19).map(row => row.hash)).size, 1, 'enum_entries_source_zero_payload_not_visited');
process.stdout.write(JSON.stringify({ schemas, inputs, rows }));
