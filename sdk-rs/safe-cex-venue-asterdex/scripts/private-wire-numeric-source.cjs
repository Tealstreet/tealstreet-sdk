// Erased TS record contracts and actual JSON.parse number bits, not a TS decoder.
const assert = require('node:assert/strict');
const fixtures = require('../tests/fixtures/private-wire-numeric.json');
const { schemas, accepts } = require('./oracle.cjs');
assert.equal(Object.keys(fixtures).length, 6);
let cases = 0;
for (const [owner, fixture] of Object.entries(fixtures)) {
  assert.ok(schemas[owner], `actual source schema ${owner}`);
  const template = JSON.stringify(fixture.input);
  assert.equal(template.split('"__number__"').length, 2);
  for (const [literal, bits] of [
    ['1', '3ff0000000000000'], ['1.25', '3ff4000000000000'], ['-0', '8000000000000000'],
  ]) {
    const parsed = JSON.parse(template.replace('"__number__"', literal));
    assert.ok(accepts(schemas[owner], parsed), `${owner}: valid source record ${literal}`);
    const bytes = Buffer.alloc(8);
    bytes.writeDoubleBE(parsed[fixture.wireNumber]);
    assert.equal(bytes.toString('hex'), bits, `${owner}: JSON.parse native bits ${literal}`);
    cases++;
  }
}
console.log(`Aster private-wire fixtures: six source record schemas / ${cases} JSON.parse bit witnesses pass (Node ${process.versions.node}, ${process.arch}).`);
