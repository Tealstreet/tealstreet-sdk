// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const entry=require.resolve(process.env.SAFE_CEX_WORKER_LODASH_PATH?process.env.SAFE_CEX_WORKER_LODASH_PATH+'/lodash.js':'lodash');
assert.equal(crypto.createHash('sha256').update(fs.readFileSync(entry)).digest('hex'),'f5465f55566bf544aad0a31c6135889ca1ed81eea8f53ec61c6cbe86926f07cf','installed lodash fingerprint');
const lodash=require(entry),math=Object.create(Math);math.random=()=>0.25;
assert.equal(lodash.VERSION,'4.18.1');
const random=lodash.runInContext({Math:math}).random;
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const ast=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=ast.statements.filter(s=>ts.isEnumDeclaration(s)&&['OrderSide','OrderType','LogSeverity'].includes(s.name.text)).map(s=>s.getText(ast).replace('export ','')).join('\n');
const bytes=new DataView(new ArrayBuffer(8));
function bits(n){if(Number.isNaN(n))return 'NaN';bytes.setFloat64(0,n);return bytes.getBigUint64(0).toString(16).padStart(16,'0');}
function encode(v){return typeof v==='number'?{$num:bits(v)}:Array.isArray(v)?v.map(encode):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,encode(x)])):v;}
async function run(status,sizes,outcome,interruption){
 let now=1000,nextTimer=0,release;
 const timers=new Map(),events=[];
 const pending=new Promise(resolve=>{release=resolve;});
 const context=vm.createContext({random,nanoid:()=> 'generated',Date:{now:()=>now},setTimeout(fn,delay){const id=++nextTimer;timers.set(id,{fn,delay});events.push({timer:id,delay});return id;},clearTimeout(id){events.push({clear:id});timers.delete(id);}});
 const severity=fs.readFileSync(new URL('../../../packages/logging/src/LogSeverity.ts',import.meta.url),'utf8').replace(/\bexport /g,'');
 vm.runInContext(ts.transpileModule(severity+'\n'+enums+'\n'+['utils/safe-math.ts','webworker/twap/TwapStatus.ts','webworker/twap/TwapManager.ts','webworker/twap/TwapInstance.ts'].map(source).join('\n')+'\nglobalThis.Instance=TwapInstance;globalThis.Manager=twapManager;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 const options={accountName:'account',activeMarket:{symbol:'BTC'},side:'buy',size:3,ordersCount:2,duration:1,randomnessPercentage:10,useChaserOrders:false,useLimitOrders:false,reduceOnly:false};
 const state={accountName:'account',activeMarket:options.activeMarket,side:'buy',size:3,status,ordersSizes:[...sizes],ordersCount:2,ordersExecuted:0,nextOrderAt:2000};
 const exchange={store:{tickers:[]},async placeOrder(order){events.push({order});if(outcome==='pending')await pending;if(outcome==='error')throw new Error('fixture rejection');},emitter:{emit(...args){events.push({emit:args});}}};
 const instance=new context.Instance(exchange,{postMessage(message){events.push({post:encode(message)});}},options,value=>events.push({update:encode(value)}),state,'fixture');
 context.Manager.add(instance);
 const start=instance.start();
 await new Promise(resolve=>setImmediate(resolve));
 const before={state:encode(instance.state),events:encode(events),timers:timers.size,registered:context.Manager.hasId('fixture')};
 if(interruption==='pause')await instance.pause();
 if(interruption==='terminate')await instance.terminate();
 release();const result=await start;
 const settled={result:result??'undefined',state:encode(instance.state),events:encode(events),timers:timers.size,registered:context.Manager.hasId('fixture')};
 const firstTimer=timers.entries().next().value;
 if(firstTimer){const [id,timer]=firstTimer;timers.delete(id);now+=timer.delay;events.push({fired:id});await timer.fn();}
 const fired={state:encode(instance.state),events:encode(events),timers:timers.size,registered:context.Manager.hasId('fixture')};
 await instance.cleanup();
 const cleanup={state:encode(instance.state),events:encode(events)};
 return {status,sizes:sizes.map(bits),outcome,interruption,before,settled,fired,cleanup};
}
const rows=[];
for(const status of ['idle','running','paused','completed'])for(const sizes of [[],[0,2],[-0,2],[NaN,2],[Infinity,2],[1,2],[1]])for(const outcome of ['ok','error','pending'])for(const interruption of ['none','pause','terminate'])rows.push(await run(status,sizes,outcome,interruption));
const pendingPause=rows.find(r=>r.status==='idle'&&r.sizes[0]===bits(1)&&r.sizes.length===2&&r.outcome==='pending'&&r.interruption==='pause');
assert.equal(pendingPause.before.state.status,'running','PENDING-START reaches placement before settling');
assert.equal(pendingPause.settled.state.status,'paused','SOURCE-RACE pause status survives pending slice');
assert.equal(pendingPause.settled.timers,1,'SOURCE-RACE pending start rearms a timer after pause');
const pendingTerminate=rows.find(r=>r.status==='idle'&&r.sizes[0]===bits(1)&&r.sizes.length===2&&r.outcome==='pending'&&r.interruption==='terminate');
assert.equal(pendingTerminate.settled.state.status,'completed','SOURCE-RACE completed status survives pending slice');
assert.equal(pendingTerminate.settled.registered,false,'SOURCE-RACE terminate removes manager membership');
assert.equal(pendingTerminate.settled.timers,1,'SOURCE-RACE pending start rearms a timer after terminate');
assert.equal(pendingTerminate.fired.events.filter(e=>e.fired).length,1,'SOURCE-RACE timer actually fires after terminate');
assert.equal(pendingTerminate.fired.state.status,'completed','SOURCE-RACE completed callback returns without placing');
assert.equal(pendingTerminate.fired.events.filter(e=>e.order).length,1,'SOURCE-RACE terminated callback places no additional order');
assert.equal(pendingPause.fired.events.filter(e=>e.fired).length,1,'SOURCE-RACE timer actually fires after pause');
assert.equal(pendingPause.fired.state.status,'paused','SOURCE-RACE paused callback returns without placing');
for(const row of rows.filter(r=>r.status==='paused'||r.status==='completed'))assert.equal(row.before.events.length,0,'START-GUARD paused and completed have no effects');
for(const row of rows){
 const active=row.status==='idle'||row.status==='running';
 const places=active&&row.sizes.length>0&&!['0000000000000000','8000000000000000','NaN'].includes(row.sizes[0]);
 const expected=places?1:0;
 assert.equal(row.before.events.filter(e=>e.order).length,expected,'SLICE-TRUTHINESS before order count');
 assert.equal(row.settled.events.filter(e=>e.order).length,expected,'SLICE-TRUTHINESS settled order count');
 assert.equal(row.settled.state.ordersExecuted.$num,bits(expected),'SLICE-TRUTHINESS execution count');
 if(places)assert.equal(row.settled.events.find(e=>e.order).order.amount.$num,row.sizes[0],'SLICE-AMOUNT retains nonfinite or finite input bits');
 if(active&&row.sizes.length===0){assert.equal(row.settled.timers,0,'EMPTY-SLICE does not schedule');assert.equal(row.settled.registered,false,'EMPTY-SLICE removes membership');}
}
for(const row of rows)assert.equal(row.cleanup.state.markForCleanup,true,'CLEANUP marks even completed tasks');
assert.equal(pendingPause.before.state.ordersExecuted.$num,bits(0),'PENDING-START does not advance execution before placement');
assert.equal(pendingPause.settled.state.ordersExecuted.$num,bits(1),'SLICE-COUNT increments after pending placement');
const rejected=rows.find(r=>r.status==='idle'&&r.sizes[0]===bits(1)&&r.sizes.length===2&&r.outcome==='error'&&r.interruption==='none');
assert.equal(rejected.settled.state.ordersExecuted.$num,bits(1),'SLICE-COUNT includes rejected placement');
assert.deepEqual(rejected.settled.events.find(e=>e.emit).emit,['log','TWAP BTC slice failed: fixture rejection','error'],'REJECTION-LOG preserves source severity and message');
process.stdout.write(JSON.stringify({rows}));
