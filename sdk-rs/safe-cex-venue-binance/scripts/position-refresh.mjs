import assert from 'node:assert/strict';import fs from 'node:fs';import vm from 'node:vm';import {createRequire} from 'node:module';
const ts=createRequire(import.meta.url)(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const throttleNames=['queued','boundary','overdue','disposed','paused','scheduled','no-pending','negative-elapsed','nan-now','nan-throttle','negative-zero-throttle','clock-fail','pause-fail','set-fail','immediate-fail','callback-disposed','callback-no-pending','callback-fail','late-method','repeat-callback','clock-mutates','pause-mutates'];
const executeNames=['normal','inflight','disposed','paused','pause-fail','version-start-fail','clock-fail','fetch-sync-fail','fetch-reject','disposed-during-fetch','version-changed','pending-during-fetch','version-nan','version-zero-sign','version-end-fail','apply-fail','apply-sets-pending','apply-disposes','retry-fail','retry-sets-inflight','paused-mutates-disposal','clock-disposes','clock-mutates-pending','fetch-marks-version-and-pending','pending-after-reject'];
const encode=n=>Number.isNaN(n)?'nan':Object.is(n,-0)?'-zero':n;
const results=[];let active;
for(const [variant,klass] of [['binance','BinancePrivateWebsocket'],['binancepm','BinancePMPrivateWebsocket']]){
 const file=new URL(`../../../packages/safe-cex/src/exchanges/binance/${variant}.ws-private.ts`,import.meta.url),ast=ts.createSourceFile(file.pathname,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),decl=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);
 const names=['lastPositionFetchTime','positionFetchThrottleMs','positionFetchScheduled','positionFetchInFlight','hasPendingPositionEvents','throttledFetchPositions','executePositionFetch'];
 const members=names.map(name=>{const m=decl.members.find(m=>m.name?.getText(ast)===name);if(!m&&name==='positionFetchInFlight'&&variant==='binancepm')return '';assert.ok(m);return m.getText(ast);});
 const exports={};vm.runInNewContext(ts.transpileModule(`exports.Subject=class {${members.join('\n')}}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,{exports,Date:{now:()=>active.now()},setTimeout:(fn,delay)=>active.set(fn,delay)});
 const snapshot=i=>({disposed:i.isDisposed,pending:i.hasPendingPositionEvents,scheduled:i.positionFetchScheduled,inflight:i.positionFetchInFlight??false,last:encode(i.lastPositionFetchTime),throttle:encode(i.positionFetchThrottleMs)});
 for(const name of throttleNames){
  const instance=new exports.Subject(),trace=[],observed=[],calls=[],error=RangeError('position fixture');let callback,delay,current=1,failure=null,callbackFailure=null;
  instance.isDisposed=name==='disposed';instance.hasPendingPositionEvents=name!=='no-pending';instance.positionFetchScheduled=name==='scheduled';
  const standard=instance.positionFetchThrottleMs;
  let now=(name==='boundary'||name==='immediate-fail')?standard:name==='overdue'?standard+1:name==='negative-elapsed'?100:name==='nan-now'?NaN:name==='negative-zero-throttle'?0:100;
  if(name==='negative-elapsed')instance.lastPositionFetchTime=200;
  if(name==='nan-throttle')instance.positionFetchThrottleMs=NaN;
  if(name==='negative-zero-throttle')instance.positionFetchThrottleMs=-0;
  instance.parent={isBackgroundReconciliationPaused(){trace.push(['paused']);if(name==='pause-fail')throw error;if(name==='pause-mutates'){instance.isDisposed=true;instance.lastPositionFetchTime=50;instance.positionFetchThrottleMs=200;}return name==='paused';}};
  const execute=id=>()=>{trace.push(['execute',id]);observed.push([instance.positionFetchScheduled,instance.hasPendingPositionEvents,instance.isDisposed]);calls.push(id);if(name==='immediate-fail'||name==='callback-fail')throw error;};
  instance.executePositionFetch=execute(1);
  active={now(){trace.push(['now']);if(name==='clock-fail')throw error;if(name==='clock-mutates')instance.isDisposed=true;return now;},set(fn,ms){trace.push(['set',encode(ms)]);delay=ms;callback=fn;if(name==='set-fail')throw error;return {id:42};}};
  try{instance.throttledFetchPositions();}catch(e){assert.equal(e,error);failure='original';}
  let callbackIsExecute=null,callbackVoid=null;
  if(callback){callbackIsExecute=callback===instance.executePositionFetch;assert.equal(callbackIsExecute,false);if(failure===null){if(name==='late-method'){current=2;instance.executePositionFetch=execute(2);}if(name==='callback-disposed')instance.isDisposed=true;if(name==='callback-no-pending')instance.hasPendingPositionEvents=false;try{callbackVoid=callback()===undefined;if(name==='repeat-callback')callbackVoid=callback()===undefined&&callbackVoid;}catch(e){assert.equal(e,error);callbackFailure='original';}}}
  results.push({...snapshot(instance),variant,method:'throttle',name,trace,observed,calls,delay:delay===undefined?null:encode(delay),callbackIsExecute,callbackVoid,failure,callbackFailure});
 }
 for(const name of executeNames){
  const instance=new exports.Subject(),trace=[],observed=[],error=RangeError('position fixture'),positions=[{id:'first',contracts:3.6,entryPrice:12.3},{id:'second',contracts:2.1,entryPrice:14.2}];let version=name==='version-nan'?NaN:name==='version-zero-sign'?-0:1,versionReads=0,applyCount=0,retryCount=0,failure=null;
  instance.isDisposed=name==='disposed';instance.positionFetchInFlight=name==='inflight';instance.hasPendingPositionEvents=true;
  const observe=label=>observed.push([label,instance.positionFetchInFlight,instance.hasPendingPositionEvents,instance.isDisposed,encode(instance.lastPositionFetchTime)]);
  instance.parent={isBackgroundReconciliationPaused(){trace.push(['paused']);if(name==='pause-fail')throw error;if(name==='paused-mutates-disposal')instance.isDisposed=true;return name==='paused';},getPositionStateVersion(){versionReads++;trace.push(['version',encode(version)]);if((name==='version-start-fail'&&versionReads===1)||(name==='version-end-fail'&&versionReads===2))throw error;return version;},fetchPositions(){trace.push(['fetch']);observe('fetch');if(name==='fetch-sync-fail')throw error;return Promise.resolve().then(()=>{if(name==='disposed-during-fetch')instance.isDisposed=true;if(name==='version-changed'||name==='fetch-marks-version-and-pending')version=2;if(name==='version-zero-sign')version=0;if(['pending-during-fetch','retry-fail','retry-sets-inflight','fetch-marks-version-and-pending','pending-after-reject'].includes(name))instance.hasPendingPositionEvents=true;if(name==='fetch-reject'||name==='pending-after-reject')throw error;return positions;});}};
  instance.store={addOrUpdatePositionsFull(p){trace.push(['apply']);observe('apply');assert.equal(p,positions);applyCount++;if(name==='apply-sets-pending')instance.hasPendingPositionEvents=true;if(name==='apply-disposes')instance.isDisposed=true;if(name==='apply-fail')throw error;}};
  instance.throttledFetchPositions=()=>{trace.push(['retry']);observe('retry');retryCount++;if(name==='retry-sets-inflight')instance.positionFetchInFlight=true;if(name==='retry-fail')throw error;};
  active={now(){trace.push(['now']);observe('now');if(name==='clock-fail')throw error;if(name==='clock-disposes')instance.isDisposed=true;if(name==='clock-mutates-pending')instance.hasPendingPositionEvents=true;return 1000;},set(){throw Error('execute oracle unexpectedly scheduled native timer');}};
  try{await instance.executePositionFetch();}catch(e){assert.equal(e,error);failure='original';}
  results.push({...snapshot(instance),variant,method:'execute',name,trace,observed,applyCount,retryCount,failure,versionReads,positions:applyCount?positions:null,identity:applyCount?true:null});
 }
}
assert.equal(results.length,94,'independently authored floor 2*(22+25)');process.stdout.write(JSON.stringify(results));
