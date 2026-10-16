import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import ts from 'typescript';
import vm from 'node:vm';
import camelCase from 'lodash/camelCase.js';
import snakeCase from 'lodash/snakeCase.js';
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
const context=vm.createContext({camelCase,snakeCase,memoize:fn=>fn});
for(const file of ['utils/get-key.ts','exchanges/bybit/bybit.fill-event.ts']) {
 const source=ts.createSourceFile(file,readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
 const declarations=source.statements.filter(s=>!ts.isImportDeclaration(s));
 const text=declarations.map(s=>s.getText(source).replace(/^export /,'')).join('\n');
 vm.runInContext(ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText,context);
}
const resolve=vm.runInContext('resolveBybitExecution',context);
const values=[undefined,null,'',0,-0,1,1e21,'-0','0','12abc','0x10','0b11','12e+','12e+3','Infinity','-Infinity','NaN','\uFEFF12x','\u008512x',true,false,[],[12],['12x'],{}, {toString:null}];
const cases=[];
const add=input=>{const order=JSON.parse(JSON.stringify(input));let result,error;try{result=resolve(order)}catch(e){error=e.message}cases.push({order,result,error,negativePrice:result?Object.is(result.price,-0):false,negativeAmount:result?Object.is(result.amount,-0):false});};
for(const price of values) for(const qty of values) {
 add({avgPrice:price,cumExecQty:qty,price:'0.057',qty:'3'});
 add({avg_price:price,cum_exec_qty:qty,price:'0.057',qty:'3'});
}
for(const value of values) {
 add({avgPrice:value,avg_price:'88',cumExecQty:value,cum_exec_qty:'99',price:{toString:null},qty:{toString:null}});
 add({price:value,qty:value});
}
add(null);add({});
assert.equal(resolve({avgPrice:'0.0519',price:'0.057',cumExecQty:'1',qty:'3'}).price,0.0519);
assert.equal(resolve({avgPrice:0,price:{toString:null},cumExecQty:0,qty:{toString:null}}).amount,0);
process.stdout.write(JSON.stringify(cases));
