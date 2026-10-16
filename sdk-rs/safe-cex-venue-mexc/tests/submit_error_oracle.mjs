import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {ts,root,load} from './source.mjs';

const filename=root+'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts';
const source=fs.readFileSync(filename,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='MexcExchange');
assert(klass,'MEXC declaration');
const initializer=name=>{
 const method=klass.members.find(n=>ts.isPropertyDeclaration(n)&&n.name.getText(ast)===name);
 assert(method?.initializer&&ts.isArrowFunction(method.initializer),name);
 return `exports.${name}=function(${method.initializer.parameters.map(n=>n.getText(ast)).join(',')}) ${method.initializer.body.getText(ast)};`;
};
const constant=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)==='MEXC_REGULAR_ORDER_THROTTLE_COOLDOWN_MS'));
assert(constant,'source cooldown constant');
const compiled=ts.transpileModule([constant.getText(ast),initializer('isNativeBatchAccessDeniedError'),initializer('noteRegularOrderSubmitError')].join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
let time=1_000,clockCalls=0;
const module={exports:{}};
vm.runInNewContext(`(function(exports){${compiled}\n})(module.exports)`,{module,toNumber:load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts').toNumber,virtualClock:{getCurrentTimeMs:()=>{clockCalls++;return time;}}});
const errors=[
 undefined,null,{}, {code:511}, {code:'511 trailing'}, {code:510}, {code:'510.0junk'},
 {code:999,response:{data:{code:511}}}, {code:511,response:{data:{code:0}}},
 {code:511,response:{data:{code:null}}}, {code:511,response:{data:{code:''}}},
 {response:{data:{message:'I DO NOT HAVE ACCESS'}}}, {response:{data:{msg:'Access to the interface denied'}}},
 {message:'Market Maker required'}, {message:'Requests are TOO FREQUENT'},
 {response:{data:{message:'',msg:'market maker'}}}, {response:{data:{message:false,msg:'requests are too frequent'}}},
 {response:{data:{message:null,msg:'requests are too frequent'}}},
 {response:{data:{}},message:'requests are too frequent'}, {response:{data:{message:['requests are too frequent']}}},
 {response:{data:{message:{},code:510}}}, {code:false,response:{data:{msg:'do not have access'}}},
 {code:0,message:{toString:0,valueOf:0}}, {code:{toString:0,valueOf:0},message:'requests are too frequent'},
 {code:510,message:{toString:0,valueOf:0}}, {code:511,response:{data:null}}, 'foo',
 {code:'5.11e2'}, {code:'0510'}, {code:[511]}, {code:'Infinity',message:'market maker'},
 {response:{data:{message:0}},message:'requests are too frequent'}, {message:{}},
];
assert.equal(errors.length,33,'independent submit error fixture floor');
const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const capture=task=>{try{return {ok:task()};}catch(e){return {error:{name:e.name,message:e.message}};}};
const cases=errors.map((error,index)=>{
 const denied=capture(()=>module.exports.isNativeBatchAccessDeniedError(error));
 const states=[0,20_000,Number.NaN,Number.NEGATIVE_INFINITY].map((initial,state)=>{
  time=state===3?Number.POSITIVE_INFINITY:1_000;
  clockCalls=0;const instance={regularOrderSubmitCooldownUntil:initial};
  const outcome=capture(()=>{module.exports.noteRegularOrderSubmitError.call(instance,error);return null;});
  return {initial:bits(initial),time:bits(time),cooldown:bits(instance.regularOrderSubmitCooldownUntil),clockCalls,outcome};
 });
 return {index,...(error!==undefined?{input:error}:{}),denied,states};
});
assert.equal(cases[32].index,32,'last submit error fixture');
process.stdout.write(JSON.stringify({cases}));
