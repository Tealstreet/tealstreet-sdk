import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/',import.meta.url);
function load(file,klass,method){
 const ast=ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
 const declaration=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);assert.ok(declaration,klass);
 const member=declaration.members.find(n=>n.name?.getText(ast)===method);assert.ok(member,method);
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn=function(...args){return (${member.initializer.getText(ast)})(...args);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,WebSocket:{OPEN:1},performance:{now:()=>100}});return exports.fn;
}
const send=load('base.ws.ts','BaseWebSocket','send'),sendWithLog=load('base.ws.ts','BaseWebSocket','sendWithLog'),cases=[];
for(const [variant,klass] of [['binance','BinancePrivateWebsocket'],['binancepm','BinancePMPrivateWebsocket']]){
 const file=`binance/${variant}.ws-private.ts`,onOpen=load(file,klass,'onOpen'),ping=load(file,klass,'ping');
 for(const disposed of [false,true])for(const resync of [false,true])for(const opened of [false,true])for(const failSend of [false,true])for(const disposeOnSend of [false,true])for(const failFetch of [false,true])for(const sendMutation of ['none','resync','opened','clear'])for(const fetchMutation of [false,true]){
  const trace=[],parent={isDisposed:disposed};let error=null;
  const flags=()=>({resync:instance.resyncOrdersOnOpen,opened:instance.hasOpened});
  const instance={parent,get isDisposed(){return parent.isDisposed;},resyncOrdersOnOpen:resync,hasOpened:opened,ping,send,sendWithLog,pingAt:25,emitWsDiagnosticSend(){},throttledFetchOrders(){trace.push({kind:'fetch',disposed:parent.isDisposed,...flags()});if(fetchMutation){this.resyncOrdersOnOpen=true;this.hasOpened=false;}if(failFetch)throw Object.assign(TypeError('fixture fetch'),{code:'fetch-code'});},ws:{readyState:1,send(frame){trace.push({kind:'send',frame,...flags()});if(disposeOnSend)parent.isDisposed=true;if(sendMutation==='resync')instance.resyncOrdersOnOpen=true;if(sendMutation==='opened')instance.hasOpened=true;if(sendMutation==='clear'){instance.resyncOrdersOnOpen=false;instance.hasOpened=false;}if(failSend)throw Object.assign(RangeError('fixture send'),{code:'send-code'});}}};
  try{onOpen.call(instance);}catch(e){error={name:e.name,message:e.message,code:e.code};}
  cases.push({variant,disposed,resync,opened,failSend,disposeOnSend,failFetch,sendMutation,fetchMutation,trace,flags:flags(),error});
 }
}
assert.equal(cases.length,1024);
process.stdout.write(JSON.stringify(cases));
