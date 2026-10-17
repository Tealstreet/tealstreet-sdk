'use strict';
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const src = path.join(root, 'packages/safe-cex/src');
const cache = new Map();
function load(file) {
  if (cache.has(file)) return cache.get(file);
  assert.ok(file.startsWith(src + path.sep) || file === path.join(root, 'packages/logging/src/LogSeverity.ts'), `source outside safe-cex: ${file}`);
  const source = fs.readFileSync(file, 'utf8');
  const exports = {};
  cache.set(file, exports);
  const result = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
    reportDiagnostics: true,
  });
  assert.equal((result.diagnostics || []).filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  vm.runInNewContext(result.outputText, { exports, require(name) {
    if (name === '@tealstreet/logging/src/LogSeverity') return load(path.join(root, 'packages/logging/src/LogSeverity.ts'));
    assert.ok(name.startsWith('.'), `unexpected runtime dependency ${name}`);
    return load(path.resolve(path.dirname(file), name) + '.ts');
  } }, { filename: file });
  return exports;
}

const crypto=require('node:crypto');
const hashes={};
function method(file,className,name,bindings,receiver) {
 const source=fs.readFileSync(file,'utf8');
 hashes[path.basename(file)]=crypto.createHash('sha256').update(source).digest('hex');
 const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const classes=ast.statements.filter(s=>ts.isClassDeclaration(s)&&s.name.text===className);
 assert.equal(classes.length,1,'actual class selected');
 const members=classes[0].members.filter(m=>m.name?.getText(ast)===name);
 assert.equal(members.length,1,`actual method ${name}`);
 assert.ok(ts.isPropertyDeclaration(members[0])&&ts.isArrowFunction(members[0].initializer),'actual arrow body selected');
 const result=ts.transpileModule(`function make(){return (${members[0].initializer.getText(ast)});}`,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
 assert.equal((result.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 return vm.runInNewContext(result.outputText+';make',bindings,{filename:file}).call(receiver);
}
function capture(v){
 if(v===undefined)return {undefined:true};
 if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return {number:Number.isNaN(v)?'7ff8000000000000':b.toString('hex')};}
 if(Array.isArray(v))return v.map(capture);
 if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,x])=>[k,capture(x)]));
 return v;
}
const types=load(path.join(src,'types.ts'));
const futuresTypes=load(path.join(src,'exchanges/kraken/kraken.types.ts'));
const spotTypes=load(path.join(src,'exchanges/kraken/krakenspot.types.ts'));
const getKeyFile=path.join(src,'utils/get-key.ts'),getKeyText=fs.readFileSync(getKeyFile,'utf8');hashes[path.basename(getKeyFile)]=crypto.createHash('sha256').update(getKeyText).digest('hex');
const getKeyAst=ts.createSourceFile(getKeyFile,getKeyText,ts.ScriptTarget.Latest,true),decl=getKeyAst.statements.filter(ts.isVariableStatement).flatMap(statement=>[...statement.declarationList.declarations]).find(declaration=>declaration.name.getText(getKeyAst)==='v');assert.ok(decl);
const requireLodash=name=>require(path.join(root,'node_modules/lodash',name));
const getV=vm.runInNewContext(ts.transpileModule('function make(){return ('+decl.initializer.getText(getKeyAst)+')}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText+';make()', {s:requireLodash('memoize')(requireLodash('snakeCase')),c:requireLodash('memoize')(requireLodash('camelCase'))});
const createPositionId=load(path.join(src,'utils/createPositionId.ts')).createPositionId;
const configs=require('./composition-fixtures.cjs');
let rejectionObserver;process.on('unhandledRejection',error=>{if(rejectionObserver)rejectionObserver(error);else{console.error(error);process.exitCode=1;}});
const grafana=spotTypes;
function shape(receiver){return Object.getOwnPropertyNames(receiver).map(key=>{const d=Object.getOwnPropertyDescriptor(receiver,key),v=d.value;return {key,enumerable:d.enumerable,configurable:d.configurable,writable:d.writable,kind:typeof v==='function'?'function':Object.prototype.toString.call(v)==='[object Map]'?'map':'value',value:typeof v==='function'?v.name:Object.prototype.toString.call(v)==='[object Map]'?v.size:capture(v)};});}
const files={KrakenExchange:'kraken.exchange.ts',KrakenSpotExchange:'krakenspot.exchange.ts',KrakenPublicWebsocket:'kraken.ws-public.ts',KrakenSpotPublicWebsocket:'krakenspot.ws-public.ts',KrakenPrivateWebsocket:'kraken.ws-private.ts',KrakenSpotPrivateWebsocket:'krakenspot.ws-private.ts'};
function declaration(name,bindings){const file=path.join(src,'exchanges/kraken',files[name]),source=fs.readFileSync(file,'utf8');hashes[files[name]]=crypto.createHash('sha256').update(source).digest('hex');const ast=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);const cls=ast.statements.filter(s=>ts.isClassDeclaration(s)&&s.name.text===name);assert.equal(cls.length,1);const result=ts.transpileModule(cls[0].getText(ast),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal((result.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);return {Class:vm.runInNewContext(result.outputText+`;exports.${name}`,{exports:{},...bindings},{filename:file}),ast,cls:cls[0]};}
function environment(raw){
 const cfg=structuredClone(raw),events=[],original=new Error('read failure');cfg.parent={...cfg.parent,store:cfg.store,options:cfg.options};let current,baseCount=0,apiCount=0;
 const effect=(kind,details={})=>{events.push({kind,...details});if(cfg.effectFail===kind)throw original;};
 const store=cfg.store;
 store.updateOrder=(selector,patch)=>{effect('updateOrder',{selector,patch});const o=store.orders.find(o=>o.id===selector.id);if(o)Object.assign(o,patch);};
 store.updateOrderStatus=(selector,status)=>{effect('status',{selector,status});const o=store.orders.find(o=>o.id===selector.id);if(o)o.status=status;};
 store.addOrUpdateOrder=order=>{effect('addOrder',{order,infoIdentity:false});const i=store.orders.findIndex(o=>o.id===order.id);if(i<0)store.orders.push(order);else store.orders[i]=order;};
 cfg.parent.emitter={emit(name,payload){assert.equal(name,'fill');effect('fill',{payload});}};
 class Base {
  trackLastReceived(){effect('received');}
  recordMessageHandlerStats(feed){effect('stats',{feed});}
  dispose(){effect('baseDispose',{baseId:this.baseId});this.isDisposed=true;}
  createXhrWithSecondaryKey(options,factory,rate){assert.equal(factory,createAPI,'original imported factory supplied');effect('secondary',{optionsIdentity:options===cfg.options,initialized:this.__ClassFields.every(k=>Object.hasOwn(this,k)),xhrUndefined:this.xhr===undefined});const client=factory(cfg.secondaryOptions||options,this);return rate.rateLimit(client);}
 }
 class BaseSocket extends Base {constructor(parent){super();effect('superSocket',{parentIdentity:parent===cfg.parent});this.baseId=++baseCount;this.parent=parent;this.store=parent.store;this.isDisposed=false;Object.assign(this,cfg.seed||{});current=this;}}
 class BaseExchange extends Base {constructor(name,options,store){super();effect('superExchange',{nameIdentity:name===cfg.name,optionsIdentity:options===cfg.options,storeIdentity:store===cfg.store});this.baseId=++baseCount;this.exchangeName=name;this.options=options;this.store=store;this.isDisposed=false;this.publicWebsocket=cfg.publicSocket;this.privateWebsocket=cfg.privateSocket;Object.assign(this,cfg.seed||{});current=this;}}
 function createAPI(options,receiver,base){const id=apiCount++;effect('api',{spot:cfg.spot,base:base==='history'?'history':'derivatives',options,receiverIdentity:receiver===current,xhrUndefined:receiver.xhr===undefined,unlimitedUndefined:receiver.unlimitedXHR===undefined,historyUndefined:receiver.historyXhr===undefined,id});if(cfg.apiFailAt===id)throw original;return {client:id,base:base==='history'?'history':'derivatives'};}
 const bindings={...types,BaseWebSocket:BaseSocket,BaseExchange,createAPI,rateLimit(client,config){effect('rateLimit',{client,maxRPS:config.maxRPS});return cfg.wrapRate?{wrapped:client}:client;},GRAFANA_ENDPOINTS_EVERY:grafana.GRAFANA_ENDPOINTS_EVERY,GRAFANA_ENDPOINTS_PERIODIC:grafana.GRAFANA_ENDPOINTS_PERIODIC,
 Date:class extends Date{static now(){return cfg.clock??1723638896789;}},ORDER_TYPE:spotTypes.ORDER_TYPE,v:getV,createPositionId,jsonParse:JSON.parse,console:{error(...args){effect('parseError',{prefix:args[0],message:args[1]?.message});}}};
 return {cfg,events,original,bindings,classCache:new Map(),get current(){return current;},effect};
}
function instantiate(env,name){if(!env.classCache.has(name))env.classCache.set(name,declaration(name,env.bindings));const {Class,ast,cls}=env.classCache.get(name);const fields=cls.members.filter(m=>ts.isPropertyDeclaration(m)&&!m.modifiers?.some(x=>x.kind===ts.SyntaxKind.DeclareKeyword)).map(m=>m.name.getText(ast));if(!Object.hasOwn(Class.prototype,'__ClassFields'))Object.defineProperty(Class.prototype,'__ClassFields',{value:fields});const exchange=name.endsWith('Exchange');return {receiver:exchange?new Class(env.cfg.name,env.cfg.options,env.cfg.store):new Class(env.cfg.parent),Class,fields,ast,cls};}
function wrap(env,receiver,name){const original=receiver[name];const bound=(...args)=>{env.effect('bound',{method:name,baseId:receiver.baseId,receiverIdentity:receiver===env.current,argumentIdentity:args[0]===env.cfg.argument});return original(...args);};Object.defineProperty(bound,'name',{value:original.name});receiver[name]=bound;}
async function classes(){const rows={};for(const [name,cases]of Object.entries(configs.classes)){rows[name]=[];for(const raw of cases){const env=environment(raw);let receiver,error,snapshot,pair,invocation;try{const instance=instantiate(env,name);receiver=instance.receiver;snapshot=shape(receiver);
 if(env.cfg.pair){const first=receiver;const second=instantiate(env,name).receiver;const methods=instance.fields.filter(k=>typeof first[k]==='function');const objects=instance.fields.filter(k=>first[k]&&typeof first[k]==='object');pair={functions:methods.map(k=>({key:k,fresh:first[k]!==second[k]})),containers:objects.map(k=>({key:k,shared:first[k]===second[k]})),prototypeOwn:Object.hasOwn(first,'mapTimeframe')};const member=name==='KrakenExchange'?'dispose':name==='KrakenSpotExchange'?'tick':'onMessage';first.isDisposed=true;wrap(env,first,member);const detached=first[member];await Reflect.apply(detached,second,[env.cfg.argument]);}
 if(env.cfg.invoke){if(env.cfg.invoke==='mapTimeframe')wrap(env,receiver,'mapTimeframe');for(const method of ['onMessage','handleFills','handleExecutions','parseOrder','throttledFetchOrders'])if(typeof receiver[method]==='function')wrap(env,receiver,method);receiver.throttledFetchOrders=()=>{env.effect('bound',{method:'throttledFetchOrders',baseId:receiver.baseId,receiverIdentity:receiver===env.current,argumentIdentity:false});env.effect('throttle');};invocation=await receiver[env.cfg.invoke](env.cfg.argument);}
 }catch(e){error={identity:e===env.original,name:e.name,message:e.message};}
 rows[name].push({cfg:capture(raw),snapshot,pair,invocation:capture(invocation),error,events:capture(env.events),sourceConstructed:!!receiver});}}return rows;}
async function polling(){const rows=[];for(const raw of configs.polling){const cfg=structuredClone(raw),events=[],original=new Error('read failure'),active=new Map(),timeouts=new Map(),cache=new Map(),timers=[];let calls=0,count=0,cleanup,release;
 const effect=(kind,details={})=>{events.push({kind,...details});if(cfg.effectFail===kind)throw original;};
 const receiver={ohlcvPollingActive:active,ohlcvPollingTimeouts:timeouts,lastCandleCache:cache,fetchOHLCV(options){const n=calls++;effect('fetch',{options});if(cfg.fetchSyncFail)throw original;return new Promise((r,j)=>{release=()=>cfg.fetchFail?j(original):r(cfg.pages[Math.min(n,cfg.pages.length-1)]);});}};
 const bindings={Date:{now:()=>1723638896789},setTimeout(callback,delay){const timer=++count;const sameCallback=timers.length===0||timers.at(-1).callback===callback;effect('timer',{timer,delay,sameCallback});timers.push({timer,callback,canceled:false});return timer;},clearTimeout(timer){effect('clear',{timer});const t=timers.find(t=>t.timer===timer);if(t)t.canceled=true;},console:{error(prefix,error){effect('console',{prefix,identity:error===original});}}};
 const file=path.join(src,'exchanges/kraken/kraken.exchange.ts');const listen=method(file,'KrakenExchange','listenOHLCV',bindings,receiver);
 const key=cfg.options.symbol+':'+cfg.options.interval;if(cfg.cached)cache.set(key,cfg.cached);
 const callback=candle=>{effect('candle',{candle,identity:cfg.pages.some(p=>p.includes(candle))});if(cfg.mutateCandle)Object.assign(candle,cfg.mutateCandle);if(cfg.cleanupOnCallback)cleanup();if(cfg.callbackFail)throw original;};
 rejectionObserver=error=>effect('unhandled',{identity:error===original});
 let error;try{cleanup=listen(cfg.options,callback);const prefix=events.length;Object.assign(cfg.options,cfg.mutateOptions||{});if(cfg.cleanupBefore)cleanup();if(release){release();release=undefined;}await new Promise(r=>setImmediate(r));
 if(cfg.replace){const old=cleanup;cleanup=listen(cfg.options,callback);if(cfg.oldCleanup)old();if(release){release();release=undefined;}await new Promise(r=>setImmediate(r));}
 for(let i=0;i<(cfg.fire??0);i++){const t=timers.findLast(t=>!t.canceled);assert.ok(t,'finite requested timer exists');t.canceled=true;t.callback();if(release){release();release=undefined;}await new Promise(r=>setImmediate(r));}
 await new Promise(r=>setImmediate(r));
 const beforeCleanup={active:capture(Object.fromEntries(active)),timeouts:capture(Object.fromEntries(timeouts)),cache:capture(Object.fromEntries(cache))};cleanup();rows.push({cfg:capture(raw),prefix,events:capture(events),beforeCleanup,afterCleanup:{active:capture(Object.fromEntries(active)),timeouts:capture(Object.fromEntries(timeouts)),cache:capture(Object.fromEntries(cache))}});
 }catch(e){error={identity:e===original,name:e.name,message:e.message};rows.push({cfg:capture(raw),error,events:capture(events)});}
 }return rows;}
(async()=>{const a=await classes(),c=await polling();process.stdout.write(JSON.stringify({header:{hashes,tz:process.env.TZ},classes:a,polling:c}));})().catch(e=>{console.error(e);process.exitCode=1});
