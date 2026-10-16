import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const path=new URL('../../../packages/safe-cex/src/webworker/store.webworker.ts',import.meta.url);
const ast=ts.createSourceFile(path.pathname,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const stop=ast.statements.findIndex(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(ast)==='start'));
assert.ok(stop>0,'PORT source helper prefix found');
const prefix=ast.statements.slice(0,stop).filter(s=>!ts.isImportDeclaration(s)).map(s=>s.getText(ast)).join('\n').replace(/\bexport /g,'');
const source=ts.transpileModule(prefix+'\nglobalThis.api={ports,portsBySubId,portStates,workerConfigHandlers,registerWorkerConfigHandler,cleanupPortState,cleanupPortEverywhere,deliverToPortWithThrottle,broadcastWithThrottle};',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
function setup(){
 let now=0,next=1;const timers=new Map(),effects=[],posts=[],labels=new Map();
 const context=vm.createContext({Date:{now:()=>now},setTimeout:(fn,delay)=>{const id=next++;timers.set(id,fn);effects.push(['set',id,delay]);return id;},clearTimeout:id=>{timers.delete(id);effects.push(['clear',id,0]);}});
 vm.runInContext(source,context);const r=context.api;
 const port=name=>{const p={fail:false,hook:null,postMessage(m){posts.push([name,labels.get(m)]);if(p.hook)p.hook(m);if(p.fail)throw new Error('closed');}};return p;};
 const msg=(label,...supplied)=>{const value=supplied.length?supplied[0]:{};labels.set(value,label);return value;};
 const hidden=p=>{let st=r.portStates.get(p);if(!st){st={visible:true,throttleEnabled:false,lastDeliveryTime:0,pendingUpdate:null,throttleTimer:null};r.portStates.set(p,st);}st.visible=false;st.throttleEnabled=true;return st;};
 const fire=id=>{const fn=timers.get(id);assert.ok(fn,'PORT source pending timer');timers.delete(id);fn();};
 return {r,port,msg,hidden,fire,timers,effects,posts,now:x=>{now=x;},next:x=>{next=x;},profile:()=>({posts:structuredClone(posts),timers:structuredClone(effects)})};
}
const conf=setup(),calls=[],a=cfg=>calls.push(cfg.value),b=cfg=>calls.push(-cfg.value);
conf.r.registerWorkerConfigHandler(a);conf.r.registerWorkerConfigHandler(a);conf.r.registerWorkerConfigHandler(b);
assert.equal(conf.r.workerConfigHandlers.length,3);const cfg={value:17};for(const h of conf.r.workerConfigHandlers)h(cfg);cfg.value=42;for(const h of conf.r.workerConfigHandlers)h(cfg);
const config={ids:Array.from(conf.r.workerConfigHandlers,h=>h===a?'a':'b'),calls};
const s=setup(),p=s.port('p');s.r.deliverToPortWithThrottle(p,s.msg('a'));const state=s.hidden(p);
s.now(3000);p.hook=()=>assert.equal(state.lastDeliveryTime,3000);s.r.deliverToPortWithThrottle(p,s.msg('b'));p.hook=null;
s.now(3100);s.r.deliverToPortWithThrottle(p,s.msg('c'));s.now(3200);s.r.deliverToPortWithThrottle(p,s.msg('d'));
p.hook=()=>{assert.equal(state.lastDeliveryTime,3000);s.now(5501);};s.now(5500);s.fire(1);p.hook=null;
s.now(5600);s.r.deliverToPortWithThrottle(p,s.msg('e'));state.visible=true;s.now(5700);s.r.deliverToPortWithThrottle(p,s.msg('f'));
state.visible=false;state.throttleEnabled=false;s.r.deliverToPortWithThrottle(p,s.msg('g'));state.throttleEnabled=true;s.now(6000);s.r.deliverToPortWithThrottle(p,s.msg('h'));
const late=s.timers.get(3);s.r.cleanupPortState(p);assert.equal(state.throttleTimer,3);const replacement=s.hidden(p);replacement.pendingUpdate=s.msg('j');s.now(8000);late();assert.equal(state.lastDeliveryTime,5501);assert.equal(replacement.lastDeliveryTime,8000);
const timeline=s.profile();assert.equal(timeline.posts.length,6);assert.equal(timeline.timers.length,5);
const values=[['undefined',undefined,false],['null',null,false],['false',false,false],['zero',0,false],['negzero',-0,false],['nan',NaN,false],['empty','',false],['true',true,true],['inf',Infinity,true],['lone','\ud800',true],['object',{},true],['array',[],true]];
const truthiness=values.map(([label,value,truthy])=>{const s=setup(),p=s.port('p'),st=s.hidden(p);s.now(100);s.r.deliverToPortWithThrottle(p,s.msg(label,value));s.now(2500);s.fire(1);assert.equal(s.posts.length,Number(truthy));assert.equal(st.throttleTimer,null);return {label,delivered:!!s.posts.length,pending:Object.is(st.pendingUpdate,value),last:st.lastDeliveryTime};});
assert.equal(truthiness.length,12);
const z=setup(),zp=z.port('p'),zs=z.hidden(zp);z.next(0);z.now(-100);z.r.deliverToPortWithThrottle(zp,z.msg('a'));z.r.deliverToPortWithThrottle(zp,z.msg('b'));z.now(2600);z.fire(0);assert.equal(zs.throttleTimer,null);z.fire(1);assert.equal(z.posts.length,1);
const zc=setup(),zcp=zc.port('p');zc.hidden(zcp);zc.next(0);zc.r.deliverToPortWithThrottle(zcp,zc.msg('z'));zc.r.cleanupPortState(zcp);assert.equal(zc.effects.length,1);
const r=setup(),ra=r.port('a'),rb=r.port('b'),rc=r.port('c'),held=[ra,ra,rb];r.r.ports.group=held;r.r.portsBySubId.cross=rc;const m=r.msg('m');
r.r.broadcastWithThrottle('missing',m,'cross');assert.equal(r.posts.length,0);r.r.broadcastWithThrottle('group',m,'cross');r.r.broadcastWithThrottle('group',m,'unknown');r.r.broadcastWithThrottle('group',m,'');
let once=false;ra.hook=()=>{if(!once){once=true;held.push(rc);}};r.r.broadcastWithThrottle('group',m);ra.hook=null;assert.equal(r.posts.at(-1)[0],'c');
ra.fail=true;r.r.portsBySubId.gone=ra;const only=[ra];r.r.ports.only=only;r.r.ports.empty=[];r.r.broadcastWithThrottle('group',m);assert.equal(r.posts.length,15);assert.equal(held.length,4);assert.equal(only.length,1);assert.deepEqual(Array.from(r.r.ports.group),[rb,rc]);
const f=setup(),fp=f.port('same'),ft=f.port('same');f.r.ports.g=[fp,ft,fp];const fo=f.hidden(fp);fp.fail=true;f.now(3000);f.r.deliverToPortWithThrottle(fp,f.msg('lead-fail'));assert.equal(fo.lastDeliveryTime,3000);assert.deepEqual(Array.from(f.r.ports.g),[ft]);
const t=setup(),tp=t.port('tail');t.r.ports.g=[tp];const to=t.hidden(tp);t.r.deliverToPortWithThrottle(tp,t.msg('tail-fail'));tp.fail=true;t.now(2500);t.fire(1);assert.equal(to.lastDeliveryTime,0);assert.equal(to.throttleTimer,1);assert.equal(t.r.portStates.has(tp),false);
const re=setup(),rep=re.port('reentrant'),reo=re.hidden(rep);re.r.deliverToPortWithThrottle(rep,re.msg('x'));let ren;
let replacementMessage;rep.hook=()=>{re.r.cleanupPortState(rep);ren=re.hidden(rep);ren.lastDeliveryTime=777;replacementMessage=re.msg('replacement');ren.pendingUpdate=replacementMessage;};re.now(2500);re.fire(1);assert.equal(reo.lastDeliveryTime,2500);assert.equal(ren.lastDeliveryTime,777);assert.equal(ren.pendingUpdate,replacementMessage);
let prototypeFailure=false;
// Inherited constructor is selected as the port; postMessage throws, then cleanup
// absorbs it. Observe the selected non-port via its newly-created-and-deleted state.
// A getter on the inherited object establishes this callsite without replacing helper code.
const contextProto=vm.createContext({Date:{now:()=>0},setTimeout:()=>1,clearTimeout:()=>{}});vm.runInContext(source,contextProto);
vm.runInContext(`api.ports.group=[{postMessage(){globalThis.delivered=true;}}];Object.defineProperty(Object,'postMessage',{get(){globalThis.prototypeSelected=true;throw new Error('prototype');},configurable:true});api.broadcastWithThrottle('group',{},'constructor');`,contextProto);
prototypeFailure=!!contextProto.prototypeSelected&&!contextProto.delivered;
process.stdout.write(JSON.stringify({config,timeline,truthiness,zeroTimer:z.profile(),zeroCleanup:zc.profile(),routing:{broadcast:r.profile(),leadingFailure:f.profile(),trailingFailure:t.profile(),reentrant:re.profile()},prototypeFailure}));
