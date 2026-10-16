import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const encode=n=>n===undefined?'unset':Number.isNaN(n)?'nan':n===Infinity?'infinity':n===-Infinity?'-infinity':Object.is(n,-0)?'-zero':n;
const token=n=>n===undefined?undefined:{id:n},id=t=>t?.id;
const results=[];
for(const [variant,klass] of [['binance','BinancePrivateWebsocket'],['binancepm','BinancePMPrivateWebsocket']]){
 const file=new URL(`../../../packages/safe-cex/src/exchanges/binance/${variant}.ws-private.ts`,import.meta.url),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);
 const members=['dispose','detachSocket','scheduleListenKeyRefresh'].map(name=>{const m=decl.members.find(m=>m.name?.getText(ast)===name);assert.ok(m);return m.getText(ast);});
 let active;
 class Base {dispose(){active.trace.push(['base']);if(active.fail==='base')throw active.error;active.disposed=true;}}
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.Subject=class extends BaseWebSocket {${members.join('\n')}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,BaseWebSocket:Base,WebSocket:{OPEN:1,CONNECTING:0},clearTimeout:t=>active.clear(t),setTimeout:(fn,delay)=>active.set(fn,delay)});
 for(const name of ['normal','unset','same','clear1-fail','clear2-fail','base-fail','late-refresh']){
  const instance=new exports.Subject();instance.listenKeyRetryTimer=token(name==='unset'?undefined:11);instance.listenKeyRefreshTimer=token(name==='unset'?undefined:name==='same'?11:22);
  const trace=[],cleared=[],remaining=new Set([id(instance.listenKeyRetryTimer),id(instance.listenKeyRefreshTimer)].filter(x=>x!==undefined)),error=RangeError('dispose fixture');let failure=null;
  active={trace,error,fail:name==='base-fail'?'base':null,disposed:false,clear:t=>{const ordinal=cleared.length+1;trace.push(['clear',encode(id(t))]);cleared.push(encode(id(t)));if(name===`clear${ordinal}-fail`)throw error;remaining.delete(id(t));if(name==='late-refresh'&&ordinal===1){instance.listenKeyRefreshTimer=token(33);remaining.add(33);}}};
  try{instance.dispose();}catch(e){assert.equal(e,error);failure='original';}
  results.push({variant,method:'dispose',name,trace,cleared,remaining:[...remaining].sort((a,b)=>a-b),disposed:active.disposed,retry:encode(id(instance.listenKeyRetryTimer)),refresh:encode(id(instance.listenKeyRefreshTimer)),failure});
 }
 const specs=[['normal',1800000],['zero',0],['negative',-1],['fraction',1.5],['nan',NaN],['infinity',Infinity],['negative-infinity',-Infinity],['negative-zero',-0],['clear-fail',1800000],['set-fail',1800000],['late-field-set-fail',1800000],['unset-field',1800000],['repeat-callback',1800000]];
 for(const [name,delay] of specs){
  const instance=new exports.Subject();instance.listenKeyRefreshTimer=token(name==='unset-field'?undefined:22);instance.listenKeyRetryTimer=token(11);
  const trace=[],calls=[],remaining=new Set(name==='unset-field'?[]:[22]),error=RangeError('schedule fixture');let callback,seenDelay,seenTimer,failure=null;
  instance.updateListenKey=()=>{calls.push(1);return 99;};
  active={trace,clear:t=>{trace.push(['clear',encode(id(t))]);if(name==='clear-fail')throw error;remaining.delete(id(t));if(name==='late-field-set-fail')instance.listenKeyRefreshTimer=token(33);},set:(fn,delay)=>{trace.push(['set',encode(delay)]);seenDelay=delay;seenTimer=encode(id(instance.listenKeyRefreshTimer));if(name==='set-fail'||name==='late-field-set-fail')throw error;callback=fn;remaining.add(42);return token(42);}};
  try{instance.scheduleListenKeyRefresh(delay);}catch(e){assert.equal(e,error);failure='original';}
  const callbackIsUpdate=callback===instance.updateListenKey;let returnUndefined=null;
  if(callback){if(name!=='repeat-callback')instance.updateListenKey=()=>{calls.push(2);return 99;};returnUndefined=callback()===undefined;if(name==='repeat-callback')returnUndefined=callback()===undefined&&returnUndefined;}
  results.push({variant,method:'scheduleListenKeyRefresh',name,delay:encode(delay),trace,calls,remaining:[...remaining].sort((a,b)=>a-b),seenDelay:encode(seenDelay),seenTimer:seenTimer??null,callbackIsUpdate,returnUndefined,retry:encode(id(instance.listenKeyRetryTimer)),refresh:encode(id(instance.listenKeyRefreshTimer)),failure});
 }
 const detachSpecs=[['missing',0],['connecting',0],['open',1],['closing',2],['closed',3],['listeners-absent',1],['remove1-fail',0],['remove2-fail',0],['remove3-fail',0],['read1-fail',0],['read2-fail',0],['close-fail',0],['changed-ready',2],['swapped-owner',1],['handler-change',0],['late-state-change',1]];
 for(const [name,initial] of detachSpecs){
  const instance=new exports.Subject(),trace=[],removed=[],remaining=new Set(['message','close','open']),error=RangeError('detach fixture');let reads=0,closeCalls=0,closed=false,failure=null,state=initial;
  const handlers=Object.fromEntries(['message','close','open'].map((name,i)=>[name,{id:(i+1)*101}]));instance.onMessage=handlers.message;instance.onClose=handlers.close;instance.onOpen=handlers.open;
  const socket={id:100,get readyState(){reads++;trace.push(['read',100]);if(name===`read${reads}-fail`)throw error;return name==='changed-ready'?(reads===1?2:0):state;},close(){closeCalls++;trace.push(['close',100]);if(name==='close-fail')throw error;closed=true;}};
  if(name!=='listeners-absent')socket.removeEventListener=(event,handler)=>{trace.push(['remove',100,event,handler.id]);removed.push([event,handler.id]);if(name===`remove${removed.length}-fail`)throw error;remaining.delete(event);if(removed.length===1){if(name==='swapped-owner')instance.ws={id:404};if(name==='handler-change')instance.onClose={id:999};if(name==='late-state-change')state=3;}};
  instance.ws=name==='missing'?null:socket;
  try{instance.detachSocket();}catch(e){assert.equal(e,error);failure='original';}
  results.push({variant,method:'detachSocket',name,initial,trace,removed,remaining:[...remaining].sort(),reads,closeCalls,closed,owner:instance.ws?.id??null,failure});
 }
}
assert.equal(results.length,72);
process.stdout.write(JSON.stringify(results));
