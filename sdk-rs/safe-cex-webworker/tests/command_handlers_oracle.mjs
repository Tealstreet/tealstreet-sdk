// Execute all three actual handler sources, actual BaseHandler/TwapInstance/TwapManager.
// Chaser constructor/manager and venue/store are dependency fixtures; native assertions
// separately inspect the real Rust chaser and actual native DefaultStore fields.
import fs from 'node:fs'; import vm from 'node:vm'; import assert from 'node:assert/strict'; import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),lodash=require('lodash');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const types=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=types.statements.filter(s=>ts.isEnumDeclaration(s)&&['OrderSide','OrderType','PositionSide','PositionMode'].includes(s.name.text)).map(s=>s.getText(types).replace('export ','')).join('\n');
const severity=fs.readFileSync(new URL('../../../packages/logging/src/LogSeverity.ts',import.meta.url),'utf8').replace(/\bexport /g,'');
const code=ts.transpileModule(severity+'\n'+enums+'\n'+['utils/safe-math.ts','webworker/twap/TwapStatus.ts','webworker/twap/TwapManager.ts','webworker/twap/TwapInstance.ts','webworker/handlers/BaseHandler.ts','webworker/handlers/ChaserOrderHandler.ts','webworker/handlers/TwapHandler.ts','webworker/handlers/UiStateHandler.ts'].map(source).join('\n')+'\nglobalThis.classes={ChaserOrderHandler,TwapHandler,UiStateHandler};globalThis.manager=twapManager;globalThis.TwapInstance=TwapInstance;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const opts=()=>({accountName:'account',activeMarket:{symbol:'BTC'},size:3,duration:1,ordersCount:2,side:'buy',chaserDelay:100,reduceOnly:false,randomnessPercentage:0,useLimitOrders:false,useChaserOrders:false});
const state=status=>({accountName:'account',activeMarket:{symbol:'BTC'},size:3,ordersSizes:[1,2],side:'buy',status,ordersCount:2,ordersExecuted:0,nextOrderAt:2000});
const restored=(id,status)=>({id,options:opts(),state:state(status)});
function setup(){let now=1000,next=1,ids=0;const timers=new Map(),events=[],chasers=[],posts=[],logs=[];
 const math=Object.create(Math);math.random=()=>0.25;
 const context=vm.createContext({random:lodash.runInContext({Math:math}).random,nanoid:()=>`generated-${ids++}`,Date:{now:()=>now},console:{log:(...a)=>logs.push(['log',a]),error:(...a)=>logs.push(['error',a])},setTimeout:(fn,delay)=>{const n=next++;timers.set(n,{fn,at:now+delay});return n;},clearTimeout:n=>timers.delete(n),ChaserOrderInstance:class{constructor(exchange,options){assert.equal(exchange,venue);this.id=`generated-${ids++}`;this.options=options;}}});
 const venue={name:'bybit',store:{markets:[],tickers:[],fetchedLeverage:{},positions:[],update(p){Object.assign(this,p);},updatePositions(updates){for(const [where,patch] of updates){Object.assign(this.positions.find(p=>p.id===where.id),patch);}}},getActiveTickerSymbols:()=>venue.activeSymbols||[],setActiveTickerSymbols(symbols){venue.activeSymbols=symbols;if(venue.onSymbols)venue.onSymbols();},async placeOrder(){events.push('order');return [];},emitter:{emit(){}}};
 context.chaserOrderManagerInstance={instances:chasers,add(c){events.push('chaser:add');chasers.push(c);},remove(id){events.push(`chaser:remove:${id}`);const i=chasers.findIndex(c=>c.id===id);if(i>=0)chasers.splice(i,1);},cleanupExchange(e,r){assert.equal(e,venue);events.push(`cleanup:${r}`);}};
 vm.runInContext(code,context);const ctx={postMessage:p=>posts.push(p)},metadata={fetchLeverageOnMarginSymbolChange:true},settings={};
 const handler=name=>new context.classes[name]('account','bybit',venue,ctx,settings,metadata);
 return {context,venue,ctx,metadata,settings,handler,events,chasers,posts,logs,timers,advance(t){now=t;for(const [n,task] of [...timers])if(task.at<=now){timers.delete(n);task.fn();}}};
}
const b=setup(),classes=[];
for(const name of ['ChaserOrderHandler','TwapHandler','UiStateHandler']){
 const h=b.handler(name);assert.equal(h._exchange,b.venue);assert.equal(h._ctx,b.ctx);assert.equal(h._settings,b.settings);assert.equal(h._metadata,b.metadata);assert.equal(h._accountName,'account');assert.equal(h._exchangeName,'bybit');
 assert.equal(h.bufferUntilMarketsLoaded({op:'ui:state',data:{}}),true);classes.push(h);
}
b.advance(31000);assert.equal(b.posts.length,3,'HANDLER source independent class timeout floor');const classNames=b.posts.map(p=>p.data[1].sentryData.tags.worker_handler);assert.deepEqual(classNames,['ChaserOrderHandler','TwapHandler','UiStateHandler']);classes.forEach(h=>h.dispose());
const r=setup(),rch=r.handler('ChaserOrderHandler'),rtw=r.handler('TwapHandler'),rui=r.handler('UiStateHandler');
rch.bufferUntilMarketsLoaded({op:'chaser:terminate',data:'buffered'});rtw.bufferUntilMarketsLoaded({op:'twap:state',data:[restored('replay','idle')]});rui.bufferUntilMarketsLoaded({op:'ui:state',data:{activeSymbols:['BTC']}});r.venue.store.markets=[{symbol:'BTC'}];r.advance(1500);assert.equal(r.posts.length,0);const replay={chaser:r.events[0]==='chaser:remove:buffered',twap:r.context.manager.runningTwaps.has('replay'),ui:r.venue.activeSymbols[0]==='BTC'};assert.deepEqual(replay,{chaser:true,twap:true,ui:true});
const c=setup(),ch=c.handler('ChaserOrderHandler');
for(const [i,a] of [undefined,'',' ','explicit'].entries()){ch._accountName=`live-${i}`;const options={symbol:'BTC',size:2,side:'sell',maxAttempts:1000,delay:70,reduceOnly:true};if(a!==undefined)options.accountName=a;ch.handle({op:'chaser:start',data:options});}
const accounts=c.chasers.map(c=>c.options.accountName);assert.deepEqual(accounts,['live-0','live-1',' ','explicit']);ch.handle({op:'chaser:terminate',data:'generated-1'});ch.handle({op:'chaser:terminateAll',data:null});ch.handle({op:'twap:pause',data:'unused'});assert.equal(c.chasers.length,3);
const t=setup(),tw=t.handler('TwapHandler'),manager=t.context.manager,realAdd=manager.add.bind(manager);let replace=false,rejections=0;
manager.add=w=>{t.events.push(`add:${w.id}`);for(const name of ['pause','resume']){const method=w[name];w[name]=function(...args){const p=method.apply(this,args);p.catch(()=>rejections++);return p;};}const r=realAdd(w);if(replace)w.state={...w.state,status:'paused'};return r;};
tw.handle({op:'twap:start',data:opts()});assert.deepEqual(t.events,['add:generated-0','order']);await Promise.resolve();const w=manager.getById('generated-0');assert.equal(w.state.status,'running');
for(const [op,status] of [['twap:pause','paused'],['twap:resume','running'],['twap:cleanup','running'],['twap:stop','completed']]){tw.handle({op,data:w.id});assert.equal(w.state.status,status);}tw.handle({op:'twap:pause',data:'missing'});
tw.handle({op:'twap:state',data:[restored('dup','idle')]});replace=false;
// Track real async method rejections without replacing their synchronous prefixes.

t.ctx.postMessage=()=>{throw new Error('post failed');};
tw.handle({op:'twap:state',data:[{id:'dup',options:null,state:null},restored('run','running'),{id:'broken',options:opts()},restored('after','idle')]});await Promise.resolve();
replace=true;t.ctx.postMessage=()=>{};tw.handle({op:'twap:state',data:[restored('swapped','running')]});assert.equal(manager.getById('swapped').state.status,'running');tw.handle({op:'twap:state',data:[restored('','idle'),restored('','idle')]});const ids=[...manager.runningTwaps.keys()];assert.deepEqual(ids,['dup','run','broken','swapped','generated-1','generated-2']);assert.equal(manager.getById('run').state.status,'running');assert.equal(rejections,2);const errors=t.logs.filter(([level])=>level==='error').length;assert.equal(errors,1);
const u=setup(),ui=u.handler('UiStateHandler');u.venue.store.positions=[{id:'p',symbol:'BTC',side:'long',leverage:5}];u.metadata.fetchLeverageOnMarginSymbolChange=false;
u.venue.onSymbols=()=>{u.events.push('symbols');u.metadata.fetchLeverageOnMarginSymbolChange=true;};let release;const pending=new Promise(r=>release=r);u.venue.fetchLeverageForSymbol=s=>{u.events.push(`fetch:${s}`);return pending;};
ui.handle({op:'ui:state',data:{activeSymbols:['BTC','',7,'BTC'],activeMarginSymbol:'BTC'}});assert.equal(u.venue.store.fetchedLeverage.BTC.isFetching,true);const before=u.venue.store.positions[0].leverage;release({buyLeverage:11,sellLeverage:13});await pending;await Promise.resolve();const after=u.venue.store.positions[0].leverage;ui.handle({op:'twap:pause',data:{activeSymbols:['OTHER'],activeMarginSymbol:'OTHER'}});
const f=setup();let failureFetches=0;f.venue.onSymbols=()=>{throw new Error('symbols failed');};f.venue.fetchLeverageForSymbol=()=>{failureFetches++;return Promise.resolve(null);};assert.throws(()=>f.handler('UiStateHandler').handle({op:'ui:state',data:{activeSymbols:['BTC'],activeMarginSymbol:'BTC'}}),/symbols failed/);assert.equal(failureFetches,0);
const ready=setup();ready.venue.store.positions=[{id:'p',symbol:'BTC',side:'long',leverage:5}];ready.venue.fetchLeverageForSymbol=()=>Promise.resolve({buyLeverage:11,sellLeverage:13});ready.handler('UiStateHandler').handle({op:'ui:state',data:{activeMarginSymbol:'BTC'}});const readyBeforeMicrotask=ready.venue.store.positions[0].leverage;await Promise.resolve();const readyAfterMicrotask=ready.venue.store.positions[0].leverage;assert.equal(readyBeforeMicrotask,5);assert.equal(readyAfterMicrotask,11);
process.stdout.write(JSON.stringify({replay,classes:classNames,chaser:{accounts,events:c.events},twap:{ids,runStatus:manager.getById('run').state.status,errors,rejections},ui:{events:u.events,before,after,failureFetches,readyBeforeMicrotask,readyAfterMicrotask}}));
