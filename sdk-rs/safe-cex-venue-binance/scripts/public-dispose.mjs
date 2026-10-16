import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/',import.meta.url);
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const base=parse('base.ws.ts');
const baseClass=base.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseWebSocket');
const baseDispose=baseClass.members.find(n=>n.name?.getText(base)==='dispose');
assert.ok(baseDispose);
const cases=[];
for(const [variant,name] of [['binance','BinancePublicWebsocket'],['binancepm','BinancePMPublicWebsocket']]){
 const ast=parse(`binance/${variant}.ws-public.ts`);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===name);
 const member=klass.members.find(n=>n.name?.getText(ast)==='dispose');assert.ok(member);
 const exports={};
 vm.runInNewContext(ts.transpileModule(`class Base {${baseDispose.getText(base)}}\nexports.Klass=class extends Base {${member.getText(ast)}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,clearTimeout(){throw Error('unexpected timer');}});
 for(const disposed of [false,true])for(const childFails of [false,true])for(const connected of [false,true]){
  const trace=[],instance=new exports.Klass(),error=Object.assign(new RangeError('fixture market disposal'),{code:'dispose-code'});
  Object.assign(instance,{isDisposed:disposed,forceClosed:false,marketWebsocket:{dispose(){trace.push('market');if(childFails)throw error;}},ws:connected?{terminate(){trace.push('terminate');},close(){trace.push('close');}}:undefined});
  const attempts=[];
  for(let i=0;i<2;i++){
   let failure=null;
   try{instance.dispose();}catch(e){failure={same:e===error,name:e.name,message:e.message,code:e.code};}
   attempts.push({trace:[...trace],forceClosed:instance.forceClosed,hasSocket:instance.ws!==undefined,error:failure});
  }
  cases.push({variant,disposed,childFails,connected,attempts});
 }
}
assert.equal(cases.length,16);
process.stdout.write(JSON.stringify(cases));
