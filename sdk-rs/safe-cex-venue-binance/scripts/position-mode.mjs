import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/binance/',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const cases=[];
const values=[undefined,null,false,true,0,1,'','true',[],{},[true],{dualSidePosition:true},{dualSidePosition:false},...['true','false',1,0,null,[],{},[true]].map(value=>({dualSidePosition:value}))];
for(const [variant,klass,client] of [['binance','BinanceExchange','xhr'],['binancepm','BinancePMExchange','papiXhr']]){
 const ast=parse(`${variant}.exchange.ts`),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);assert.ok(decl);
 const method=decl.members.find(n=>n.name?.getText(ast)==='fetchPositionMode');assert.ok(method);
 const types=parse(`${variant}.types.ts`),endpoints=types.statements.flatMap(n=>ts.isVariableStatement(n)?[...n.declarationList.declarations]:[]).find(n=>n.name.text==='ENDPOINTS');assert.ok(endpoints);
 const exports={};vm.runInNewContext(ts.transpileModule(`const ENDPOINTS=${endpoints.initializer.getText(types)}; exports.fn=function(){return (${method.initializer.getText(ast)})();}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
 for(let index=0;index<values.length;index++)for(const reject of [false,true]){
  const trace=[],data=values[index],error=Object.assign(RangeError('fixture request'),{code:'fixture-code',response:{status:503,data:{msg:'venue message'}}});
  let result=null,failure=null;
  const instance=Object.fromEntries(['xhr','papiXhr','fapiXhr','publicXhr'].map(name=>[name,{get:async(...args)=>{trace.push({client:name,args});if(reject)throw error;return {data};}}]));
  try{result=await exports.fn.call(instance);}catch(e){failure={name:e.name,message:e.message,code:e.code??null,same:e===error,response:e.response??null};}
  assert.equal(trace.length,1);assert.equal(trace[0].client,client);
  cases.push({variant,index,reject,...(data===undefined?{}:{data}),trace,result,error:failure});
 }
}
assert.equal(cases.length,84);
process.stdout.write(JSON.stringify(cases));
