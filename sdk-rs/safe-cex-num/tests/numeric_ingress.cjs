'use strict';
// JSON.parse is the oracle; record bits before JSON.stringify can erase -0/Infinity.
const texts = ['-0', '-0.0', '0', '0.0', '1.25', '1.5', '1e3', '1e400', '-1e400', '-1e-400', '1e-400',
  '5e-324', '-5e-324', '2.4703282292062327e-324', '-2.4703282292062327e-324',
  '9007199254740993', '18446744073709551615', '-9223372036854775808', '1.7976931348623157e308'];
let state = 0x3120a4f9;
function next() { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; return state >>> 0; }
for (let i = 0; i < 2000; i++) {
  const sign = next() & 1 ? '-' : '';
  texts.push(`${sign}${next()}.${next()}e${(next() % 1001) - 500}`);
}
const rows = texts.map(text => {
  const value = JSON.parse(text);
  const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
  return { text, bits: bytes.readBigUInt64BE().toString(16).padStart(16, '0') };
});
process.stdout.write(JSON.stringify(rows));
