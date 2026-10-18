const assert = require('node:assert/strict');

const method = process.argv[2];
assert.ok(['toUpperCase', 'toLowerCase', 'trim'].includes(method));
const inputs = [
  '', 'abc-XYZ-123', 'Straße ß ﬃ', 'İ I ı i', 'ΟΣ Σ ΟΣΑ ΟΣ\u0301',
  '\u{10428}\u{10400}😀',
  ' \t\r\n\u00a0\u1680\u2000\u2028\u2029\u202f\u205f\u3000\ufeff',
  '\u0085\u180e\u200b', '\ud800\ud800\udc00\udc00', '\udfff\ud800',
  'Σ\ud800Ο', 'ΟΣ\udc00Α', '\ufeff\ud800x\udc00\u00a0',
];
for (let unit = 0xd800; unit <= 0xdfff; unit++) {
  const surrogate = String.fromCharCode(unit);
  inputs.push(surrogate, ` \tΟΣ${surrogate}Aß\ufeff`);
}
assert.equal(inputs.length, 4109);
const units = value => Array.from({length: value.length}, (_, i) => value.charCodeAt(i));
process.stdout.write(JSON.stringify(inputs.map(input => ({
  input: units(input), expected: units(input[method]()),
}))));
