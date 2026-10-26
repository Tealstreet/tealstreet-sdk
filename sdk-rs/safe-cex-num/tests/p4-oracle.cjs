const fs = require('node:fs');
const path = require('node:path');
const {stripTypeScriptTypes} = require('node:module');
const root = path.resolve(__dirname, '../../..');
function load(file, names, injected = {}) {
  const source = stripTypeScriptTypes(fs.readFileSync(path.join(root, 'packages/safe-cex/src/utils', file), 'utf8').replace(/^import .*;$/mg, '').replace(/export /g, ''));
  return new Function(...Object.keys(injected), source + '\nreturn {' + names.join(',') + '};')(...Object.values(injected));
}
const math = load('safe-math.ts', ['multiply1','divide1','handleRoundingError','round'], {isNotNumber: v => Number.isNaN(v)});
const ccxt = load('ccxt_helpers/precision.ts', ['integerPrecisionToAmount','countDecimals','decimalToPrecision','parsePrecision','precisionToString']);
const {parsePrecision} = load('parsePrecision.ts', ['parsePrecision']);
const {roundUSD} = load('roundUSD.ts', ['roundUSD']);
const encode = x => Number.isNaN(x) ? 'NaN' : Object.is(x, -0) ? '-0' : String(x);
const cases = [];
let seed = 4817;
const values = [0,-0,NaN,Infinity,-Infinity,1.005,2.675,-1.25,5e-324,1e21,1e-22,999.999,10.123456,1.23456];
for(let i=0;i<600;i++) { seed=(Math.imul(seed,1664525)+1013904223)>>>0; values.push((seed/2**32-0.5)*10**(i%30-12)); }
for(const op of ['handleRoundingError','round']) for(const a of values) cases.push({op,a:encode(a),expected:encode(math[op](a))});
for(const a of values) { cases.push({op:'roundUSD',a:encode(a),expected:encode(roundUSD(a))}); for(const b of [0,-0,0.01,3,-1,Infinity,NaN]) for(const op of ['multiply1','divide1']) cases.push({op,a:encode(a),b:encode(b),expected:encode(math[op](a,b))}); }
for(const input of [undefined,'','0','-0','1','0x10','0b10',' 10000xyz','0.0001000','1e-8','1e-0x10','1E-2e-3','\uFEFF1.0junk','\u00851.0junk','2.5E-7','1e','1junk','1.0junk','1e2e3','9007199254740993']) {
  for(const op of ['integerPrecisionToAmount','countDecimals','decimalToPrecision']) cases.push({op,input:input??null,expected:encode(ccxt[op](input))});
  for(const integer of [false,true]) cases.push({op:'ccxtParse',input:input??null,integer,expected:encode(ccxt.parsePrecision(input,integer))});
  try { cases.push({op:'parsePrecision',input:input??null,expected:parsePrecision(input)??null}); } catch(error) { cases.push({op:'parsePrecision',input:input??null,error:error.message}); }
}
for(const a of [0,1,2,8,-1,2.5,NaN]) cases.push({op:'precisionToString',a:encode(a),expected:ccxt.precisionToString(a)});
process.stdout.write(JSON.stringify(cases));
