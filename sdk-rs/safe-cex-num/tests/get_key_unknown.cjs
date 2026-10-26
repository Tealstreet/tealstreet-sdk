const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const {bits} = require('./support/number-bits.cjs');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler && path.isAbsolute(compiler), 'absolute isolated TypeScript install required');
const ts = require(compiler);
const lodash = require(path.resolve(compiler, '../lodash/lodash.js'));
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/utils/get-key.ts');
let source = fs.readFileSync(file, 'utf8').replace(/^import .*;\r?\n/gm, '');
if (process.env.GET_KEY_UNKNOWN_CONTROL === 'primitive-missing') {
  const wrong = source.replace('if (obj == null)', 'if (obj == null || typeof obj !== "object")');
  assert.notEqual(wrong, source, 'primitive guard control matched'); source = wrong;
}
const context = {exports:{}, camelCase:lodash.camelCase, snakeCase:lodash.snakeCase, memoize:lodash.memoize};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,context);
const arrayNames = Object.getOwnPropertyNames(Array.prototype);
const objectNames = Object.getOwnPropertyNames(Object.prototype);
assert.equal(arrayNames.length,40,'measured stock Array.prototype surface');
assert.equal(objectNames.length,12,'measured stock Object.prototype surface');
function encode(value) {
  if (value === undefined) return {kind:'undefined'};
  if (value === Array.prototype) return {kind:'array-prototype',truthy:true};
  if (value === Object.prototype) return {kind:'object-inherited',text:String(value),truthy:true};
  for (const name of arrayNames) if (typeof Array.prototype[name] === 'function' && value === Array.prototype[name]) return {kind:'array-inherited',name,text:String(value),truthy:true};
  for (const name of objectNames) if (typeof Object.prototype[name] === 'function' && value === Object.prototype[name]) return {kind:'object-inherited',text:String(value),truthy:true};
  return typeof value === 'number' ? {kind:'number',bits:bits(value),truthy:Boolean(value)} : {kind:'json',value,truthy:Boolean(value)};
}
function measure(fn) { try {return {ok:encode(fn())};} catch(e) {return {error:{name:e.name,message:e.message}};} }
const rawValues = [null,'null','0','-0','1','1.25','1e400','-1e400','false','true','""','"plain"',JSON.stringify('😀\u0000'),'[]','[null,7,-0]','{}','{"first":null,"second":9}','{"trade_id":7,"tradeId":null}','{"tradeId":1,"trade_id":2}','{"constructor":0,"__proto__":false}','{"length":7,"0":11}'];
const keys = ['', '0','1','2','3','00','+0','-0','0.0',' 0','4294967294','4294967295','tradeId','trade_id','first','second','fallback',"x'\n😀",...arrayNames,...objectNames];
const rows=[];
for (const raw of rawValues) {
 const object=raw===null?undefined:JSON.parse(raw);
 for (const key of keys) {
   rows.push({raw,keys:[key],mode:'v',expected:measure(()=>context.exports.v(object,key))});
   rows.push({raw,keys:[key,'second','length'],mode:'vn',expected:measure(()=>context.exports.vn(object,key,'second','length'))});
 }
 rows.push({raw,keys:[],mode:'vn',expected:measure(()=>context.exports.vn(object))});
}
assert.equal(context.exports.vn({first:null,second:9},'first','second'),null,'own null stops alias search');
assert.equal(context.exports.v({tradeId:undefined,trade_id:9},'tradeId'),undefined,'own undefined stops spelling search');
assert.equal(context.exports.vn({tradeId:undefined,second:9},'tradeId','second'),9,'own undefined advances alias search');
const refusals = ['"\\ud800"','"x\\udfff"'].map(raw => {
  const scalar = JSON.parse(raw);
  const keyError = measure(() => context.exports.v(0,scalar));
  const rhsError = measure(() => context.exports.v(scalar,'x'));
  return {raw,keyError:JSON.stringify(keyError),rhsError:JSON.stringify(rhsError)};
});
process.stdout.write(JSON.stringify({arrayNames,objectNames,rows,refusals}));
