import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),lodash=require('lodash');
const math=Object.create(Math);math.random=()=>0.25;const random=lodash.runInContext({Math:math}).random;
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const ast=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=ast.statements.filter(s=>ts.isEnumDeclaration(s)&&['OrderSide','OrderType','LogSeverity'].includes(s.name.text)).map(s=>s.getText(ast).replace('export ','')).join('\n');
const severity=fs.readFileSync(new URL('../../../packages/logging/src/LogSeverity.ts',import.meta.url),'utf8').replace(/\bexport /g,'');
const code=ts.transpileModule(severity+'\n'+enums+'\n'+['utils/safe-math.ts','webworker/twap/TwapStatus.ts','webworker/twap/TwapManager.ts','webworker/twap/TwapInstance.ts'].map(source).join('\n')+'\nglobalThis.Instance=TwapInstance;globalThis.Manager=twapManager;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const bytes=new DataView(new ArrayBuffer(8));function bits(n){if(Number.isNaN(n))return 'NaN';bytes.setFloat64(0,n);return bytes.getBigUint64(0).toString(16).padStart(16,'0');}
function encode(v){return typeof v==='number'?{$num:bits(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;}
const opts=()=>({accountName:'account',activeMarket:{symbol:'BTC'},side:'buy',size:3,ordersCount:2,duration:1,randomnessPercentage:10,useChaserOrders:false,useLimitOrders:false,reduceOnly:false,chaserDelay:100});
const initial=(status='idle',sizes=[])=>({accountName:'account',activeMarket:{symbol:'BTC'},side:'buy',size:3,status,ordersSizes:sizes,ordersCount:2,ordersExecuted:0,nextOrderAt:2000});
function setup(){let now=1000,next=1,ids=0,rngHook=null,placementMode='ok',release;const pending=new Promise(resolve=>release=resolve);const localMath=Object.create(Math);localMath.random=()=>{if(rngHook)rngHook();return 0.25;};const random=lodash.runInContext({Math:localMath}).random;const timers=new Map(),events=[],reads=[],chasers=[];
 const context=vm.createContext({random,nanoid:()=>`generated-${ids++}`,Date:{now:()=>reads.length?reads.shift():now},setTimeout:(fn,delay)=>{const id=next++;timers.set(id,{fn,delay});events.push({timer:id,delay});return id;},clearTimeout:id=>{timers.delete(id);events.push({clear:id});},ChaserOrderInstance:class{constructor(exchange,options){assert.equal(exchange,venue);assert.deepEqual(Object.keys(options),['symbol','size','side','maxAttempts','reduceOnly','delay']);assert.equal(options.maxAttempts,1000);assert.equal(options.delay,100);assert.equal(options.reduceOnly,false);assert.equal(options.symbol,'BTC');this.id=`generated-${ids++}`;this.remainingSize=options.size;this.isStopped=false;this.options=options;}}});
 const venue={store:{tickers:[]},async placeOrder(order){events.push({order});if(placementMode==='pending-error'){await pending;throw new Error('fixture rejection');}},emitter:{emit:(...emit)=>events.push({emit})}};
 context.chaserOrderManagerInstance={instances:chasers,add:c=>{chasers.push(c);events.push({chaser:c.id,size:c.options.size});},remove:id=>{events.push({removeChaser:id});const i=chasers.findIndex(c=>c.id===id);if(i>=0){chasers[i].remainingSize=0;chasers.splice(i,1);}}};
 vm.runInContext(code,context);const manager=context.Manager;const ctx={postMessage:post=>events.push({post:encode(post)})};
 const worker=(status='idle',sizes=[])=>{const w=new context.Instance(venue,ctx,opts(),s=>events.push({update:encode(s)}),initial(status,sizes),'fixture');manager.add(w);return w;};
 const fire=async id=>{const t=timers.get(id);timers.delete(id);events.push({fired:id});await t.fn();await Promise.resolve();};
 return {context,venue,manager,ctx,events,reads,timers,chasers,worker,fire,ids:()=>ids,rngHook:fn=>{rngHook=fn;},mode:mode=>{placementMode=mode;},release};
}
const c=setup(),o=opts(),st=initial('idle',[1,2]);let calls=0,mode='normal',order=[];
function cb(s){calls++;assert.equal(this,w);assert.equal(s,this.state);order.push('callback');if(mode==='throw')throw new Error('callback failed');if(mode==='replace'){this.state=initial('paused',[9]);c.manager.remove(this.id);}}
let posts=0;c.ctx.postMessage=()=>{posts++;order.push('post');};const w=new c.context.Instance(c.venue,c.ctx,o,cb,st,'');
const emptyId=w.id,stateSame=w.state===st,optionsSame=w.options===o,callbackSame=w.updateStateCallback===cb,constructorCalls=calls;c.manager.add(w);w.updateState({status:'running'});const oldStatus=st.status,callbackBeforePost=order.join(',')==='callback,post';const newer=w.state;newer.ordersSizes.length=0;const arrayAlias=st.ordersSizes.length===0;
mode='replace';let postAfterRemoval=false;c.ctx.postMessage=msg=>{posts++;postAfterRemoval=Object.keys(msg.data).length===0;};w.updateState({nextOrderAt:777});
mode='throw';const prior=posts;assert.throws(()=>w.updateState({markForCleanup:true}),/callback failed/);const callbackFailurePostCount=posts-prior,committedMarker=w.state.markForCleanup;
const white=new c.context.Instance(c.venue,c.ctx,o,cb,undefined,' ');const whitespaceId=white.id;o.ordersCount=Infinity;const before=c.ids();assert.throws(()=>new c.context.Instance(c.venue,c.ctx,o,cb),/Invalid array length/);const idBeforeError=c.ids()===before+1;
const constructor={emptyId,stateSame,optionsSame,callbackSame,constructorCalls,oldStatus,callbackBeforePost,postAfterRemoval,callbackFailurePostCount,committedMarker,arrayAlias,whitespaceId,idBeforeError};
const p=setup(),pw=p.worker('running',[1]);pw.timerId=42;await pw.pause();p.reads.push(1000,1001);pw.state.nextOrderAt=2000;await pw.resume();await pw.pause();p.reads.push(3000,3001,3002);await pw.resume();pw.sliceTimers.push(101,102);await pw.terminate();pw.sliceTimers.push(103);await pw.terminate();await pw.cleanup();assert.equal(p.reads.length,0);const pauseResume=encode(p.events);
const prices=[];const priceInputs=[['buy',12,13,10,true],['sell',12,13,10,true],['buy',0,13,10,true],['sell',12,NaN,0,true],['buy',-12,13,10,true],['sell',12,Infinity,10,true],['buy',NaN,13,-0,true],['buy',12,13,10,false]];
for(const [side,bid,ask,last,present] of priceInputs){const s=setup(),w=s.worker();w.options.useLimitOrders=true;w.options.side=side;s.venue.store.tickers=present?[{symbol:'BTC',bid,ask,last}]:[];await w.placeOrder(3);assert.equal(s.events.length,1);prices.push(encode(s.events));}
const policies=[];for(let index=0;index<12;index++){const s=setup(),w=s.worker();const stopped=index<8&&Math.floor(index/4)===1,timeout=index<8&&index%4>=2?'next':'market',fail=index<8&&index%2===1?'next':'market';Object.assign(w.options,{useChaserOrders:true,useLimitOrders:true,timeoutPct:index===11?NaN:50,timeoutAction:timeout,failAction:fail});await w.placeOrder(index===10?NaN:3);assert.equal(w.trackedChaserOrderIds.length,1);const chaser=s.chasers[0];chaser.isStopped=stopped;if(index===8)s.chasers.length=0;if(index===9)chaser.remainingSize=0;w.options.timeoutAction='next';w.options.failAction='next';await s.fire(w.sliceTimers[0]);assert.equal(w.sliceTimers.length,0);policies.push(encode(s.events));}
const rr=setup();rr.mode('pending-error');const ro=opts();let rw;
function rcb(state){rr.events.push({update:encode(state)});if(state.status==='running'){state.ordersExecuted=7;this.options.activeMarket.symbol='OTHER';}}
rw=new rr.context.Instance(rr.venue,rr.ctx,ro,rcb,initial('idle',[1,2]),'fixture');rr.manager.add(rw);rr.rngHook(()=>{ro.duration=1000;ro.randomnessPercentage=99;ro.activeMarket.symbol='RNG';});const rf=rw.start();assert.equal(rr.events.length,3);await rw.pause();rw.options.activeMarket.symbol='AFTER';rr.release();await rf;const rngRace=encode(rr.events);
const settlement=[];
for (const method of ['resume', 'terminate']) {
 for (const mode of ['active', 'noop', 'post-error']) {
  const s=setup();
  const initialStatus=method==='resume'?(mode==='noop'?'running':'paused'):(mode==='noop'?'completed':'running');
  const w=s.worker(initialStatus,[1]);
  w.timerId=42;w.sliceTimers.push(101,102);w.trackedChaserOrderIds.push('tracked');
  if(mode==='post-error') {
   const post=s.ctx.postMessage;
   s.ctx.postMessage=message=>{post(message);throw new Error('post failed');};
  }
  const order=[];
  const waiter=w[method]().then(()=>order.push('fulfilled'),error=>order.push(error.message));
  const before={events:encode(s.events),status:w.state.status,registered:s.manager.hasId(w.id),timer:w.timerId,slices:[...w.sliceTimers],tracked:[...w.trackedChaserOrderIds]};
  assert.deepEqual(order,[]);
  order.push('returned');
  await waiter;
  assert.deepEqual(order,['returned',mode==='post-error'?'post failed':'fulfilled']);
  settlement.push({method,mode,before,order});
 }
}
assert.equal(settlement.length,6);
assert.equal(prices.length,8);assert.equal(policies.length,12);
process.stdout.write(JSON.stringify({constructor,pauseResume,prices,policies,rngRace,settlement}));
