const assert = require('node:assert/strict');
const { oracleRequire, verifyPackage } = require('./fixtures/oracle-packages.cjs');
verifyPackage('starknet');
const { typedData, hash } = oracleRequire('starknet');
const prime = (1n << 251n) + 17n * (1n << 192n) + 1n;
const values = [
  ['name', 'transfer'], ['default', '__default__'], ['l1-default', '__l1_default__'],
  ['uppercase-default', '__DEFAULT__'], ['empty-name', ''], ['decimal-name', '123'],
  ['hex', '0x123'], ['uppercase-prefix', '0X123'], ['empty-hex', '0x'],
  ['empty-uppercase-hex', '0X'], ['mixed-hex', '0xAbC'], ['invalid-hex', '0xg'],
  ['negative-hex-name', '0x-1'], ['whitespace-name', ' 0x123 '],
  ['latin-name', 'café'], ['cjk-name', '雪'], ['astral-name', '😀'],
  ['lone-surrogate', '\ud800'], ['prime-minus-one', '0x' + (prime - 1n).toString(16)],
  ['prime', '0x' + prime.toString(16)], ['prime-plus-one', '0x' + (prime + 1n).toString(16)],
  ['uppercase-prime', '0X' + prime.toString(16)],
  ['wide-300', '0x' + ((1n << 300n) + 17n).toString(16)],
  ['wide-512', '0x' + ((1n << 512n) - 1n).toString(16)],
];
const inputs = values.map(([label, value]) => ({ label, inputRaw: JSON.stringify(value) }));
const rows = [];
assert.equal(hash.getSelectorFromName('__default__'), '0x2e4c01ac72b840834c6c3146a782496a90a442ac831e5188090c1d33a7c0f50');
assert.equal(hash.getSelectorFromName('__l1_default__'), '0x1ac445d7a589d19896ced0342d3d23fc7a05f2d7feaf2cd89e84104c86b5937');
assert.equal(hash.getSelectorFromName('\ud800'), hash.getSelectorFromName('\ufffd'));
for (const revision of [0, 1]) for (const [index, [label]] of values.entries()) {
  const inputRaw = inputs[index].inputRaw;
  const domainName = revision === 0 ? 'StarkNetDomain' : 'StarknetDomain';
  const domainKind = revision === 0 ? 'felt' : 'shortstring';
  const types = {
    [domainName]: [{ name: 'name', type: domainKind }, { name: 'version', type: domainKind }, { name: 'chainId', type: domainKind }],
    Message: [{ name: 'selector', type: 'selector' }],
  };
  const domain = { name: 'Fixture', version: '1', chainId: 'TEST' };
  if (revision === 1) {
    types[domainName].push({ name: 'revision', type: 'shortstring' });
    domain.revision = '1';
  }
  const raw = JSON.stringify({ types, domain, primaryType: 'Message', message: { selector: '__SELECTOR_INPUT__' } }).replace('"__SELECTOR_INPUT__"', inputRaw);
  const parsed = JSON.parse(raw);
  const value = parsed.message.selector;
  const accepted = ![8, 9].includes(index) && !(revision === 0 && index >= 19);
  const row = { id: `selector/${revision}/${label}`, revision, label, inputRaw, raw, address: '0x1234', accepted, units: Array.from({ length: value.length }, (_, i) => value.charCodeAt(i)) };
  try {
    row.hash = typedData.getMessageHash(parsed, row.address);
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', accepted, row.id);
  assert.equal(!!row.error, !accepted, row.id);
  if (accepted) {
    const normalized = structuredClone(parsed);
    normalized.message.selector = /^0x[0-9a-f]*$/i.test(value) ? '0x' + BigInt(value).toString(16) : hash.getSelectorFromName(value);
    row.prepared = normalized.message.selector;
    row.preparedHash = typedData.getMessageHash(normalized, row.address);
    assert.equal(row.hash, row.preparedHash, row.id);
  }
  rows.push(row);
}
assert.equal(inputs.length, 24, 'selector_source_declared_input_floor');
assert.equal(rows.length, 48, 'selector_source_case_floor');
assert.equal(new Set(rows.map(row => row.id)).size, 48, 'selector_source_unique_inventory');
assert.equal(rows.filter(row => row.accepted).length, 39, 'selector_source_accepted_floor');
assert.equal(rows.filter(row => !row.accepted).length, 9, 'selector_source_refused_floor');
const coercionInputs = ['true', 'false', '17', '9007199254740993', '1e400', '[]', '["0x10"]', '[1,2]', '{}', 'null', '{"toString":1}', '{"valueOf":1}'];
const coercionRows = [];
for (const revision of [0, 1]) for (const [index, inputRaw] of coercionInputs.entries()) {
  const base = JSON.parse(rows.find(row => row.revision === revision && row.label === 'name').raw);
  base.message.selector = '__SELECTOR_COERCION__';
  const raw = JSON.stringify(base).replace('"__SELECTOR_COERCION__"', inputRaw);
  const parsed = JSON.parse(raw);
  const accepted = ![9, 10].includes(index);
  const row = { id: `selector-coercion/${revision}/${index}`, revision, inputRaw, raw, address: '0x1234', accepted };
  try {
    row.hash = typedData.getMessageHash(parsed, row.address);
    const text = String(parsed.message.selector);
    row.units = Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
    const normalized = structuredClone(parsed);
    normalized.message.selector = text;
    row.stringHash = typedData.getMessageHash(normalized, row.address);
    assert.equal(row.hash, row.stringHash, row.id);
  } catch (error) {
    row.error = { name: error.name, message: error.message };
  }
  assert.equal(typeof row.hash === 'string', accepted, row.id);
  assert.equal(!!row.error, !accepted, row.id);
  coercionRows.push(row);
}
assert.equal(coercionRows.length, 24, 'selector_coercion_source_floor');
assert.equal(coercionRows.filter(row => row.accepted).length, 20);
assert.equal(coercionRows.filter(row => !row.accepted).length, 4);
process.stdout.write(JSON.stringify({ inputs, rows, coercionInputs, coercionRows }));
