const assert = require('node:assert/strict');
const numbers = [NaN, Infinity, -Infinity, -0, 0, 1, -1, 1.5, -1.5, Number.MIN_VALUE, -Number.MIN_VALUE, Number.MAX_VALUE, -Number.MAX_VALUE, 9007199254740990, 9007199254740991, 9007199254740992, -9007199254740990, -9007199254740991, -9007199254740992];
for (let exponent = 0; exponent <= 54; exponent++) {
  numbers.push(2 ** exponent, -(2 ** exponent), 2 ** exponent + 0.5, -(2 ** exponent + 0.5));
}
const rows = numbers.map(value => {
  const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
  return { bits: bytes.toString('hex'), valid: Number.isSafeInteger(value) };
});
let calls = 0;
const poison = { valueOf() { calls++; throw Error('must not coerce'); }, toString() { calls++; throw Error('must not coerce'); } };
const kinds = [['undefined', undefined], ['null', null], ['false', false], ['true', true], ['string', '1'], ['empty_string', ''], ['array', [1]], ['object', {}], ['poison', poison], ['function', () => 1], ['bigint', 1n], ['symbol', Symbol.iterator]];
const types = [{ kind: 'omitted', valid: Number.isSafeInteger() }, ...kinds.map(([kind, value]) => ({ kind, valid: Number.isSafeInteger(value) }))];
assert.equal(rows.length, 239);
assert.equal(types.length, 13);
assert.equal(calls, 0);
process.stdout.write(JSON.stringify({ rows, types, calls }));
