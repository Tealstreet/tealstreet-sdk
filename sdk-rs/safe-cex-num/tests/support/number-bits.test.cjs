const {test} = require('node:test');
const assert = require('node:assert/strict');
const {bits} = require('./number-bits.cjs');
test('NaN capture ignores only sign and payload', () => {
  for (const hex of ['7ff8000000000000','fff8000000000000','7ff0000000000001','fff8000000001234']) {
    assert.equal(bits(Buffer.from(hex,'hex').readDoubleBE()), '7ff8000000000000', 'NAN CAPTURE canonical payload');
  }
});
test('signed zero remains bit-exact at capture', () => {
  assert.equal(bits(-0), '8000000000000000', 'SIGNED ZERO CAPTURE identity');
  assert.equal(bits(0), '0000000000000000', 'POSITIVE ZERO CAPTURE identity');
});
test('finite non-NaN payload remains bit-exact at capture', () => {
  for (const hex of ['3ff0000000000001','bff0000000000001','0000000000000001','7fefffffffffffff','7ff0000000000000','fff0000000000000']) {
    assert.equal(bits(Buffer.from(hex,'hex').readDoubleBE()), hex, 'FINITE CAPTURE payload identity');
  }
});
