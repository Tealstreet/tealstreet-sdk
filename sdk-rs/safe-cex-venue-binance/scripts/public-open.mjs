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
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn=function(...args){return (${member.initializer.getText(ast)})(...args);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,WebSocket:{OPEN:1}});return exports.fn;
}
const reset=load('base.ws.ts','BaseWebSocket','resetOrderbook'),send=load('base.ws.ts','BaseWebSocket','send');
const variants=[['binance','BinanceMarketWebsocket','market'],['binance','BinancePublicWebsocket','public'],['binancepm','BinancePMMarketWebsocket','pm-market'],['binancepm','BinancePMPublicWebsocket','pm-public'],['binancespot','BinancePublicWebsocket','spot']];
const snapshots=[{}, {BTC:{updates:[{u:1}],isSnapshotLoaded:true}}, {BTC:{updates:[{u:1},{u:2}],isSnapshotLoaded:true},ETH:{updates:[{u:3}],isSnapshotLoaded:false}}];
const copy=x=>JSON.parse(JSON.stringify(x)),cases=[];
for(const [file,klass,variant] of variants){
 const onOpen=load(`binance/${file}.ws-public.ts`,klass,'onOpen');
 for(const disposed of [false,true])for(const publicOnly of [false,true])for(const failSend of [false,true])for(const disposeOnSend of [false,true])for(const failSubscribe of [false,true])for(const initial of snapshots){
  const trace=[],parent={isDisposed:disposed,options:{publicDataOnly:publicOnly}},books=copy(initial);let error=null;
  const instance={parent,get isDisposed(){return parent.isDisposed;},orderBookInnerState:books,send,resetOrderbook:reset,subscribe(){trace.push({kind:'subscribe',disposed:parent.isDisposed,books:copy(books)});if(failSubscribe)throw Error('fixture subscribe');},ws:{readyState:1,send(frame){trace.push({kind:'send',frame,books:copy(books)});if(disposeOnSend)parent.isDisposed=true;if(failSend)throw Error('fixture send');}}};
  try{onOpen.call(instance);}catch(e){error={name:e.name,message:e.message};}
  cases.push({variant,disposed,publicOnly,failSend,disposeOnSend,failSubscribe,initial,trace,books,error});
 }
}
assert.equal(cases.length,480);
process.stdout.write(JSON.stringify(cases));
