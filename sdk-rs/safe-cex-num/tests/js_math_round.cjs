const buffer = Buffer.alloc(8);
const {bits} = require('./support/number-bits.cjs');
function value(n) {buffer.writeBigUInt64BE(BigInt.asUintN(64,n));return buffer.readDoubleBE();}
const inputs = [NaN, Infinity, -Infinity, -0, 0, -0.5, -1.5, 0.49999999999999994, 4503599627370495.5, Number.MAX_VALUE];
for (let n = -1000; n <= 1000; n++) {
  const half = n + 0.5;
  const raw = BigInt('0x'+bits(half));
  for (let ulp=-2n; ulp<=2n; ulp++) inputs.push(value(raw+ulp));
}
let state = 42n;
for (let i=0;i<10000;i++) {state=BigInt.asUintN(64,state*6364136223846793005n+1442695040888963407n);inputs.push(value(state));}
// BOM is included at the numeric-input boundary, where JS removes it.
inputs.push(Number('\ufeff-0.5\ufeff'));
process.stdout.write(JSON.stringify(inputs.map(x=>[bits(x),bits(Math.round(x))])));
