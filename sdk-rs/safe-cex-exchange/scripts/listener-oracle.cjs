const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const compiler = process.env.SAFE_CEX_TYPESCRIPT_PATH || require.resolve('typescript');
const ts = require(compiler);
const {createRequire} = require('node:module');
const isolated = createRequire(require.resolve(compiler));
const {TinyEmitter} = isolated('tiny-emitter');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src');
function methods(file, className, names) {
  const source = ts.createSourceFile(file, fs.readFileSync(path.join(root,file),'utf8'), ts.ScriptTarget.Latest, true);
  const cls = source.statements.find(n => ts.isClassDeclaration(n) && n.name.text === className);
  assert.ok(cls, `actual ${className}`);
  return names.map(name => {
    const member=cls.members.find(n=>n.name?.getText(source)===name);
    assert.ok(member, `actual ${className}.${name}`);
    return member.getText(source);
  }).join('\n');
}
const context=vm.createContext({console:{log(){}}, LogSeverity:{Info:'info',Warning:'warning'}, FILL_RECONCILED_VENUES:new Set(), Error});
const source=`class Base { ${methods('exchanges/base.ts','BaseExchange',['log','waitForRemoteSigner'])} }\nclass Manager { ${methods('webworker/chaserOrder/ChaserOrderManager.ts','ChaserOrderManager',['log','findBestPrice','emitComplete','bybitDemoPositionFill'])} }\nglobalThis.Base=Base;globalThis.Manager=Manager;`;
vm.runInContext(ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
(async()=>{
 if(process.argv.includes('--resync')) {
  const text=fs.readFileSync(path.join(root,'exchanges/base.ws.ts'),'utf8');
  const source=ts.createSourceFile('base.ws.ts',text,ts.ScriptTarget.Latest,true);
  const statement=source.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(source)==='runReopenResync'));
  assert.ok(statement,'actual runReopenResync declaration');
  vm.runInContext(ts.transpileModule(`const reopenResyncInFlight=new WeakMap(); ${statement.getText(source)}; globalThis.runResync=runReopenResync; globalThis.resyncState=reopenResyncInFlight;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
  const rows=[];
  for(const disposed of [false,true]) {
   const failure=new TypeError('resync log listener failed');
   let rejectFirst, fetches=0, rejected;
   const events=[];
   const first=new Promise((_,reject)=>{rejectFirst=reject;});
   const parent={isDisposed:false,fetchAndSetPrivateSnapshot(n){assert.equal(n,10);events.push('fetch');return ++fetches===1?first:Promise.resolve();},log(_,severity){assert.equal(severity,'warning');events.push('log');throw failure;}};
   const onUnhandled=error=>{rejected=error;events.push('unhandled');};
   process.once('unhandledRejection',onUnhandled);
   try {
    context.runResync(parent);
    context.resyncState.get(parent).rerun=true;
    parent.isDisposed=disposed;
    rejectFirst(new Error('snapshot rejected'));
    await new Promise(setImmediate);
    assert.equal(rejected,failure,'source resync rejection retains logger error identity');
    assert.equal(fetches,disposed?1:2,'source finally retains queued resync unless disposed');
    assert.deepEqual(events,disposed?['fetch','log','unhandled']:['fetch','log','fetch','unhandled'],'source finally runs before unhandled rejection delivery');
    rows.push({disposed,fetches,running:context.resyncState.has(parent),error:{name:rejected.name,message:rejected.message}});
   } finally {process.removeListener('unhandledRejection',onUnhandled);}
  }
  process.stdout.write(JSON.stringify(rows));return;
 }
 if(process.argv.includes('--signer-lifetime')) {
  const rows=[];
  for(const finish of ['available','disposed']) {
   const callbacks=new Set(), events=[];
   let removals=0;
   const signer={isAvailable:()=>false,onAvailabilityChange(cb){callbacks.add(cb);return()=>{assert.ok(callbacks.delete(cb));removals++;};}};
   const emitter=new TinyEmitter();
   emitter.on('remoteSignerStatus',value=>events.push(value));
   const base=new context.Base();Object.assign(base,{emitter,options:{remoteSigner:signer},isDisposed:false,remoteSignerAvailabilityCleanups:new Set()});
   void base.waitForRemoteSigner();
   assert.deepEqual(events,['disconnected']);
   assert.equal(callbacks.size,1,'discarding the Promise keeps availability subscription');
   if(finish==='available')for(const callback of [...callbacks])callback(true);
   else {base.isDisposed=true;for(const cleanup of [...base.remoteSignerAvailabilityCleanups])cleanup();}
   await new Promise(setImmediate);
   assert.equal(callbacks.size,0);assert.equal(removals,1);assert.equal(base.remoteSignerAvailabilityCleanups.size,0);
   rows.push({finish,events,removals});
  }
  process.stdout.write(JSON.stringify(rows));return;
 }
 if(process.argv.includes('--signer')) {
  const rows=[];
  for(const fail of ['disconnected','connected']) {
   const emitter=new TinyEmitter(), failure=new TypeError(`${fail} listener failed`);
   let callback, removals=0, settled=false, rejection;
   const signer={isAvailable:()=>false,onAvailabilityChange(cb){callback=cb;return()=>{removals++;};}};
   const base=new context.Base();Object.assign(base,{emitter,options:{remoteSigner:signer},isDisposed:false,remoteSignerAvailabilityCleanups:new Set()});
   emitter.on('remoteSignerStatus',value=>{if(value===fail)throw failure;});
   const promise=base.waitForRemoteSigner();
   promise.then(()=>{settled=true;},error=>{settled=true;rejection=error;});
   await Promise.resolve();
   let thrown;
   if(callback)try{callback(true);}catch(error){thrown=error;}
   await Promise.resolve();
   for(const cleanup of base.remoteSignerAvailabilityCleanups)cleanup();
   await Promise.resolve();
   assert.equal(fail==='connected'?thrown:rejection,failure,'signer status retains listener identity');
   assert.equal(settled,fail==='disconnected','connected throw leaves source promise unresolved');
   rows.push({fail,settled,registered:!!callback,removals,name:failure.name});
  }
  process.stdout.write(JSON.stringify(rows));return;
 }
 const rows=[];
 for(const event of ['log','error','update']) {
   const emitter=new TinyEmitter(),events=[],failure=new TypeError(`${event} failure`);
   emitter.on(event,()=>{events.push('first');throw failure;});
   emitter.on(event,()=>events.push('second'));
   let caught;try{emitter.emit(event,{})}catch(error){caught=error;}
   assert.equal(caught,failure,'emitter original error identity');
   assert.deepEqual(events,['first'],'emitter throw stops later listener');
   rows.push({kind:'fanout',event,events,name:caught.name,message:caught.message});
 }
 for(const fail of ['log','error']) {
   const emitter=new TinyEmitter(),events=[],failure=new TypeError(`${fail} failure`);
   const base=new context.Base();Object.assign(base,{emitter,name:'bybit',options:{tuning:{consoleLogging:false}},store:{marketsBySymbol:{},markets:[],tickersBySymbol:{},tickers:[]}});
   const instance={id:'i',exchange:base,options:{symbol:'BTC'},markComplete(){events.push('stop');}};
   for(const event of ['log','error']){
     emitter.on(event,()=>{events.push(event);if(event===fail)throw failure;});
     emitter.on(event,()=>events.push(`${event}-later`));
   }
   let caught;try{new context.Manager().findBestPrice(instance)}catch(error){caught=error;}
   assert.equal(caught,failure,'worker listener error identity');
   assert.deepEqual(events,fail==='log'?['log']:['log','log-later','stop','error'],'worker stop ordering');
   rows.push({kind:'price',fail,events,name:caught.name,message:caught.message});
 }
 for(const logFails of [false,true]){
   const emitter=new TinyEmitter(),events=[],completeFailure=new TypeError('complete failure'),logFailure=new TypeError('log failure');
   emitter.on('chaser:complete',()=>{events.push('complete');throw completeFailure;});
   emitter.on('chaser:complete',()=>events.push('complete-later'));
   emitter.on('log',message=>{events.push('log');assert.ok(message.includes('TypeError: complete failure'));if(logFails)throw logFailure;});
   const instance={id:'i',exchange:{name:'bybit',emitter,store:{orders:[]}},state:{stopReason:'stop'},options:{symbol:'BTC',side:'buy',size:1},everTrackedOrderIds:[],filledSize:0};
   let caught;try{await new context.Manager().emitComplete(instance)}catch(error){caught=error;}
   assert.equal(caught,logFails?logFailure:undefined,'completion catches listener but not recovery log');
   rows.push({kind:'complete',logFails,events,error:caught?.message??null});
 }
 process.stdout.write(JSON.stringify(rows));
})().catch(error=>{console.error(error);process.exitCode=1;});
