import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/binance/',import.meta.url);
const cases=[];
for(const [variant,name,child] of [
 ['binance','BinanceMarketWebsocket',false],['binancepm','BinancePMMarketWebsocket',false],
 ['binancespot','BinancePublicWebsocket',false],['binance','BinancePublicWebsocket',true],['binancepm','BinancePMPublicWebsocket',true],
]){
 const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
 const ast=parse(`${variant}.ws-public.ts`),types=parse(`${variant}.types.ts`);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===name);
 const member=klass.members.find(n=>n.name?.getText(ast)==='connectAndSubscribe');assert.ok(member);
 const urls=types.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(types)==='BASE_WS_URL'));assert.ok(urls);
 const exports={};vm.runInNewContext(ts.transpileModule(`${urls.getText(types)}\nexports.fn=function(){return (${member.initializer.getText(ast)})();}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports});
 for(const testnet of [undefined,false,true])for(const disposed of [false,true])for(const childFails of child?[false,true]:[false]){
  let optionsReads=0;
  const trace=[],listeners=[],error=Object.assign(new RangeError('fixture market connect'),{code:'market-code'});
  const handlers={open(){},message(){},close(){}};
  const instance={isDisposed:disposed,parent:{get options(){optionsReads++;return{testnet};}},onOpen:handlers.open,onMessage:handlers.message,onClose:handlers.close,
   createWebsocket(...args){assert.equal(args.length,1,'source options remain omitted');trace.push(['create',args[0]]);return{addEventListener(event,callback){assert.equal(callback,handlers[event],'source listener identity');listeners.push(event);}};},
   marketWebsocket:{connectAndSubscribe(){trace.push(['market']);if(childFails)throw error;}}};
  let failure=null;
  try{exports.fn.call(instance);}catch(e){failure={same:e===error,name:e.name,message:e.message,code:e.code};}
  cases.push({variant,name,child,testnet:testnet??null,disposed,childFails,optionsReads,trace,listeners,error:failure});
 }
}
assert.equal(cases.length,42);
process.stdout.write(JSON.stringify(cases));
