import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/exchanges/',import.meta.url);
const protocolFile='binance/binancespot.ws-private.ts';
const protocolAst=ts.createSourceFile(protocolFile,fs.readFileSync(new URL(protocolFile,root),'utf8'),ts.ScriptTarget.Latest,true);
const idDeclaration=protocolAst.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.getText(protocolAst)==='BINANCE_SPOT_USER_DATA_REQUEST_ID');
assert.ok(idDeclaration);
const requestId=vm.runInNewContext(idDeclaration.initializer.getText(protocolAst));

function load(file,klass,name){
 const ast=ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
 const c=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);assert.ok(c,klass);
 const m=c.members.find(n=>n.name?.getText(ast)===name);assert.ok(m,name);
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.fn=function(...args){return (${m.initializer.getText(ast)})(...args);}`,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,WebSocket:{OPEN:1},BINANCE_SPOT_USER_DATA_REQUEST_ID:requestId});return exports.fn;
}
const onOpen=load('binance/binancespot.ws-private.ts','BinancePrivateWebsocket','onOpen');
const send=load('base.ws.ts','BaseWebSocket','send'),sendWithLog=load('base.ws.ts','BaseWebSocket','sendWithLog'),forceClose=load('base.ws.ts','BaseWebSocket','forceClose');
const paramsCases={object:{apiKey:'key',recvWindow:5000,timestamp:100,signature:'sig'},undefined:undefined,null:null,array:[1,null,'💠'],text:'value',nan:{timestamp:NaN},infinity:{timestamp:Infinity},bigint:{signature:10n}};
const nums={zero:0,large:2**53,nan:NaN,infinity:Infinity};
const cases=[];
for(const disposed of [false,true])for(const action of ['none','dispose','advance','reset'])for(const outcome of [...Object.keys(paramsCases),'failure','blankFailure'])for(const failSend of [false,true])for(const [initial,n] of Object.entries(nums)){
 const trace=[],parent={isDisposed:disposed};let settle;
 const instance={parent,get isDisposed(){return parent.isDisposed;},subscriptionAttempt:n,forceClosed:false,send,sendWithLog,forceClose,emitWsDiagnosticSend(){},emitAndLogError(message){trace.push({kind:'error',message});},buildUserDataSubscriptionParams(){trace.push({kind:'build'});return new Promise((resolve,reject)=>{settle=()=>outcome.endsWith('Failure')||outcome==='failure'?reject(Error(outcome==='blankFailure'?'':'fixture signer')):resolve(paramsCases[outcome]);});},ws:{readyState:1,send(frame){trace.push({kind:'send',frame});if(failSend)throw Error('fixture send');},close(){trace.push({kind:'close'});}}};
 const pending=onOpen.call(instance);
 const before={trace:[...trace],attempt:String(instance.subscriptionAttempt)};
 if(action==='dispose')parent.isDisposed=true;
 if(action==='advance')instance.subscriptionAttempt+=1;
 if(action==='reset')instance.subscriptionAttempt=0;
 settle?.();await pending;
 cases.push({disposed,action,outcome,failSend,initial,before,trace,attempt:String(instance.subscriptionAttempt),forceClosed:instance.forceClosed});
}
assert.equal(cases.length,640);
process.stdout.write(JSON.stringify(cases));
