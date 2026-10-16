import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = new URL('../../../', import.meta.url);
const sources = {};
for (const [file, names] of [['order-history',['toNumber','toOptionalNumber']],['closed-pnl',['toNumber']]]) {
  const source = fs.readFileSync(new URL(`packages/safe-cex/src/exchanges/binance/binance.${file}.ts`, root),'utf8');
  const exports = {};
  vm.runInNewContext(ts.transpileModule(source + '\n' + names.map(n => `exports.${n} = ${n};`).join('\n'), {compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText, {
    exports, require: id => {assert.equal(id,'../../types');return {OrderSide:{},OrderStatus:{},OrderType:{},PositionSide:{}};},
  });
  for (const name of names) sources[`${file}.${name}`] = exports[name];
}
const values = [undefined,null,false,true,0,-1,1,1e-7,1e21,{},[],[1],[null],['12'],['12','3'],{toString:0},{toString:null},[{toString:0}],{valueOf:2},
  '', ' ', '\ufeff12tail', '\u008512tail', '0x10','0b11','12abc','-0','-0.0','Infinity','-Infinity','NaN','1e999','1e-999','1e','+.1tail','--1'];
for (let i=0;i<256;i++) {values.push(String.fromCharCode(i)+'12.5x', '12.5'+String.fromCharCode(i)+'x');}
for (let i=-320;i<310;i++) values.push(`-1.2345678901234567e${i}tail`);
const result = [];
for (const [name,fn] of Object.entries(sources)) for (const value of values) {
  let expected;
  try {const n=fn(value);expected=n===undefined?{absent:true}:{value:String(n),negativeZero:Object.is(n,-0)};}
  catch {expected={error:true};}
  result.push({name,input:value,expected});
}
process.stdout.write(JSON.stringify(result));
