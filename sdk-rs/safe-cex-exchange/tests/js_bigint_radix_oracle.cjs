'use strict';
// Actual engine primitive, one process for the complete native-driven corpus.
const cases = JSON.parse(require('node:fs').readFileSync(0, 'utf8'));
if (cases.length !== 472) throw new Error('BigInt radix source census');
function radix(spec) {
  switch (spec.kind) {
    case 'undefined': return undefined;
    case 'null': return null;
    case 'bool': return spec.value;
    case 'number': return Number(spec.value);
    case 'string': return spec.value;
    case 'bigint': return BigInt(spec.value);
    case 'array': return spec.value;
    default: throw new Error('unknown radix descriptor');
  }
}
const rows = cases.map(test => {
  const value = BigInt(test.value);
  try {
    return {ok: test.radix.kind === 'omitted'
      ? value.toString() : value.toString(radix(test.radix))};
  } catch (error) { return {error: error.name}; }
});
process.stdout.write(JSON.stringify({node: process.version, arch: process.arch, rows}));
