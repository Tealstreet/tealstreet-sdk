import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const results=[];
for(const [variant,klass] of [['binance','BinancePrivateWebsocket'],['binancepm','BinancePMPrivateWebsocket']]){
 const file=new URL(`../../../packages/safe-cex/src/exchanges/binance/${variant}.ws-private.ts`,import.meta.url),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);
 const members=['detachSocket'].map(name=>{const m=decl.members.find(m=>m.name?.getText(ast)===name);assert.ok(m);return m.getText(ast);});
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.Subject=class {${members.join('\n')}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,WebSocket:{OPEN:1,CONNECTING:0}});
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
assert.equal(results.length,32);
process.stdout.write(JSON.stringify(results));
