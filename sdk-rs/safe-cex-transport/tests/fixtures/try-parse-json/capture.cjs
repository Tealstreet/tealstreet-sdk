const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const load = require('../../../scripts/ts-source.cjs');
const root = path.resolve(__dirname, '../../../../..');
const parse = load(path.join(root, 'packages/safe-cex/src/utils/tryParseJson.ts')).tryParseJson;
const numbers = ['0', '-0', '-0.0', '-0e10', '1.50', '1e0', '1.25', '9007199254740993', '-9007199254740993', '1e20', '1e21', '1e-6', '1e-7', '5e-324', '1e-400', '-1e-400', '1e400', '-1e400'];
const inputs = numbers.flatMap(n => ['{"n":'+n+'}', '[{"n":'+n+'}]', '{"nested":{"n":'+n+'}}']);
inputs.push('null', '1.50', '"str"', 'false', 'not json', '\ufeff{"n":1}', '{"n":1}\ufeff', '{"2":2,"1":1,"b":3,"a":4}');
function observe(value) {
 if (typeof value === 'number') {assert(!Number.isNaN(value),'closed JSON corpus must not acquire NaN payloads');const b=Buffer.alloc(8);b.writeDoubleBE(value);return {kind:'number',bits:b.toString('hex'),string:String(value),truthy:Boolean(value),finite:Number.isFinite(value)};}
 if (Array.isArray(value)) return {kind:'array',children:value.map(observe)};
 if (value!==null && typeof value==='object') return {kind:'object',keys:Object.keys(value),children:Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]))};
 return {kind:typeof value,value};
}
const cases=inputs.map(text=>({raw:JSON.stringify(text),observed:observe(parse(text))}));
// Existing objects are returned by identity, not normalized or copied.
const obj={n:-0};assert.equal(parse(obj),obj,'source existing-object identity');
const target=path.join(__dirname,'vectors.json');
const result=JSON.stringify({source:'actual tryParseJson.ts; numeric observations precede JSON serialization',cases},null,2)+'\n';
if(process.argv.includes('--capture'))fs.writeFileSync(target,result,{flag:'wx'});
else assert.equal(result,fs.readFileSync(target,'utf8'),'tryParseJson oracle drift');
console.error(`tryParseJson capture verification: ${process.platform}/${process.arch}, ${process.version}; NaN unreachable, all other bits exact`);
