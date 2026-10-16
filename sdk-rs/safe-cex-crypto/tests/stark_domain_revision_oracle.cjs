const assert = require('node:assert/strict');
const { oracleRequire, verifyPackage } = require('./fixtures/oracle-packages.cjs');
verifyPackage('starknet');
const { typedData } = oracleRequire('starknet');
const inputs = [
  ['missing', null], ['null', 'null'], ['string-zero', '"0"'], ['string-one', '"1"'],
  ['leading-zero', '"01"'], ['plus-one', '"+1"'], ['hex-one', '"0x1"'],
  ['number-zero', '0'], ['number-one', '1'], ['true', 'true'], ['false', 'false'],
  ['empty-array', '[]'], ['one-array', '[1]'], ['object', '{}'],
  ['noncallable-to-string', '{"toString":1}'],
];
const rows = [];
for (const revision of [0, 1]) for (const [index, [label, inputRaw]] of inputs.entries()) {
  const name = revision === 0 ? 'StarkNetDomain' : 'StarknetDomain';
  const kind = revision === 0 ? 'felt' : 'shortstring';
  const types = {
    [name]: [{ name: 'name', type: kind }, { name: 'version', type: kind }, { name: 'chainId', type: kind }],
    Message: [{ name: 'x', type: 'felt' }],
  };
  if (revision === 1) types[name].push({ name: 'revision', type: 'shortstring' });
  const domain = { name: 'Fixture', version: '1', chainId: 'TEST' };
  if (inputRaw !== null) domain.revision = '__DOMAIN_REVISION__';
  const raw = JSON.stringify({ types, domain, primaryType: 'Message', message: { x: '17' } }).replace('"__DOMAIN_REVISION__"', inputRaw ?? 'null');
  const parsed = JSON.parse(raw);
  const accepted = revision === 0 ? [0, 1, 2].includes(index) : [3, 8, 12].includes(index);
  const row = { id: `domain-revision/${revision}/${label}`, revision, label, inputRaw, raw, address: '0x1234', accepted, hasRevision: Object.hasOwn(parsed.domain, 'revision') };
  try {
    row.hash = typedData.getMessageHash(parsed, row.address);
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', accepted, row.id);
  assert.equal(!!row.error, !accepted, row.id);
  rows.push(row);
}
assert.equal(inputs.length, 15, 'domain_revision_source_declared_input_floor');
assert.equal(rows.length, 30, 'domain_revision_source_case_floor');
assert.equal(new Set(rows.map(row => row.id)).size, 30, 'domain_revision_source_unique_inventory');
assert.equal(rows.filter(row => row.accepted).length, 6, 'domain_revision_source_accepted_floor');
assert.equal(rows.filter(row => !row.accepted).length, 24, 'domain_revision_source_refused_floor');
for (const revision of [0, 1]) assert.equal(new Set(rows.filter(row => row.revision === revision && row.accepted).map(row => row.hash)).size, 1, 'domain_revision_source_accepted_hash_identity');
process.stdout.write(JSON.stringify({ inputs, rows }));
