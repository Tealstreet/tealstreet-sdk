'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),file='packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts',source=fs.readFileSync(path.join(root,file),'utf8'),tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
const cls=tree.statements.find(ts.isClassDeclaration),selected=cls.members.filter(m=>m.name?.getText(tree)==='fetchOrdersOnce');assert.equal(selected.length,1);const body=selected[0].getText(tree);
const bindings=[['uniq','lodash/uniq',true],['ENDPOINTS','./lighter.types',false],['LogSeverity','../../types',false]];
const imports=bindings.map(([name,module,def])=>{const a=tree.statements.filter(ts.isImportDeclaration).flatMap(n=>{const clause=n.importClause;if(!clause)return[];if(def&&clause.name?.text===name)return[{body:n.getText(tree),module:n.moduleSpecifier.text,type:!!clause.isTypeOnly}];return clause.namedBindings&&ts.isNamedImports(clause.namedBindings)?clause.namedBindings.elements.filter(e=>e.name.text===name&&!e.propertyName).map(e=>({body:n.getText(tree),module:n.moduleSpecifier.text,type:!!(clause.isTypeOnly||e.isTypeOnly)})):[]});assert.equal(a.length,1);assert.equal(a[0].module,module);assert.equal(a[0].type,false);return a[0]});
const inputs=['packages/safe-cex/src/exchanges/lighter/lighter.types.ts','packages/logging/src/LogSeverity.ts','packages/safe-cex/src/types.ts'].map(f=>fs.readFileSync(path.join(root,f),'utf8'));
const typeTree=ts.createSourceFile('types.ts',inputs[0],ts.ScriptTarget.Latest,true),endpoint=typeTree.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(d=>d.name.getText(typeTree)==='ENDPOINTS'));assert.equal(endpoint.length,1);assert.ok(inputs[2].includes("export { LogSeverity } from '@tealstreet/logging/src/LogSeverity'"));
function compile(s){const out=ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal(out.diagnostics.length,0);return out.outputText}
const scope=vm.createContext(vm.constants.DONT_CONTEXTIFY);scope.exports={};vm.runInContext(compile('class Subject {'+body+'}exports.Subject=Subject;'),scope);
const aux=vm.createContext({exports:{}});vm.runInContext(compile(endpoint[0].getText(typeTree)+'\n'+inputs[1]),aux);
const uniq=require(require.resolve('lodash/uniq',{paths:[root]})),lodashVersion=require(require.resolve('lodash/package.json',{paths:[root]})).version;
const SourceArray=vm.runInContext('Array',scope),SourcePromise=vm.runInContext('Promise',scope),SourceObject=vm.runInContext('Object',scope),SourceError=vm.runInContext('Error',scope),SourceRegExp=vm.runInContext('RegExp',scope),SourceString=vm.runInContext('String',scope),SourceNumber=vm.runInContext('Number',scope),SourceDate=vm.runInContext('Date',scope),SourceMap=vm.runInContext('Map',scope);
const sourceThen=SourcePromise.prototype.then,sourceCatch=SourcePromise.prototype.catch,sourceResolve=SourcePromise.resolve,sourceAllSettled=SourcePromise.allSettled,arrayIterator=SourceArray.prototype[Symbol.iterator],arrayMethods=Object.fromEntries(['map','filter','some','flatMap','push'].map(n=>[n,SourceArray.prototype[n]])),sourceIsArray=SourceArray.isArray,sourceValues=SourceObject.values,sourceBigIntString=vm.runInContext('BigInt.prototype.toString',scope),sourceFunctionString=vm.runInContext('Function.prototype.toString',scope),sourceHasInstance=vm.runInContext('Function.prototype[Symbol.hasInstance]',scope),sourceRegExpTest=SourceRegExp.prototype.test,stringMethods=Object.fromEntries(['toLowerCase','includes'].map(n=>[n,SourceString.prototype[n]])),sourceParseInt=SourceNumber.parseInt,sourceIsFinite=SourceNumber.isFinite;
const isSourceError=value=>sourceHasInstance.call(SourceError,value);
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));let c;
function tag(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='boolean')return{kind:'boolean',value:v};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')}}if(typeof v==='bigint')return{kind:'bigint',value:String(v)};if(typeof v==='string')return{kind:'string',units:units(v)};if(typeof v==='symbol')return{kind:'symbol'};if(typeof v==='function'){if(!c.ids.has(v))c.ids.set(v,'Callback'+c.callbacks++);return{kind:'function',id:c.ids.get(v)}}if(!c.ids.has(v))c.ids.set(v,'Literal'+c.literal++);if(v instanceof SourcePromise){c.opaque.add(v);if(!c.observed.has(v)){c.observed.add(v);sourceThen.call(v,undefined,()=>{});}}const id=c.ids.get(v),result={kind:'object',id};if(!c.opaque.has(v)){c.suppress++;try{result.array=Array.isArray(v);result.fields=(isSourceError(v)?['name','message']:Object.keys(v)).map(key=>({keyUnits:units(key),value:tag(v[key])}));result.error=isSourceError(v);if(sourceHasInstance.call(SourceRegExp,v))result.regex={pattern:units(v.source),flags:units(v.flags)};result.symbols=Object.getOwnPropertySymbols(v).filter(k=>Object.prototype.propertyIsEnumerable.call(v,k)).map(key=>({key:tag(key),value:tag(v[key])}));}finally{c.suppress--;}}return result;}
function completion(v,ok){if(ok)return{kind:'ok',value:tag(v)};if(v===c.thrown)return{kind:'thrown',value:tag(v)};if(isSourceError(v))return{kind:'generated',name:v.name,message:units(v.message),value:tag(v)};return{kind:'thrown',value:tag(v)}}
function event(e,work){if(c.suppress)return work();e.depth=c.depth;const key=e.op+':'+(e.name||String(e.key));if(e.key!==undefined){if(typeof e.key==='symbol')e.keyValue=tag(e.key);else e.keyUnits=units(e.key);delete e.key}c.events.push(e);c.seen[key]=(c.seen[key]||0)+1;if(c.i.throwAt===key&&c.seen[key]===(c.i.throwNth||1)){c.injected++;e.error=completion(c.thrown,false);throw c.thrown;}try{const v=work();e.result=tag(v);return v}catch(error){e.error=completion(error,false);throw error}}
function callable(name,receiver,work,construct=false){const id='Callable'+c.callables++;const f=function(...args){if(construct)assert.ok(new.target);else assert.equal(this,receiver,name+' receiver');const e={op:construct?'construct':'call',name,callee:{kind:'function',id},receiver:tag(receiver),args:args.map(tag)};try{const r=event(e,()=>{c.depth++;try{return work(...args)}finally{c.depth--;}});c.events.push({op:'end-call',depth:c.depth});return r}catch(error){c.events.push({op:'end-call',depth:c.depth});throw error}};c.ids.set(f,id);return f;}
function global(name,work,construct=false){Object.defineProperty(scope,name,{configurable:true,get(){return event({op:'global',name},()=>callable(name,undefined,work,construct))}})}
function register(id,v){c.ids.set(v,id);c.opaque.add(v);return v}
// Authored provider records have no thenables; implicit Promise engine reads are outside this bounded trace.
function object(id,fields={}){const target=SourceObject.assign(SourceObject.create(SourceObject.prototype),fields);let p;p=new Proxy(target,{
 get(t,key){if(key==='then')return Reflect.get(t,key,p);return event({op:'get',object:{kind:'object',id},key},()=>Reflect.get(t,key,p))},
 has(t,key){return event({op:'has',object:{kind:'object',id},key},()=>Reflect.has(t,key))},
 ownKeys(t){return event({op:'ownKeys',object:{kind:'object',id}},()=>Reflect.ownKeys(t))},
 getOwnPropertyDescriptor(t,key){return event({op:'descriptor',object:{kind:'object',id},key},()=>Reflect.getOwnPropertyDescriptor(t,key))},
 set(t,key,value){return event({op:'set',object:{kind:'object',id},key,value:tag(value)},()=>Reflect.set(t,key,value,t))}
});register(id,p);c.records.push({id,fields:Object.entries(fields).map(([key,value])=>({keyUnits:units(key),value:tag(value)})),symbols:Object.getOwnPropertySymbols(fields).map(key=>({key:tag(key),value:tag(fields[key])}))});return p;}



function provider(name,value,reject){let resolve,rejection;const p=new SourcePromise((r,j)=>{resolve=r;rejection=j});register('P.'+name,p);c.pending.push({name,value,reject,resolve,rejection});return p;}
function callback(cb,args){c.events.push({op:'invoke',depth:c.depth,id:tag(cb).id,args:args.map(tag)});try{const v=cb(...args);c.events.push({op:'invoke-result',depth:c.depth,result:completion(v,true)});if(v instanceof SourcePromise){const id=tag(v).id;c.callbackPromises.push(v);sourceThen.call(v,value=>c.events.push({op:'callback-ready',depth:0,id,result:completion(value,true)}),error=>c.events.push({op:'callback-ready',depth:0,id,result:completion(error,false)}));}return v}catch(error){c.events.push({op:'invoke-result',depth:c.depth,result:completion(error,false)});throw error}}
function iteratorGet(array){return event({op:'member',object:tag(array),key:'@@iterator',name:'Array.iterator'},()=>callable('Array.iterator',array,()=>{const iter=arrayIterator.call(array),id='Iterator'+c.iterators++;const proxy=new Proxy(iter,{get(target,key){if(key==='next')return event({op:'member',object:{kind:'object',id},key:'next',name:'Iterator.next'},()=>callable('Iterator.next',proxy,()=>{const step=target.next(),rid='IteratorResult'+c.iteratorResults++;const result=new Proxy(step,{get(t,k){return event({op:'get',object:{kind:'object',id:rid},key:k},()=>Reflect.get(t,k))}});return register(rid,result)}));if(key==='return'&&c.i.iteratorReturn)return event({op:'member',object:{kind:'object',id},key,name:'Iterator.return'},()=>callable('Iterator.return',proxy,()=>{c.closed++;if(c.i.returnThrows)throw c.returnThrown;return SourceObject.create(null)}));return event({op:'get',object:{kind:'object',id},key},()=>Reflect.get(target,key));}});return register(id,proxy)}));}
function member(target,id,key,work){Object.defineProperty(target,key,{configurable:true,get(){return event({op:'member',object:tag(target),key,name:id+'.'+key},()=>callable(id+'.'+key,target,work))}})}

Object.defineProperty(SourceArray.prototype,Symbol.iterator,{configurable:true,get(){return iteratorGet(this)}});
for(const name of ['map','filter','some','flatMap'])Object.defineProperty(SourceArray.prototype,name,{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:name,name:'Array.'+name},()=>callable('Array.'+name,receiver,cb=>arrayMethods[name].call(receiver,(...args)=>callback(cb,args))))}});
Object.defineProperty(SourceArray.prototype,'push',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'push',name:'Array.push'},()=>callable('Array.push',receiver,(...args)=>arrayMethods.push.apply(receiver,args)))}});
for(const name of ['toLowerCase','includes'])Object.defineProperty(SourceString.prototype,name,{configurable:true,get(){const receiver=SourceString(this);return event({op:'member',object:tag(receiver),key:name,name:'String.'+name},()=>callable('String.'+name,receiver,(...args)=>stringMethods[name].call(receiver,...args)))}});
Object.defineProperty(SourceRegExp.prototype,'test',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'test',name:'RegExp.test'},()=>callable('RegExp.test',receiver,(...args)=>sourceRegExpTest.call(receiver,...args)))}});
Object.defineProperty(vm.runInContext('BigInt.prototype',scope),'toString',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'toString',name:'BigInt.toString'},()=>callable('BigInt.toString',receiver,(...args)=>sourceBigIntString.call(receiver,...args)))}});
function builtin(name,value){Object.defineProperty(scope,name,{configurable:true,get(){return event({op:'global',name},()=>value)}})}
builtin('Array',SourceArray);builtin('Number',SourceNumber);builtin('Object',SourceObject);builtin('Date',SourceDate);
for(const [object,key,fn,name]of [[SourceArray,'isArray',sourceIsArray,'Array.isArray'],[SourceNumber,'parseInt',sourceParseInt,'Number.parseInt'],[SourceNumber,'isFinite',sourceIsFinite,'Number.isFinite'],[SourceObject,'values',sourceValues,'Object.values']])Object.defineProperty(object,key,{configurable:true,get(){return event({op:'global',name,receiver:tag(object)},()=>callable(name,object,(...args)=>fn.apply(object,args)))}});
Object.defineProperty(SourceDate,'now',{configurable:true,get(){return event({op:'global',name:'Date.now',receiver:tag(SourceDate)},()=>callable('Date.now',SourceDate,()=>100000))}});
Object.defineProperty(scope,'Error',{configurable:true,get(){return event({op:'global-value',name:'Error'},()=>SourceError)}});
Object.defineProperty(SourceError,Symbol.hasInstance,{configurable:true,get(){return event({op:'instanceof-lookup',constructor:tag(SourceError)},()=>function(value){return event({op:'instanceof',constructor:tag(SourceError),object:tag(value)},()=>isSourceError(value))})}});
function externalPromise(prefix,p){if(c.externalNames.has(p)||c.ids.has(p))return p;const name=prefix+c.externalCount++;c.externalNames.set(p,name);register('P.'+name,p);sourceThen.call(p,v=>c.events.push({op:'ready',depth:0,name,result:completion(v,true)}),e=>c.events.push({op:'ready',depth:0,name,result:completion(e,false)}));return p}
function traced(cb){if(typeof cb!=='function'||sourceFunctionString.call(cb).includes('[native code]'))return cb;return function(...args){return callback(cb,args)}}
Object.defineProperty(SourcePromise.prototype,'then',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'then',name:'Promise.then'},()=>callable('Promise.then',receiver,(fulfilled,rejected)=>{const p=sourceThen.call(receiver,traced(fulfilled),traced(rejected));return [fulfilled,rejected].some(cb=>typeof cb==='function'&&!sourceFunctionString.call(cb).includes('[native code]'))?externalPromise('chain',p):p}))}});
Object.defineProperty(SourcePromise.prototype,'catch',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'catch',name:'Promise.catch'},()=>callable('Promise.catch',receiver,cb=>externalPromise('catch',sourceCatch.call(receiver,cb))))}});
Object.defineProperty(SourcePromise,'resolve',{configurable:true,get(){return event({op:'global',name:'Promise.resolve',receiver:tag(SourcePromise)},()=>callable('Promise.resolve',SourcePromise,v=>externalPromise('resolve',sourceResolve.call(SourcePromise,v))))}});
Object.defineProperty(scope,'Promise',{configurable:true,get(){return event({op:'global',name:'Promise'},()=>SourcePromise)}});
Object.defineProperty(SourcePromise,'allSettled',{configurable:true,get(){return event({op:'global',name:'Promise.allSettled',receiver:tag(SourcePromise)},()=>callable('Promise.allSettled',SourcePromise,values=>externalPromise('settled',sourceAllSettled.call(SourcePromise,values))))}});
function mapIterator(target){
 const id='MapIterator'+c.mapIterators++;let proxy;
 proxy=new Proxy(target,{get(t,key){
  if(key===Symbol.iterator)return event({op:'member',object:{kind:'object',id},key:'@@iterator',name:'MapIterator.iterator'},()=>callable('MapIterator.iterator',proxy,()=>proxy));
  if(key==='next')return event({op:'member',object:{kind:'object',id},key,name:'MapIterator.next'},()=>callable('MapIterator.next',proxy,()=>{
   const result=t.next(),rid='MapStep'+c.mapResults++;
   const value=new Proxy(result,{get(r,k){return event({op:'get',object:{kind:'object',id:rid},key:k},()=>Reflect.get(r,k))}});
   return register(rid,value);
  }));
  if(key==='return'&&c.i.mapIteratorReturn)return event({op:'member',object:{kind:'object',id},key,name:'MapIterator.return'},()=>callable('MapIterator.return',proxy,()=>{c.mapClosed++;if(c.i.mapReturnThrows)throw c.returnThrown;return SourceObject.create(null)}));
  return event({op:'get',object:{kind:'object',id},key},()=>Reflect.get(t,key));
 }});
 return register(id,proxy);
}
// Observe the real imported array in place; never copy its values or identity.
function observeImportedArray(array){
 for(const name of ['map','filter','some','flatMap'])Object.defineProperty(array,name,{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:name,name:'Array.'+name},()=>callable('Array.'+name,receiver,cb=>Array.prototype[name].call(receiver,(...args)=>callback(cb,args))))}});
 return array;
}
function prepare(input){
 const i={...input};c={i,ids:new Map(),opaque:new Set(),observed:new WeakSet(),depth:0,suppress:0,events:[],seen:{},injected:0,callables:0,callbacks:0,literal:0,records:[],pending:[],settled:new Set(),iterators:0,iteratorResults:0,closed:0,mapClosed:0,mapIterators:0,mapResults:0,externalNames:new WeakMap(),externalCount:0,callbackPromises:[],requests:[],logs:[],mapped:0,callbackReadies:0};
 c.thrown=object('Token',{message:'injected/\ud800'});c.returnThrown=object('ReturnToken',{message:'return/\ud801'});c.symbol=Symbol('fixture');
 for(const[name,value]of Object.entries({Array:SourceArray,Number:SourceNumber,Object:SourceObject,Date:SourceDate,Promise:SourcePromise,Error:SourceError}))register(name,value);
 const failureMessage=i.invalidAuth?"INVALID AUTH: couldn't find account":i.fallback?'invalid param: MARKET':i.messageEmpty?'':'fixture network failure/\ud800';
 let failure=i.errorInstance?new SourceError(failureMessage):i.errorNumber?42:i.indexString?'index/\ud800':object('Failure',{message:failureMessage,...(i.nestedAuth?{response:object('Response',{data:object('ErrorData',{message:i.nestedAuth===1?"invalid auth: couldnt find account":undefined,error:i.nestedAuth===2?object('NestedError',{message:"invalid auth: couldn't find account"}):undefined})})}:{})});
 if(i.errorInstance) c.ids.set(failure,'SourceError');
 const existing=SourceArray.of(object('ExistingOrder',{id:'existing/\ud801',extra:'extra/\ud800'}));
 const live=object('Live',{orderId:'live/\ud800',marketId:1}),expired=object('Expired',{orderId:'expired',marketId:1}),boundary=object('Boundary',{orderId:'boundary',marketId:1});
 const unknown=object('Unknown',{marker:'unrecognized/\ud800'}),nested=object('NestedOrder',{order:object('NestedValue',{OrderIndex:'nested/\ud801'})});
 let orders=SourceArray.of(live,expired,boundary,nested);
 if(i.aliases)orders=SourceArray.from(['orderId','order_id','orderIndex','order_index','OrderId','OrderIndex','Index','index'].map((key,n)=>object('Alias'+n,{[key]:'alias'+n,...(key==='Index'||key==='index'?{market_id:1}:{})})));
 if(i.changingNested){let count=0;const row=object('ChangingRow',{order:undefined});Object.defineProperty(row,'order',{get(){return object('ChangingValue'+count++,{order_id:count===2?'changed/\ud800':undefined,OrderIndex:'last/\ud801'})}});orders=SourceArray.of(row);}
 if(i.junk)orders=SourceArray.of(5,null,unknown,live);
 if(i.noOrderLike)orders=SourceArray.of(unknown,5,null);
 if(i.primitiveRows)orders=SourceArray.of(undefined,null,17);
 const payloadFor=market=>{
  if(market!==255)return object('MarketPayload'+market,{orders:SourceArray.of(object('RawMarket'+market,{orderId:'m'+market+'/\ud800',marketId:market}))});
  if(i.payloadNull)return null;
  if(i.payloadEmpty)return object('Payload',{});
  if(i.shape==='root')return orders;
  if(i.shape==='dataOrders')return object('Payload',{data:object('Data',{orders})});
  if(i.shape==='resultOrders')return object('Payload',{result:object('Result',{orders})});
  if(i.shape==='data')return object('Payload',{data:orders});
  if(i.shape==='result')return object('Payload',{result:orders});
  if(i.shape==='dictionary')return object('Payload',{orders:object('Dictionary',{first:SourceArray.of(live),second:expired,third:5})});
  if(i.shape==='emptyFirst')return object('Payload',{orders:SourceArray.of(),data:orders});
  const payload=object('Payload',{orders,...(i.lateOrdersThrow?{data:object('LateData',{orders:undefined})}:{})});
  return payload;
 };
 const cancelled=register('Cancelled',new SourceMap(i.noCancelled?[]:[['boundary',70000],['expired',69999],['nan',NaN],['future',100001]]));
 member(cancelled,'Cancelled','entries',()=>mapIterator(SourceMap.prototype.entries.call(cancelled)));
 member(cancelled,'Cancelled','delete',key=>SourceMap.prototype.delete.call(cancelled,key));
 member(cancelled,'Cancelled','has',key=>SourceMap.prototype.has.call(cancelled,key));
 const markets=i.noMarkets?SourceArray.of():SourceArray.from(['1','2','2','-1','nan','1suffix'].map((id,n)=>object('Market'+n,{id})));
 const store=object('Store',{orders:existing,markets});
 const client=object('ApiClient');member(client,'ApiClient','get',(...args)=>{
  c.requests.push(args);const market=args[1].params.market_id;
  if(i.apiSyncThrow&&market===255)throw failure;
  const reject=market===255&&(i.primaryReject||i.fallback||i.invalidAuth||i.nestedAuth||i.errorNumber)||market===1&&(i.oneMarketReject||i.allMarketReject)||market===2&&i.allMarketReject;
  return provider('api'+market,payloadFor(market),reject?failure:undefined);
 });
 const owner=new scope.exports.Subject();register('Owner',owner);c.state={hasWarnedMissingOrderAccountIndex:!!i.warnedIndex,hasWarnedMissingOrderAuth:!!i.warnedAuth,hasWarnedInvalidOrderAuthAccount:!!i.warnedInvalid};
 for(const key of Object.keys(c.state))Object.defineProperty(owner,key,{get(){return event({op:'get',object:{kind:'object',id:'Owner'},key},()=>c.state[key])},set(value){return event({op:'set',object:{kind:'object',id:'Owner'},key,value:tag(value)},()=>{c.state[key]=value})}});
 for(const[key,value]of Object.entries({store,lighterApiClient:client,recentlyCancelledOrders:cancelled}))Object.defineProperty(owner,key,{get(){return event({op:'get',object:{kind:'object',id:'Owner'},key},()=>value)}});
 member(owner,'Owner','ensureAccountIndex',()=>{if(i.indexSyncThrow)throw failure;return provider('index',i.indexUndefined?undefined:i.indexNull?null:42n,i.indexReject?failure:undefined)});
 member(owner,'Owner','getPrivateWsAuthToken',()=>provider('auth',i.noAuth?'':i.authZero?0:'public-auth/\ud800',i.authReject?failure:undefined));
 member(owner,'Owner','log',(...args)=>{c.logs.push(args)});
 member(owner,'Owner','mapOrder',order=>{if(i.mapThrow)throw failure;return i.mapEmpty?SourceArray.of():SourceArray.of(object('Mapped'+c.mapped++,{echo:order,symbol:'OUT/\ud801',contracts:17}))});
 for(const name of ['ENDPOINTS','LogSeverity'])scope[name]=new Proxy(aux.exports[name],{get(t,key){return event({op:'constant',name:name+'.'+key},()=>t[key])}});
 global('String',value=>SourceString(value));global('uniq',value=>observeImportedArray(uniq(value)));
 c.owner=owner;c.cancelled=cancelled;c.initialCancelled=Array.from(SourceMap.prototype.entries.call(cancelled),pair=>[tag(pair[0]),tag(pair[1])]);c.initialState=Object.entries(c.state).map(([key,value])=>({key,value:tag(value)}));
}
async function flush(){await new Promise(r=>setImmediate(r))}
const rows=[];
async function run(input){
 prepare(input);let output=null;try{const p=c.owner.fetchOrdersOnce();sourceThen.call(p,v=>output=completion(v,true),e=>output=completion(e,false))}catch(e){output=completion(e,false)}await flush();
 const steps=[{phase:'initial',events:c.events,output,action:null}];c.events=[];
 for(let n=0;n<30;n++){const p=c.pending.find(p=>!c.settled.has(p.name));if(!p)break;c.settled.add(p.name);if(p.reject===undefined)p.resolve(p.value);else p.rejection(p.reject);await flush();steps.push({phase:p.name,events:c.events,output,action:'provider'});c.events=[];if(n===29)throw Error('source provider floor30 stopped29')}
 assert.ok(output,'authored completed order snapshot');if(input.throwAt)assert.equal(c.injected,1,'authored injection reached '+input.throwAt);
 rows.push({steps,records:c.records,initialState:c.initialState,finalState:Object.entries(c.state).map(([key,value])=>({key,value:tag(value)})),initialCancelled:c.initialCancelled,finalCancelled:Array.from(SourceMap.prototype.entries.call(c.cancelled),pair=>[tag(pair[0]),tag(pair[1])]),requests:c.requests.map(args=>args.map(tag)),closed:c.closed,mapClosed:c.mapClosed,callbackPromises:c.callbackPromises.map(tag),providers:c.pending.map(p=>({name:p.name,value:tag(p.value),reject:p.reject===undefined?null:tag(p.reject)}))});
 if(rows.length===1){ const primaryParams=rows[0].requests[0][1].fields.find(f=>String.fromCharCode(...f.keyUnits)==='params').value;
 assert.equal(primaryParams.fields.find(f=>String.fromCharCode(...f.keyUnits)==='market_id').value.bits,'406fe00000000000','authored all-markets sentinel255');
}
}
(async()=>{
 const cases=[
 {}, //0
 {indexReject:true}, {indexUndefined:true}, {indexReject:true,warnedIndex:true},
 {noAuth:true}, {noAuth:true,warnedAuth:true}, {primaryReject:true},
 {invalidAuth:true}, {invalidAuth:true,errorInstance:true},
 {primaryReject:true,nestedAuth:1}, {primaryReject:true,nestedAuth:2}, {invalidAuth:true,warnedInvalid:true},
 {fallback:true}, {fallback:true,oneMarketReject:true}, {fallback:true,allMarketReject:true}, {fallback:true,noMarkets:true},
 {shape:'dataOrders'}, {shape:'resultOrders'}, {shape:'data'}, {shape:'result'}, {shape:'root'}, {shape:'dictionary'},
 {junk:true}, {noOrderLike:true}, {shape:'emptyFirst'}, {throwAt:'get:result'},
 {payloadEmpty:true}, {payloadNull:true}, {warnedInvalid:true}, {noCancelled:true},
 {aliases:true}, {changingNested:true}, {iteratorReturn:true}, {iteratorReturn:true,returnThrows:true},
 {mapIteratorReturn:true,throwAt:'call:Cancelled.delete'}, {mapIteratorReturn:true,mapReturnThrows:true,throwAt:'call:Cancelled.delete'},
 {invalidAuth:true,throwAt:'get:response'}, {indexReject:true,indexString:true},
 {authReject:true}, {warnedInvalid:true,throwAt:'set:hasWarnedInvalidOrderAuthAccount'},
 {mapThrow:true}, {throwAt:'call:Date.now'}, {fallback:true,apiSyncThrow:true}, {indexSyncThrow:true},
 {warnedInvalid:true,shape:'dataOrders'}, {fallback:true,shape:'data'}, {primaryReject:true,messageEmpty:true,nestedAuth:2},
 {errorNumber:true}, {mapEmpty:true}, {primitiveRows:true},
 {fallback:true,oneMarketReject:true,iteratorReturn:true}, {warnedAuth:true,warnedIndex:true},
 {authZero:true}, {indexNull:true}, {noCancelled:true,payloadEmpty:true},
 {iteratorReturn:true,shape:'dictionary'}, {indexReject:true,throwAt:'get:hasWarnedMissingOrderAccountIndex'},
 {throwAt:'member:ApiClient.get'}, {lateOrdersThrow:true,throwAt:'get:orders',throwNth:2},
 {fallback:true,throwAt:'global:uniq'}, //59
 ];
 assert.equal(cases.length,60,'independent authored input floor');
 for(const input of cases)await run(input);
 assert.equal(rows.length,60,'independent authored order snapshot floor');
 assert.equal(rows[32].closed,5,'authored one candidate plus four pair closes');
 for(const index of [12,13,14])assert.equal(rows[index].steps.flatMap(s=>s.events).filter(e=>e.op==='callback-ready').length,2,'authored two observed async market outcomes');
 assert.equal(rows[0].requests.length,1,'authored one primary request');assert.equal(rows[0].steps.at(-1).output.kind,'ok','authored primary success');
 assert.equal(rows[12].requests.length,3,'authored primary plus two unique markets');assert.equal(rows[12].callbackPromises.length,2,'authored two async market callbacks');
 assert.equal(rows[15].requests.length,1,'authored no fallback markets');assert.equal(rows[15].callbackPromises.length,0,'authored empty callback floor');
 for(const index of [39,40,41])assert.equal(rows[index].requests.length,1,'authored processing errors never refetch');
 assert.equal(rows[34].mapClosed,1,'authored abrupt outer cancellation IteratorClose');assert.equal(rows[35].mapClosed,1,'authored throwing outer cancellation IteratorClose');
 assert.equal(rows[35].steps.at(-1).output.value.id,'Token','authored original body throw beats return throw');
 assert.equal(rows[0].finalCancelled.some(pair=>pair[0].units&&String.fromCharCode(...pair[0].units)==='expired'),false,'authored age30001 expires');
 assert.equal(rows[0].finalCancelled.some(pair=>pair[0].units&&String.fromCharCode(...pair[0].units)==='boundary'),true,'authored age30000 stays');
 const contentHash=crypto.createHash('sha256').update(JSON.stringify({body,imports,inputs,lodashVersion})).digest('hex');process.stdout.write(JSON.stringify({contentHash,rows}));
})().catch(e=>{process.stderr.write(String(e.stack||e));process.exitCode=1});
