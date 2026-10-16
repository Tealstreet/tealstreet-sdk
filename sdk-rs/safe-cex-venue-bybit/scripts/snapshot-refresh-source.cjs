const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = process.cwd();
const source = ts.createSourceFile('bybit.exchange.ts', fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts'),'utf8'), 99, true);
assert.equal(source.parseDiagnostics.length,0);
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BybitExchange');
const names=['debouncedPrivateSnapshotRefresh','scheduleSetTradingStopSnapshotRefresh'];
const fields=names.map(name=>{
 const found=klass.members.filter(n=>n.name?.getText(source)===name);
 assert.equal(found.length,1);assert.ok(found[0].initializer);
 return `${name}=${found[0].initializer.getText(source)};`;
});
const constants=['PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS','PRIVATE_SNAPSHOT_REFRESH_MAX_WAIT_MS'];
const declarations=source.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>constants.includes(d.name.getText(source))));
assert.equal(declarations.length,2);
const program=declarations.map(n=>n.getText(source)).join('\n')+'\nclass Witness {'+fields.join('\n')+'}\nexports.Witness=Witness;';
const emitted=ts.transpileModule(program,{compilerOptions:{target:7,module:1},reportDiagnostics:true});
assert.equal(emitted.diagnostics.length,0);
function make({disposed=false,publicOnly=false,hold=false,reject=false,reenter=false,cancelInside=false,reportReenter=false,reportThrows=false,syncThrow=false}={}) {
 let mono=1000,wall=1000,id=0;
 const timers=new Map(),trace=[],observations=[],pending=[],observed=[],invoked=[];
 const context=vm.createContext({exports:{},
  setTimeout(callback,delay){const token=++id;timers.set(token,{callback,at:mono+delay});return token;},
  clearTimeout(token){timers.delete(token);},
 });
 const runModule=(name,requireFn)=>{
  const module={exports:{}};
  const wrapper=vm.runInContext('(function(module,exports,require){'+fs.readFileSync(require.resolve(name),'utf8')+'\n})',context);
  wrapper(module,module.exports,requireFn);return module.exports;
 };
 const now=runModule('lodash/now',name=>{assert.equal(name,'./_root');return {Date:{now:()=>wall}};});
 const lodashRequire=createRequire(require.resolve('lodash/debounce'));
 const actualDebounce=runModule('lodash/debounce',name=>name==='./now'?now:lodashRequire(name));
 context.debounce=(func,...options)=>actualDebounce(function(...args){
  const result=Reflect.apply(func,this,args);
  assert.ok(result&&typeof result.then==='function','source async callback promise');
  invoked.push(result);
  observed.push(result.then(()=>observations.push(['resolved-observation',mono]),error=>observations.push(['rejected-observation',mono,error.message])));
  return result;
 },...options);
 vm.runInContext(emitted.outputText,context);
 const owner=new context.exports.Witness();
 owner.isDisposed=disposed;owner.isPublicDataOnly=publicOnly;
 owner.fetchAndSetPrivateSnapshotHelper=function(){
  assert.equal(this,owner,'source snapshot helper receiver');
  trace.push(['fetch',mono]);
  if(reenter){reenter=false;owner.scheduleSetTradingStopSnapshotRefresh();}
  if(cancelInside)owner.debouncedPrivateSnapshotRefresh.cancel();
  if(syncThrow)throw new Error('synchronous snapshot refused');
  if(hold)return new Promise((resolve,rejectFn)=>pending.push({resolve,reject:rejectFn}));
  return reject?Promise.reject(new Error('snapshot refused')):Promise.resolve();
 };
 owner.emitAndLogError=function(error){
  assert.equal(this,owner,'source snapshot reporter receiver');
  trace.push(['report',mono,error.message]);
  if(reportReenter){reportReenter=false;const returned=owner.debouncedPrivateSnapshotRefresh();trace.push(['report-return-current',mono,invoked.length>0&&returned===invoked[0],returned===undefined]);}
  if(reportThrows)throw new Error('reporting listener refused');
 };
 const drain=async()=>{if(!hold)await Promise.all(observed);await Promise.resolve();await Promise.resolve();};
 return {owner,trace,pending,observations,invoked,
  async advance(to){
   assert.ok(to>=mono);
   for(let count=0;count<1000;count++){
    const next=[...timers].filter(([,timer])=>timer.at<=to).sort((a,b)=>a[1].at-b[1].at||a[0]-b[0])[0];
    if(!next){wall+=to-mono;mono=to;await drain();return;}
    const [token,timer]=next;wall+=timer.at-mono;mono=timer.at;timers.delete(token);
    timer.callback();
    await drain();
   }
   assert.fail('timer progress bound');
  },
  async call(){trace.push(['call',mono]);owner.scheduleSetTradingStopSnapshotRefresh();await drain();},
  async settle(){await Promise.all(observed);},
  cancel(){owner.debouncedPrivateSnapshotRefresh.cancel();},
  timerCount(){return timers.size},
  setWall(value){wall=value;},
 };
}
(async()=>{
 const cases=[];
 for(const variant of ['single','trailing','max-wait','cancel','cancel-reschedule','clock-rollback','disposed','public-only','overlap','rejected','reentrant','cancel-inside']){
  const x=make({disposed:variant==='disposed',publicOnly:variant==='public-only',hold:variant==='overlap',reject:variant==='rejected',reenter:variant==='reentrant',cancelInside:variant==='cancel-inside'});
  await x.call();
  if(variant==='trailing'){await x.advance(1200);await x.call();}
  if(variant==='max-wait')for(const time of [1200,1400,1600,1800,2000]){await x.advance(time);await x.call();}
  if(variant==='clock-rollback'){await x.advance(1100);x.setWall(900);await x.call();}
  if(variant==='cancel')x.cancel();
  if(variant==='cancel-reschedule'){x.cancel();await x.advance(1100);await x.call();}
  await x.advance(2250);
  if(variant==='overlap'){await x.call();await x.advance(2500);assert.equal(x.pending.length,2);for(const pending of x.pending)pending.resolve();await x.advance(2501);}
  await x.settle();
  const times=x.trace.filter(event=>event[0]==='fetch').map(event=>event[1]);
  const expected={single:[1250],trailing:[1450],'max-wait':[2200],cancel:[],'cancel-reschedule':[1350],'clock-rollback':[1100],disposed:[],'public-only':[],overlap:[1250,2500],rejected:[1250],reentrant:[1250,1500],'cancel-inside':[1250]}[variant];
  assert.deepEqual(times,expected,variant+' independent invocation times');
  assert.equal(x.trace.filter(event=>event[0]==='report').length,Number(variant==='rejected'));
  const expectedSettlements={single:1,trailing:1,'max-wait':1,cancel:0,'cancel-reschedule':1,'clock-rollback':1,disposed:1,'public-only':1,overlap:2,rejected:1,reentrant:2,'cancel-inside':1}[variant];
  assert.equal(x.invoked.length,expectedSettlements,variant+' fixed async invocation floor');
  assert.equal(x.observations.length,expectedSettlements,variant+' fixed result observation floor');
  assert.equal(x.observations.filter(event=>event[0]==='rejected-observation').length,0);
  cases.push({variant,trace:x.trace,timers:x.timerCount(),observations:x.observations});
 }
 assert.equal(cases.length,12);
 const supplements=[];
 for(const kind of ['promise-rejection-reentry','sync-throw-reentry','reporter-rejection']){
  const x=make({reject:kind!=='sync-throw-reentry',syncThrow:kind==='sync-throw-reentry',reportReenter:kind!=='reporter-rejection',reportThrows:kind==='reporter-rejection'});
  await x.call();await x.advance(1500);await x.settle();
  const fetches=x.trace.filter(e=>e[0]==='fetch').map(e=>e[1]);
  assert.deepEqual(fetches,kind==='reporter-rejection'?[1250]:[1250,1500]);
  if(kind!=='reporter-rejection'){
   assert.equal(x.invoked.length,2,kind+' fixed invocation floor');assert.equal(x.observations.length,2,kind+' fixed observation floor');
   const reentry=x.trace.filter(e=>e[0]==='report-return-current');assert.equal(reentry.length,1);
   assert.deepEqual(reentry[0].slice(2),kind==='sync-throw-reentry'?[false,true]:[true,false],kind+' source publication ordering');
   assert.equal(x.observations.filter(e=>e[0]==='rejected-observation').length,0);
  }else{
   assert.equal(x.observations.length,1);assert.equal(x.observations[0][0],'rejected-observation');assert.equal(x.observations[0][2],'reporting listener refused');
  }
  supplements.push({kind,trace:x.trace,observations:x.observations});
 }
 assert.equal(supplements.length,3);
 const identities=[];
 for(const kind of ['flush-new-result','flush-pending-replacement','cancel-retains-result','flush-reentrant']){
  const x=make({hold:true,reenter:kind==='flush-reentrant'});
  const debounce=x.owner.debouncedPrivateSnapshotRefresh;
  assert.equal(debounce(),undefined,kind+' initial call result');
  await x.advance(1100);
  const first=debounce.flush();
  assert.equal(x.invoked.length,1,kind+' eager flush invocation');
  assert.equal(first,x.invoked[0],kind+' flush returns actual async Promise');
  assert.equal(debounce(),first,kind+' later call returns cached Promise');
  if(kind==='flush-pending-replacement'){
   const second=debounce.flush();
   assert.equal(x.invoked.length,2,kind+' second flush invocation');
   assert.equal(second,x.invoked[1],kind+' replacement result');
   assert.notEqual(first,second,kind+' separate async identities');
  }else if(kind==='cancel-retains-result'){
   debounce.cancel();
   assert.equal(debounce.flush(),first,kind+' cancel does not erase result');
  }
  for(const pending of x.pending)pending.resolve();
  await x.settle();
  await x.advance(1500);
  for(const pending of x.pending)pending.resolve();
  await x.settle();
  const expectedFetches={
   'flush-new-result':[1100,1350],
   'flush-pending-replacement':[1100,1100],
   'cancel-retains-result':[1100],
   'flush-reentrant':[1100,1350],
  }[kind];
  assert.deepEqual(x.trace.filter(e=>e[0]==='fetch').map(e=>e[1]),expectedFetches,kind+' physical timer outcomes');
  assert.equal(x.invoked.length,expectedFetches.length,kind+' invocation floor');
  assert.equal(x.observations.length,expectedFetches.length,kind+' observation floor');
  identities.push({kind,trace:x.trace,observations:x.observations,timers:x.timerCount()});
 }
 assert.equal(identities.length,4);
 const lifetimes=[];
 for(const kind of ['disposed-pending-success','disposed-pending-rejection','public-only-pending-success']){
  const x=make({hold:true});
  await x.call();await x.advance(1250);
  assert.equal(x.pending.length,1,kind+' already-started helper floor');
  assert.equal(x.observations.length,0,kind+' helper remains pending');
  if(kind==='public-only-pending-success')x.owner.isPublicDataOnly=true;
  else {x.owner.isDisposed=true;x.cancel();}
  if(kind==='disposed-pending-rejection')x.pending[0].reject(new Error('pending snapshot refused'));
  else x.pending[0].resolve();
  await x.settle();
  assert.deepEqual(x.trace.filter(e=>e[0]==='fetch'),[['fetch',1250]],kind+' no repeated helper');
  assert.equal(x.invoked.length,1,kind+' original Promise retained');
  assert.deepEqual(x.observations,[['resolved-observation',1250]],kind+' started callback still settles');
  assert.deepEqual(x.trace.filter(e=>e[0]==='report'),kind==='disposed-pending-rejection'?[['report',1250,'pending snapshot refused']]:[],kind+' post-disposal catch behavior');
  lifetimes.push({kind,trace:x.trace,observations:x.observations});
 }
 assert.equal(lifetimes.length,3);
 console.log(JSON.stringify({scope:'actual maintained fields and actual lodash debounce/now with declared deterministic timer host',cases,supplements,identities,lifetimes}));
})().catch(error=>{console.error(error);process.exitCode=1});
