const fs = require('node:fs');
const ts = require('node:module').stripTypeScriptTypes;
const root = require('node:path').resolve(__dirname, '../../..');
function load(path, names) {
  let source = fs.readFileSync(root + '/' + path, 'utf8').replace(/^import .*;$/mg, '').replace(/export /g, '');
  source = ts(source);
  return new Function(source + '\nreturn {' + names.join(',') + '};')();
}
const math = load('packages/safe-cex/src/utils/safe-math.ts', ['afterDecimal','adjust','add','subtract','multiply','divide']);
const encode = x => Number.isNaN(x) ? 'NaN' : x === Infinity ? 'Infinity' : x === -Infinity ? '-Infinity' : Object.is(x, -0) ? '-0' : String(x);
const values = [0, -0, 0.1, 0.2, -0.5, 1, -1, 3, 1.005, 2.675, 0.0002, 2.5e-7, 1e-20, 5e-324, 1e21, 1.0000000000000002, 4503599627370495.5, NaN, Infinity, -Infinity];
for (let n=1; n<=324; n++) values.push(Number(`1e-${n}`));
let cases = [];
for (const op of ['add','subtract','multiply','divide']) for (const a of values) for (const b of values) {
  cases.push({op, a:encode(a), b:encode(b), expected:encode(math[op](a,b))});
}
for (const step of [0.01, 0.1, 0, -0.01, '0.0100', '2.5e-7', '0x10', '', 'nope']) for (const a of values) {
 cases.push({op:'adjust',a:encode(a),step,expected:encode(math.adjust(a,step))});
}
for (const x of ['1.2300','2.5E-7','2.5e+3','1e','1eNaN','1e2e3','']) cases.push({op:'afterDecimal',x,expected:encode(math.afterDecimal(x))});
for (const precision of [0,2,9,-1,2.5,NaN,Infinity]) cases.push({op:'divide',a:'-1',b:'2',precision:encode(precision),expected:encode(math.divide(-1,2,precision))});
cases.push({op:'adjust',a:'4.0706880089046605e23',step:'0x56333e7122f01abec371',expected:encode(math.adjust(4.0706880089046605e23,'0x56333e7122f01abec371'))});
for (const x of ['NaN','Infinity','-Infinity','1e-1.5','1e0x2']) cases.push({op:'afterDecimal',x,expected:encode(math.afterDecimal(x))});
for (const x of values) cases.push({op:'afterDecimalNumber',a:encode(x),expected:encode(math.afterDecimal(x))});
process.stdout.write(JSON.stringify(cases));
