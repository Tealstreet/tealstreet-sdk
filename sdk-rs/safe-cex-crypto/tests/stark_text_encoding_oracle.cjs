const assert = require('node:assert/strict');
const { oracleRequire, verifyPackage } = require('./fixtures/oracle-packages.cjs');
verifyPackage('starknet');
const { typedData, byteArray, hash } = oracleRequire('starknet');
const pairs = [[0, 'felt'], [0, 'shortstring'], [0, 'string'], [0, 'bool'], [1, 'felt'], [1, 'shortstring'], [1, 'string']];
const baseValues = [['empty', ''], ['numeric', '17'], ['tab-only', '\t'], ['nul', 'A\0B'], ['tab', 'A\tB'], ['one', 'A\u0001B'], ['lf-middle', 'A\nB'], ['cr-middle', 'A\rB'], ['lf-tail', 'AB\n'], ['cr-tail', 'AB\r'], ['ascii', 'Hello'], ['latin', 'café'], ['cjk', '雪'], ['astral', '😀']];
const baseInputs = baseValues.map(([label, value]) => ({ label, inputRaw: JSON.stringify(value) }));
const first = '0123456789ABCDEFGHIJKLMNOPQRSTU';
const second = 'zyxwvutsrqponmlkjihgfedcba98765';
assert.equal(first.length, 31);
assert.equal(second.length, 31);
const edgeValues = [
  [1, 'string', 'edge31', first, true], [1, 'string', 'edge32', first + 'v', true],
  [1, 'string', 'edge62', first + second, true], [1, 'string', 'edge63', first + second + '!', true],
  [1, 'string', 'nul31', 'A'.repeat(29) + '\0Z', true],
  [1, 'string', 'tab32', 'B'.repeat(30) + '\tQ', true],
  [1, 'string', 'lf31', 'C'.repeat(30) + '\n', true],
  [1, 'string', 'lf32', 'D'.repeat(31) + '\n', false],
  [0, 'felt', 'edge31', first, true], [0, 'felt', 'edge32', first + 'v', false],
  [1, 'felt', 'edge31', first, true], [1, 'felt', 'edge32', first + 'v', false],
];
const edgeInputs = edgeValues.map(([revision, kind, label, value, accepted]) => ({ revision, kind, label, inputRaw: JSON.stringify(value), accepted }));
const rows = [];

function capture(revision, kind, label, inputRaw, accepted) {
  const domainName = revision === 0 ? 'StarkNetDomain' : 'StarknetDomain';
  const domainKind = revision === 0 ? 'felt' : 'shortstring';
  const types = {
    [domainName]: [{ name: 'name', type: domainKind }, { name: 'version', type: domainKind }, { name: 'chainId', type: domainKind }],
    Message: [{ name: 'x', type: kind }],
  };
  const domain = { name: 'Fixture', version: '1', chainId: 'TEST' };
  if (revision === 1) {
    types[domainName].push({ name: 'revision', type: 'shortstring' });
    domain.revision = '1';
  }
  const raw = JSON.stringify({ types, domain, primaryType: 'Message', message: { x: '__TEXT_INPUT__' } }).replace('"__TEXT_INPUT__"', inputRaw);
  const parsed = JSON.parse(raw);
  const value = parsed.message.x;
  const row = { id: `text/${revision}/${kind}/${label}`, revision, kind, label, inputRaw, raw, address: '0x1234', accepted, units: Array.from({ length: value.length }, (_, index) => value.charCodeAt(index)) };
  try {
    row.hash = typedData.getMessageHash(parsed, row.address);
    row.encoded = typedData.encodeValue(types, kind, value, { parent: 'Message', key: 'x' }, String(revision))[1];
    if (revision === 1 && kind === 'string') {
      const encoded = byteArray.byteArrayFromString(value);
      row.byteWords = [encoded.data.length, ...encoded.data, encoded.pending_word, encoded.pending_word_len].map(word => BigInt(word).toString());
      assert.equal(BigInt(hash.computePoseidonHashOnElements(row.byteWords)), BigInt(row.encoded), row.id);
    }
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', accepted, row.id);
  assert.equal(!!row.error, !accepted, row.id);
  rows.push(row);
}

for (const [revision, kind] of pairs) for (const [index, { label, inputRaw }] of baseInputs.entries()) {
  capture(revision, kind, label, inputRaw, ![6, 7, 11, 12, 13].includes(index));
}
for (const { revision, kind, label, inputRaw, accepted } of edgeInputs) capture(revision, kind, label, inputRaw, accepted);
assert.equal(baseInputs.length, 14);
assert.equal(edgeInputs.length, 12);
assert.equal(rows.length, 110, 'text_source_case_floor');
assert.equal(new Set(rows.map(row => row.id)).size, 110, 'text_source_unique_inventory');
assert.equal(rows.filter(row => row.accepted).length, 72, 'text_source_accepted_floor');
assert.equal(rows.filter(row => !row.accepted).length, 38, 'text_source_refused_floor');
process.stdout.write(JSON.stringify({ pairs, baseInputs, edgeInputs, rows }));
