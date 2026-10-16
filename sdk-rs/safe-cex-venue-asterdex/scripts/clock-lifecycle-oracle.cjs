// Execute the unedited source class, real Dayjs, deterministic Axios and Node timer seams.
const fs=require('node:fs'),path=require('node:path');
const {values,ts,root}=require('./source.cjs');
const tsPath=process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root,'sdk-rs/target/asterdex-oracle-deps/node_modules/typescript');
const compilerRoot=path.resolve(path.dirname(require.resolve(tsPath)),'..');
if(require(path.join(compilerRoot,'package.json')).name!=='typescript')throw Error('Unrecognized compiler');
const dayjsPath=path.join(path.dirname(compilerRoot),'dayjs');
const dayjs=require(dayjsPath),utc=require(path.join(dayjsPath,'plugin/utc'));
const load=require('./load-module.cjs');
const cases=JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/clock-lifecycle-cases.json'),'utf8'));
const RealDate=Date,realInterval=setInterval,realClear=clearInterval,realLog=console.log;
let wall=1000,monotonic=0,requests=[],pending=[],timers=new Map(),nextTimer=0,logs=[];
global.Date=class extends RealDate{constructor(...args){super(...(args.length?args:[wall]));}static now(){return wall;}};
global.setInterval=(callback,period)=>{const id=++nextTimer;timers.set(id,{callback,period,deadline:monotonic+period});return id;};
global.clearInterval=id=>timers.delete(id);
console.log=message=>logs.push(message);
const axios={get(url,config){requests.push({url,timeout:config.timeout});return new Promise((resolve,reject)=>pending.push({resolve,reject}));}};
const loaded=load('packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts',['AsterDEXVirtualClock'],{
 './asterdex.types':values,axios,dayjs,'dayjs/plugin/utc':utc,'../../transport/registry':{withPublicCorsProxy:url=>'proxy:'+url}
});
const {AsterDEXVirtualClock}=loaded;
const number=n=>Number.isNaN(n)?'NaN':n;
const flush=async()=>{for(let i=0;i<32;i++)await Promise.resolve();};
(async()=>{
 const out=[];
 try{
  const first=loaded.asterDEXVirtualClock,again=loaded.asterDEXVirtualClock;
  if(!Object.is(first,again) || Object.is(first,new AsterDEXVirtualClock()))throw Error('singleton identity');
  const syncing=first.forceSyncAndGetOffset();
  if(pending.length!==1)throw Error('singleton dispatch');
  pending.shift().resolve({data:{serverTime:1100}});await syncing;await flush();
  if(again.getTimeOffset()!==100)throw Error('singleton shared native state');
  first.stop();if(again.getTimeOffset()!==0)throw Error('singleton shared stop');
  for(const scenario of cases){
   wall=1000;monotonic=0;requests=[];pending=[];timers=new Map();logs=[];
   const clock=new AsterDEXVirtualClock(),operations=[],snapshots=[];
   for(const step of scenario.steps){
    const testnet=step.testnet??false;
    switch(step.op){
     case 'sync':case 'force':case 'start':{
      const fn={sync:'syncClientServerTime',force:'forceSyncAndGetOffset',start:'start'}[step.op];
      const promise=step.version===undefined?clock[fn](testnet):clock[fn](testnet,step.version);
      if(!step.discard){const record={kind:step.op,done:false};operations.push(record);promise.then(value=>{record.done=true;if(step.op==='force')record.value=number(value);});}
      break;
     }
     case 'stop':clock.stop();break;
     case 'wall':wall=step.value;break;
     case 'resolve':{
      const request=pending.shift();if(!request)throw Error('No pending Axios request');
      if(step.error)request.reject(Error('offline'));else request.resolve({data:step.data});break;
     }
     case 'tick':{
      monotonic=step.value;
      for(const [id,timer]of [...timers])if(timers.has(id)&&timer.deadline<=monotonic){const captured=monotonic;timer.callback();if(timers.has(id))timer.deadline=captured+timer.period;}
      break;
     }
     case 'snapshot':snapshots.push({offset:number(clock.getTimeOffset()),current:number(clock.getCurrentTimeMs()),needs:clock.needsSync(),timers:timers.size,inFlight:clock.syncInFlight!==null,requests:requests.length});break;
     default:throw Error('Unknown step '+step.op);
    }
    await flush();
   }
   out.push({name:scenario.name,snapshots,requests,operations,logs});clock.stop();
  }
 }finally{global.Date=RealDate;global.setInterval=realInterval;global.clearInterval=realClear;console.log=realLog;}
 process.stdout.write(JSON.stringify(out));
})().catch(error=>{process.stderr.write(error.stack);process.exitCode=1;});
