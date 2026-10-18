// Actual source Manager/Instance; only three pending heavy callees are dependency
// fixtures. This proves their consumers, not cancellation/fill/supervision bodies.
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function source(path){const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);return ast.statements.filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');}
const types=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const enums=types.statements.filter(n=>ts.isEnumDeclaration(n)&&['OrderSide','PositionSide','OrderStatus'].includes(n.name.text)).map(n=>n.getText(types).replace('export ','')).join('\n');
let currentId='id';const context=vm.createContext({Date:{now:()=>0},nanoid:()=>currentId,structuredClone});
const program=ts.transpileModule(enums+'\n'+['utils/clone.ts','webworker/chaserOrder/ChaserOrderOptions.ts','webworker/chaserOrder/ChaserOrderInstance.ts','webworker/chaserOrder/ChaserOrderManager.ts'].map(source).join('\n')+'\nglobalThis.Instance=ChaserOrderInstance;globalThis.Manager=ChaserOrderManager;globalThis.Singleton=chaserOrderManagerInstance;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
vm.runInContext(program,context);

if(process.argv.includes('--composition')){
 const managerAst=ts.createSourceFile('manager',fs.readFileSync(new URL('webworker/chaserOrder/ChaserOrderManager.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
 const declaration=managerAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='ChaserOrderManager');
 assert.equal(declaration.members.filter(ts.isConstructorDeclaration).length,0,'MANAGER source has no constructor effects');
 const fields=declaration.members.filter(ts.isPropertyDeclaration);
 assert.equal(fields.length,1,'MANAGER source instance-field count');
 assert.equal(fields[0].name.getText(managerAst),'_chaserOrderInstances');
 assert.ok(ts.isArrayLiteralExpression(fields[0].initializer));
 assert.equal(fields[0].initializer.elements.length,0,'MANAGER source empty field initializer');
 const bits=value=>{const view=new DataView(new ArrayBuffer(8));view.setFloat64(0,value);return view.getBigUint64(0).toString(16).padStart(16,'0');};
 const singleton=context.Singleton, list=singleton.instances, fresh=new context.Manager();
 let externalCalls=0;
 const isolated=vm.createContext({Date:{now:()=>{externalCalls++;return 0;}},nanoid:()=>{externalCalls++;return 'unused';},structuredClone});
 vm.runInContext(program,isolated);
 assert.equal(externalCalls,0,'MANAGER source construction has no external effects');
 const events=[], exchange={name:'bybit',store:{positions:[],orders:[]},emitter:{emit:(...args)=>events.push(args)}};
 currentId='composition';
 const instance=new context.Instance(exchange,{symbol:'BTC',size:3,side:'buy',maxAttempts:10,delay:100});
 instance.markComplete('user');
 const empty=list.length===0;
 singleton.add(instance);
 const prefix={count:list.length,sameInstance:list[0]===instance,logs:events.filter(([name])=>name==='log').length};
 for(let n=0;n<32;n++)await Promise.resolve();
 assert.equal(events.length,2,'MANAGER source real engine effect count');
 const completed=events.find(([name])=>name==='chaser:complete')[1];
 const composition={empty,sameManager:context.Singleton===singleton,sameList:context.Singleton.instances===list,independent:fresh!==singleton&&fresh.instances!==list&&fresh.instances.length===0,isolated:isolated.Singleton!==singleton&&isolated.Singleton.instances!==list&&isolated.Singleton.instances.length===0,externalCalls,prefix,remaining:list.length,complete:{id:completed.id,symbol:completed.symbol,side:completed.side,reduceOnly:completed.reduceOnly,sizeBits:bits(completed.size),filledBits:bits(completed.filled),stopReason:completed.stopReason},logs:events.filter(([name])=>name==='log').map(([,message])=>message)};
 assert.equal(completed.size,3);
 assert.equal(completed.filled,0);
 process.stdout.write(JSON.stringify(composition));
 process.exit(0);
}

const ErrorClass=vm.runInContext('Error',context),error=m=>new ErrorClass(m);
function fixture(){const manager=new context.Manager(),events=[],settings={process:'ok',released:true,cleanupError:false};let release,cleanupHook;
 function venue(){const exchange={name:'bybit',store:{orders:[],positions:[]},hooks:{},emitter:{emit(e,m){events.push(['emit',e,m]);exchange.hooks[e]?.(m);}}};return exchange;}
 const exchange=venue();
 function instance(id,ex=exchange){currentId=id;return new context.Instance(ex,{symbol:'BTC',size:3,side:'buy',maxAttempts:10,delay:100,reduceOnly:false});}
 manager.processInstance=async i=>{assert.ok(manager.instances.includes(i),'MANAGER source push before prefix');events.push(['process',i.id,manager.instances.length,true]);if(settings.process==='error')throw error('process failed');};
 manager.cleanupInstance=async i=>{events.push(['cleanup',i.id,[...i.currentOrderIds],i.state.stopReason,manager.instances.length]);cleanupHook?.(i);if(!settings.released)await new Promise(r=>release=r);if(settings.cleanupError)throw error('cleanup failed');};
 manager.emitComplete=async i=>{events.push(['complete',i.id,manager.instances.length]);};
 return {manager,events,settings,exchange,venue,instance,cleanupHook:fn=>cleanupHook=fn,release:()=>release?.()};
}
const l=fixture(),lm=l.manager,alias=lm.instances;assert.equal(alias,lm._chaserOrderInstances);assert.equal(alias,lm.instances);assert.equal(alias.length,0);assert.ok(context.Singleton instanceof context.Manager);assert.notEqual(context.Singleton,lm);assert.equal(context.Singleton.instances.length,0);
const a=l.instance('a');lm.add(a);assert.equal(alias[0],a);await Promise.resolve();const b=l.instance('b');l.settings.process='error';lm.add(b);assert.equal(b.isStopped,false);await Promise.resolve();await Promise.resolve();assert.equal(b.state.stopReason,'Chaser startup failed');assert.equal(alias.length,2);l.settings.process='ok';lm.add(a);await Promise.resolve();assert.equal(alias.length,3);assert.equal(alias[2],a);[alias[0],alias[1]]=[alias[1],alias[0]];assert.equal(lm.instances[0],b);
assert.deepEqual(l.events,[['process','a',1,true],['process','b',2,true],['emit','log','[CHASERb] Chaser supervisor failed during startup for BTC: process failed'],['emit','error','Chaser supervisor failed during startup for BTC: process failed'],['cleanup','b',[],'Chaser startup failed',2],['process','a',3,true]]);
const specs=[['error','normal'],['string','normal'],['null','normal'],['array','normal'],['object','normal'],['error','stopped'],['error','log-stop'],['error','log-throw'],['error','error-throw'],['error','cleanup-throw']];assert.equal(specs.length,10);
const supervisor=[];
for(const [kind,mode] of specs){const f=fixture(),i=f.instance('a');f.manager.instances.push(i);f.settings.released=false;if(mode==='stopped')i.markComplete('prior');if(mode==='log-stop')f.exchange.hooks.log=()=>i.markComplete('listener');if(mode==='log-throw')f.exchange.hooks.log=()=>{throw error('log failed');};if(mode==='error-throw')f.exchange.hooks.error=()=>{throw error('error listener failed');};f.settings.cleanupError=mode==='cleanup-throw';
 const value={error:error('fixture'),string:'raw',null:null,array:[1,null,'x'],object:{message:'ignored'}}[kind];const future=f.manager.handleSupervisorError(i,value,'order update');const stopped=i.isStopped,reason=i.state.stopReason;f.settings.released=true;f.release();let failure=null;try{await future;}catch(e){failure=e.message;}
 supervisor.push({kind,mode,events:f.events,stopped,reason,error:failure});
}
const e=fixture(),em=e.manager,foreignEx=e.venue(),ea=e.instance('a'),eb=e.instance('b'),foreign=e.instance('foreign',foreignEx),en=e.instance('new');em.instances.push(ea,foreign,eb,ea);e.exchange.hooks.warning=()=>eb.markComplete('listener stop');let inserted=false;e.cleanupHook(i=>{const n=em.instances.indexOf(i);if(n>=0)em.instances.splice(n,1);if(!inserted){inserted=true;em.instances.push(en);}});em.cleanupExchange(e.exchange);assert.equal(foreign.isStopped,false);assert.equal(en.isStopped,false);assert.equal(eb.state.stopReason,'listener stop');assert.equal(em.instances.length,2);
const fail=fixture(),x=fail.instance('x'),y=fail.instance('y');fail.manager.instances.push(x,y);fail.exchange.hooks.warning=()=>{throw error('warning failed');};assert.throws(()=>fail.manager.cleanupExchange(fail.exchange,'custom'),/warning failed/);assert.equal(x.state.stopReason,'custom');assert.equal(y.isStopped,false);assert.equal(fail.events.length,1);
const exchange={events:e.events,remaining:em.instances.map(i=>i.id),throwStopped:[x.isStopped,y.isStopped],throwEvents:fail.events};
const r=fixture(),first=r.instance('same'),second=r.instance('same');first.currentOrderIds=['o'];r.manager.instances.push(first,second);r.exchange.store.orders=[{id:'o',symbol:'BTC',status:'closed',type:'limit',side:'buy',price:100,amount:3,filled:3,remaining:0,reduceOnly:false,lastUpdate:0}];r.exchange.hooks.log=()=>{const n=r.manager.instances.indexOf(first);if(n>=0)r.manager.instances.splice(n,1);};let selected=false;r.cleanupHook(i=>selected=i===first);r.manager.remove('missing');assert.equal(r.events.length,0);r.manager.remove('same');assert.equal(selected,true);assert.equal(second.isStopped,false);assert.equal(first.state.stopReason,'Completed');assert.equal(first.currentOrderIds.length,0);
const k=fixture(),p=k.instance('same'),q=k.instance('same'),z=k.instance('z');p.currentOrderIds=['live'];k.manager.instances.push(p,p,q,z);p.registerDisposer(()=>{assert.equal(p.currentOrderIds.length,0);k.events.push(['dispose',k.manager.instances.length,p.currentOrderIds.length]);k.manager.instances.splice(0,1);k.manager.instances.reverse();});k.manager.removeKeepOrder('missing');assert.equal(k.events.length,0);k.manager.removeKeepOrder('same');assert.equal(p.state.stopReason,'User cancelled, kept order in book');assert.equal(q.isStopped,false);assert.equal(k.manager.instances.length,2);assert.equal(k.manager.instances[0],z);assert.equal(k.manager.instances[1],q);assert.equal(p.claimCompleteEmission(),false);k.manager.instances.length=0;k.manager.instances.push(p);k.manager.removeKeepOrder('same');assert.equal(k.manager.instances.length,0);assert.deepEqual(k.events,[['dispose',4,0],['complete','same',2]]);
process.stdout.write(JSON.stringify({list:l.events,supervisor,exchange,removal:{remove:r.events,keep:k.events}}));
