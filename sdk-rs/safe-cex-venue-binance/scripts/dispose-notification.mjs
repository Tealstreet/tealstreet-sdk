// Actual RWS disposal removes the outgoing close callback before touching its host.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH?name=>require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name)):require;
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function load(file,deps){
 const exports={};
 vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL(file,root),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,{exports,require:name=>{assert.ok(name in deps,`unexpected import ${name}`);return deps[name];},structuredClone});
 return exports;
}
const typesAst=ts.createSourceFile('types.ts',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enumText=typesAst.statements.filter(n=>ts.isEnumDeclaration(n)).map(n=>n.getText(typesAst)).join('\n');
const types={};vm.runInNewContext(ts.transpileModule(enumText,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:types});
const {DefaultStore}=load('store/store.base.ts',{'lodash/uniqBy':runtimeRequire(process.argv.includes('--missing-runtime-import')?'lodash/uniqBy-deliberately-absent':'lodash/uniqBy'),'../types':types,'../utils/clone':load('utils/clone.ts',{}),'../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1000}}});
const {LogSeverity}=load('../../logging/src/LogSeverity.ts',{});
const parse=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const base=parse('exchanges/base.ws.ts');
const baseClass=base.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BaseWebSocket');
const baseDispose=baseClass.members.find(n=>n.name?.getText(base)==='dispose');assert.ok(baseDispose);
const events=load('../../../thirdparty/reconnecting-websocket/src/events.ts',{});
let rwsSource=fs.readFileSync(new URL('../../../thirdparty/reconnecting-websocket/src/ReconnectingWebsocket.ts',root),'utf8');
if(process.argv.includes('--keep-rws-listeners')){
 const before=rwsSource;rwsSource=rwsSource.replace('this._removeListeners();\n    const ws = this._ws as any;', 'const ws = this._ws as any;');
 assert.notEqual(rwsSource,before,'listener control applies to actual terminate');
}
const rws={};let timerId=0;
vm.runInNewContext(ts.transpileModule(rwsSource,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{
 exports:rws,require:name=>{assert.equal(name,'./events');return events;},
 setTimeout(fn,delay){if(delay===0)queueMicrotask(fn);return ++timerId;},clearTimeout(){},console:{log(){},warn(){},error(){}},
});
const cases=[];
for(const [variant,name] of [['binance','BinancePublicWebsocket'],['binancepm','BinancePMPublicWebsocket']]){
 const ast=parse(`exchanges/binance/${variant}.ws-public.ts`);
 const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===name);
 const dispose=klass.members.find(n=>n.name?.getText(ast)==='dispose');assert.ok(dispose);
 let code=`class Base {${baseDispose.getText(base)}}\nexports.Klass=class extends Base {${dispose.getText(ast)}}`;
 if(process.argv.includes('--skip-parent-dispose'))code=code.replace('super.dispose();','void 0;');
 const exported={};vm.runInNewContext(ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exported,clearTimeout(){}});
 for(const terminates of [false,true])for(const delayed of [false,true])for(const readyState of [0,1])for(const listenerThrows of [false,true]){
  const trace=[],store=new DefaultStore();let raw,closeCallbacks=0,notifications=0;
  const unsub=store.subscribe(()=>{notifications++;if(listenerThrows)throw Error('unexpected health notification');});
  class Host {
   static CLOSING=2;
   constructor(){raw=this;this.readyState=readyState;this.handlers=new Map();if(terminates)this.terminate=()=>this.finish('terminate');}
   addEventListener(name,fn){const handlers=this.handlers.get(name)||[];handlers.push(fn);this.handlers.set(name,handlers);}
   removeEventListener(name,fn){this.handlers.set(name,(this.handlers.get(name)||[]).filter(item=>item!==fn));}
   close(){this.finish('close');}
   finish(kind){trace.push(kind);this.readyState=3;const emit=()=>{for(const fn of [...(this.handlers.get('close')||[])])fn({code:1000,reason:''});};if(delayed)queueMicrotask(emit);else emit();}
  }
  const socket=new rws.ReconnectingWebSocket('wss://fixture.invalid',[],{WebSocket:Host,minReconnectionDelay:0});
  // Drive only the actual constructor's resolved preparation promises. No network.
  for(let i=0;i<20;i++)await Promise.resolve();
  assert.ok(raw,'actual RWS constructor created host');
  assert.equal(raw.handlers.get('close').length,1,'actual RWS registered its close forwarder');
  socket.addEventListener('close',()=>{closeCallbacks++;store.updateSocketHealth({publicHealthy:false});});
  const instance=Object.assign(new exported.Klass(),{ws:socket,marketWebsocket:{dispose(){trace.push('child');}}});
  let error=null;
  try{instance.dispose();}catch(e){error=e.message;}
  const immediate={closeCallbacks,notifications};
  for(let i=0;i<4;i++)await Promise.resolve();
  assert.equal(closeCallbacks,0,'RWS disposal detaches close forwarding before host termination');
  assert.equal(notifications,0,'RWS disposal cannot publish health through removed listener');
  assert.deepEqual(trace,['child',terminates?'terminate':'close'],'venue child then actual parent termination');
  assert.equal(error,null,'disposal does not encounter subscriber errors');
  assert.equal(instance.ws,undefined,'base disposal clears its socket');
  assert.equal(instance.forceClosed,true,'base disposal marks force closed');
  assert.equal(socket._ws,undefined,'actual RWS terminate clears its host');
  cases.push({variant,terminates,delayed,readyState,listenerThrows,trace,immediate,closeCallbacks,notifications});
  unsub();
 }
}
assert.equal(cases.length,32);
process.stdout.write(JSON.stringify(cases));
