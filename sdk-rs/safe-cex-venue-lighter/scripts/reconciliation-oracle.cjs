'use strict';
const fs=require('node:fs'),path=require('node:path'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),vm=require('node:vm'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'),file='packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts',source=fs.readFileSync(path.join(root,file),'utf8'),tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
const cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='LighterExchange'),names=['fetchAndSetPrivateSnapshot','fetchAndSetPrivateSnapshotHelper','tick','fetchBalance','fetchAndSetBalance','syncLeverageFromExchange','getPrivateWsAuthToken','dispose'];
const members=names.map(name=>{const matches=cls.members.filter(n=>n.name?.getText(tree)===name);assert.equal(matches.length,1);return{name,body:matches[0].getText(tree)}});
const constants=tree.statements.filter(ts.isVariableStatement).filter(n=>n.declarationList.declarations.some(d=>d.name.getText(tree)==='INVALIDATE_WEBSOCKET_POSITIONS_UPDATE_DELAY')).map(n=>n.getText(tree));assert.equal(constants.length,1);
const imports=[];for(const[local,module]of [['virtualClock','../../utils/virtualClock'],['loop','../../utils/loop'],['mergeUpdatesUsingLastUpdate','../../utils/mergeUpdatesUsingLastUpdate'],['LogSeverity','../../types'],['BaseExchange','../base']]){const matches=tree.statements.filter(ts.isImportDeclaration).flatMap(n=>n.importClause?.namedBindings&&ts.isNamedImports(n.importClause.namedBindings)?n.importClause.namedBindings.elements.filter(e=>e.name.text===local).map(e=>({original:e.propertyName?.text||e.name.text,module:n.moduleSpecifier.text,type:!!(n.importClause.isTypeOnly||e.isTypeOnly),body:n.getText(tree)})):[]);assert.equal(matches.length,1);assert.equal(matches[0].original,local);assert.equal(matches[0].module,module);assert.equal(matches[0].type,false);imports.push(matches[0]);}
const severity=fs.readFileSync(path.join(root,'packages/logging/src/LogSeverity.ts'),'utf8'),types=fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8');assert.ok(types.includes("export { LogSeverity } from '@tealstreet/logging/src/LogSeverity'"));
const scope=vm.createContext(vm.constants.DONT_CONTEXTIFY);scope.exports={};scope.BaseExchange=class{};
const compiled=ts.transpileModule(severity+'\n'+constants.join('\n')+'\nclass LighterExchange extends BaseExchange {'+members.map(m=>m.body).join('\n')+'} exports.Subject=LighterExchange;',{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal(compiled.diagnostics.length,0);vm.runInContext(compiled.outputText,scope);
const SourcePromise=vm.runInContext('Promise',scope),nativeAll=SourcePromise.all;let c;const token={},symbol=Symbol('message/\ud800');const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
function tag(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='boolean')return{kind:'boolean',value:v};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')}}if(typeof v==='string')return{kind:'string',units:units(v)};if(v===symbol)return{kind:'symbol'};if(v===SourcePromise)return{kind:'function',id:'Promise'};if(typeof v==='function'){return{kind:'function',id:c.ids.get(v)||'Callee'}};if(!c.ids.has(v)){const id='Literal'+c.literals.length;c.ids.set(v,id);const entry={id,array:Array.isArray(v),fields:[]};c.literals.push(entry);entry.fields=Object.keys(v).map(key=>({key,value:tag(v[key])}))}return{kind:'object',id:c.ids.get(v)}}
function event(e,work){c.events.push(e);const key=e.op+':'+(e.name||e.key);c.seen[key]=(c.seen[key]||0)+1;if(c.input.changeAt===key&&c.seen[key]===(c.input.changeNth||1)){if(c.input.change==='disposed')c.state.isDisposed=true;if(c.input.change==='last')c.state.lastWebsocketPositionsUpdate=c.input.nextLast;if(c.input.change==='signer')c.state.signerAdapter=c.input.nextSigner?c.signer:undefined;}
 if(c.input.throwAt===key){e.error='token';throw token}try{const result=work();e.result=tag(result);return result}catch(err){e.error=err===token?'token':{name:err.name,message:units(err.message)};throw err}}
function call(name,receiver,work){return function(...args){assert.equal(this,receiver,name+' receiver');if(name==='loop')c.ids.set(args[0],'Callback');return event({op:'call',name,receiver:tag(receiver),args:args.map(tag)},()=>work(...args))}}
function bind(obj,id,key,work){Object.defineProperty(obj,key,{configurable:true,get(){return event({op:'member',name:id+'.'+key,object:tag(obj),key},()=>call(id+'.'+key,obj,work))}})}
function property(obj,key,store){Object.defineProperty(obj,key,{configurable:true,get(){return event({op:'get',object:tag(obj),key},()=>store[key])},set(value){event({op:'set',object:tag(obj),key,value:tag(value)},()=>{store[key]=value})}})}
function record(id,values,{spread=false}={}){const target={};for(const[key,value]of Object.entries(values))Object.defineProperty(target,key,{value,writable:true,configurable:true,enumerable:key!=='hidden'});const proxy=new Proxy(target,{get(t,key){return event({op:'get',object:tag(proxy),key:String(key)},()=>Reflect.get(t,key))},...(spread?{ownKeys(t){return event({op:'own-keys',object:tag(proxy)},()=>Reflect.ownKeys(t))},getOwnPropertyDescriptor(t,key){return event({op:'descriptor',object:tag(proxy),key},()=>{const d=Reflect.getOwnPropertyDescriptor(t,key);return d&&{...d}})}}:{})});c.ids.set(proxy,id);c.objects[id]=proxy;return proxy}
// Instrument the actual result iterator, preserving its two source next calls
// and early IteratorClose return lookup. No source member body is rewritten.
function pairProxy(values){const target=values;const proxy=new Proxy(target,{get(t,key){if(key===Symbol.iterator)return event({op:'get',object:tag(proxy),key:'@@iterator'},()=>{const method=call('Pair.iterator',proxy,()=>{
 const native=t[Symbol.iterator](),iter=new Proxy(native,{get(it,key){if(key==='next')return event({op:'get',object:tag(iter),key:'next'},()=>call('Iterator.next',iter,()=>{const result=native.next();const id=result.done?'Done':c.nextResult++===0?'Result0':'Result1';return record(id,result)}));return event({op:'get',object:tag(iter),key:String(key)},()=>Reflect.get(it,key))}});c.ids.set(iter,'Iterator');return iter});c.ids.set(method,'IteratorFunction');return method});return event({op:'get',object:tag(proxy),key:String(key)},()=>Reflect.get(t,key))}});c.ids.set(proxy,'Pair');return proxy}
Object.defineProperty(SourcePromise,'all',{get(){return event({op:'global',name:'Promise.all'},()=>call('Promise.all',SourcePromise,values=>{const promise=nativeAll.call(SourcePromise,values).then(pair=>pairProxy(pair));c.ids.set(promise,'AllPromise');return promise}))}});
const clock={};scope.virtualClock=clock;Object.defineProperty(clock,'getCurrentTimeMs',{get(){return event({op:'global',name:'virtualClock.getCurrentTimeMs'},()=>call('virtualClock.getCurrentTimeMs',clock,()=>c.input.now))}});
for(const[name,work]of [['String',value=>String(value)],['mergeUpdatesUsingLastUpdate',()=>c.objects.MergedPositions],['loop',(callback,interval)=>{c.callback=callback;c.ids.set(callback,'Callback');if(c.input.loopNow)callback();return undefined}]])Object.defineProperty(scope,name,{configurable:true,get(){return event({op:'global',name},()=>call(name,undefined,work))}});
Object.defineProperty(scope.BaseExchange.prototype,'dispose',{get(){const owner=this;return event({op:'global',name:'super.dispose'},()=>call('super.dispose',owner,()=>{c.state.isDisposed=true}))}});
function completion(value,error){if(error===token)return{kind:'thrown'};if(error)return{kind:'generated',name:error.name,message:units(error.message)};return{kind:'ok',value:tag(value)}}
const rows=[];
async function run(method,extra={}){
 const input={disposed:false,last:undefined,nextLast:undefined,now:10000,opts:'record',ttl:undefined,signer:true,initSigner:true,signerDispose:true,errorMessage:'fault/\ud800',errorKind:'token',reject:null,loopNow:false,callback:true,pending:true,...extra};
 c={input,events:[],ids:new WeakMap(),literals:[],objects:{},seen:{},nextResult:0,state:{isDisposed:input.disposed,lastWebsocketPositionsUpdate:input.last},providers:{},settlers:{}};
 const owner=new scope.exports.Subject();c.ids.set(owner,'Owner');const originals=Object.fromEntries(names.map(name=>[name,owner[name]]));
 c.ids.set(token,'Token');Object.defineProperty(token,'message',{configurable:true,get(){return event({op:'get',object:tag(token),key:'message'},()=>input.errorMessage)}});
 const store={},signer={},publicWs={},privateWs={},map={},rawBalance={},storedBalance={},positions={},orders={},currentPositions={},mergedPositions={},tickReturn={};
 for(const[id,obj]of Object.entries({Store:store,Signer:signer,Public:publicWs,Private:privateWs,Map:map,RawBalance:rawBalance,StoredBalance:storedBalance,Positions:positions,Orders:orders,CurrentPositions:currentPositions,MergedPositions:mergedPositions,TickReturn:tickReturn})){c.ids.set(obj,id);c.objects[id]=obj;}c.ids.set(clock,'virtualClock');c.signer=signer;
 c.rawReads=0;for(const balance of [rawBalance,storedBalance])Object.setPrototypeOf(balance,Object.create(Object.prototype,{then:{get(){
  return event({op:'get',object:tag(balance),key:'then'},()=>{
   const eligible=balance===storedBalance||++c.rawReads>1;
   if(!eligible||!input.returnThen)return undefined;
   if(input.returnThen==='throw')throw token;
   const then=function(resolve,reject){assert.equal(this,balance,'inherited then receiver');event({op:'return-then-call',object:tag(balance)},()=>{input.returnThen==='resolve'?resolve('adopted/\ud800'):reject(token)})};c.ids.set(then,'Adopter');return then;
  });
 }}}));
 const loaded=record('Loaded',{7:'meta/\ud800',markets:false,positions:false,orders:false,balance:false,hidden:'not-enumerable'},{spread:true});
 c.state.store=store;c.state.signerAdapter=input.signer?signer:undefined;c.state.publicWebsocket=publicWs;c.state.privateWebsocket=privateWs;c.state.ohlcvFallbackFetchByKey=map;
 for(const key of Object.keys(c.state))property(owner,key,c.state);for(const[key,value]of Object.entries({positions:currentPositions,balance:storedBalance,loaded}))property(store,key,{[key]:value});
 const err=input.errorKind==='undefined'?undefined:input.errorKind==='string'?'err/\ud800':input.errorKind==='symbol'?symbol:token;
 const provider=(name,value)=>{const promise=new SourcePromise((resolve,reject)=>{c.settlers[name]=()=>{if(name==='ensure'&&input.initSigner)c.state.signerAdapter=signer;input.reject===name?reject(err):resolve(value)}});c.ids.set(promise,name+'Promise');c.providers[name]=promise;return promise};
 const pHelper=provider('helper',undefined),pPositions=provider('positions',positions),pOrders=provider('orders',orders),pRaw=provider('raw',rawBalance),pBalance=provider('balance',rawBalance),pEnsure=provider('ensure',signer),pAuth=provider('auth','auth/\ud800');
 const used=new Set();const use=(name,promise)=>{used.add(name);return promise};
 bind(owner,'Owner','fetchAndSetPrivateSnapshotHelper',()=>use('helper',pHelper));bind(owner,'Owner','fetchPositions',()=>use('positions',pPositions));bind(owner,'Owner','fetchOrders',()=>use('orders',pOrders));bind(owner,'Owner','fetchRawBalance',()=>use('raw',pRaw));bind(owner,'Owner','fetchBalance',()=>use('balance',pBalance));bind(owner,'Owner','ensureSignerAdapter',()=>use('ensure',pEnsure));bind(owner,'Owner','getTickInterval',()=>17.5);bind(owner,'Owner','tick',()=>tickReturn);
 for(const key of ['handlePrivateSnapshotFetchError','emitAndLogError','log','warnSignerUnavailable'])bind(owner,'Owner',key,()=>undefined);
 for(const key of ['addOrUpdatePositionsFull','addOrUpdateOrdersFull','update'])bind(store,'Store',key,()=>undefined);
 bind(signer,'Signer','getAuthToken',()=>use('auth',pAuth));bind(signer,'Signer','dispose',()=>undefined);
 // Optional guard Get differs from the later captured member Get.
 Object.defineProperty(signer,'dispose',{configurable:true,get(){return event({op:'get',object:tag(signer),key:'dispose'},()=>input.signerDispose==='noncallable'?17:input.signerDispose?call('Signer.dispose',signer,()=>undefined):undefined)}});
 bind(publicWs,'Public','dispose',()=>undefined);bind(privateWs,'Private','dispose',()=>undefined);bind(map,'Map','clear',()=>undefined);
 const opts=record('Opts',{allowRecentAccountSnapshot:true,includeAccountLimits:false});let options=input.opts==='record'?opts:input.opts==='undefined'?undefined:input.opts==='null'?null:false;
 const args=['fetchBalance','fetchAndSetBalance'].includes(method)?[options]:method==='getPrivateWsAuthToken'?[input.ttl]:method==='syncLeverageFromExchange'?['BTC/\ud800']:[];
 const taggedArgs=args.map(tag),steps=[];let value,error;try{value=originals[method](...args)}catch(e){error=e}
 if(value&&typeof value.then==='function'){
  steps.push({phase:'initial',events:c.events,output:{kind:'promise'}});c.events=[];
  let done;const settled=value.then(v=>{done=completion(v)},e=>{done=completion(undefined,e)});
  // Each settlement action is independent of native output; source execution
  // determines which API calls exist. Drain source FIFO jobs between stages.
  let actions=[];for(let i=0;i<12&&!done;i++){
   await new Promise(resolve=>setImmediate(resolve));
   steps.push({phase:'idle',events:c.events});c.events=[];
   const next=[...used].find(name=>!actions.includes(name));if(!next)break;
   actions.push(next);c.settlers[next]();
  }
  await settled;
  steps.push({phase:'settled',events:c.events,output:done});c.events=[];
  // Move each fulfilled dependency's events to the next source idle/settled
  // phase. Its action is attached to that next phase, before continuation.
  let pendingActions=0;for(let i=1;i<steps.length;i++){if(i>1&&pendingActions<actions.length){steps[i].resolve=actions[pendingActions++];}}
  assert.equal(pendingActions,actions.length,'authored max12 dependency settlement floor');
 }else{steps.push({phase:'initial',events:c.events,output:completion(value,error)});c.events=[];}
 if(c.callback&&input.callback){let result,error;try{result=c.callback()}catch(e){error=e}steps.push({phase:'callback',events:c.events,output:completion(result,error)});c.events=[];}
 const construction=[];for(const step of steps)for(const e of step.events){
  if(e.op!=='call')continue;
  const add=(v,op)=>{if(v?.kind==='object'&&v.id.startsWith('Literal'))construction.push({op,id:v.id})};
  if(e.name==='Owner.fetchPositions')add(e.args[1],'literal');
  if(e.name==='Owner.fetchRawBalance'||e.name==='Owner.fetchBalance')add(e.args[0],'literal');
  if(e.name==='Promise.all')add(e.args[0],'array');
  if(e.name==='Store.update'){const root=c.literals.find(v=>v.id===e.args[0].id);const loaded=root.fields.find(f=>f.key==='loaded');add(loaded.value,'spread');add(e.args[0],'literal');}
 }
 rows.push({method,input:{reject:input.reject,errorKind:input.errorKind,loopNow:input.loopNow,initSigner:input.initSigner,returnThen:input.returnThen||null},args:taggedArgs,steps,literals:c.literals,construction});
}
(async()=>{
 for(const extra of [{},{reject:'helper'},{throwAt:'member:Owner.fetchAndSetPrivateSnapshotHelper'},{throwAt:'call:Owner.fetchAndSetPrivateSnapshotHelper'}])await run('fetchAndSetPrivateSnapshot',extra);
 for(const last of [undefined,8999,9000,9001,10000,NaN,Infinity,-0])await run('fetchAndSetPrivateSnapshotHelper',{last});
 for(const extra of [{disposed:true},{reject:'positions'},{reject:'orders'},{reject:'balance'},{last:9001,changeAt:'get:lastWebsocketPositionsUpdate',changeNth:2,change:'last',nextLast:undefined},{changeAt:'call:Store.addOrUpdatePositionsFull',change:'disposed'},{changeAt:'call:Owner.fetchBalance',change:'disposed'}])await run('fetchAndSetPrivateSnapshotHelper',extra);
 for(const throwAt of ['global:Promise.all','member:Owner.fetchPositions','call:Owner.fetchPositions','member:Owner.fetchOrders','call:Owner.fetchOrders','get:@@iterator','call:Iterator.next','get:return','get:isDisposed','get:lastWebsocketPositionsUpdate','global:virtualClock.getCurrentTimeMs','call:virtualClock.getCurrentTimeMs','global:mergeUpdatesUsingLastUpdate','call:mergeUpdatesUsingLastUpdate','get:store','member:Store.addOrUpdatePositionsFull','call:Store.addOrUpdatePositionsFull','member:Store.addOrUpdateOrdersFull','call:Store.addOrUpdateOrdersFull','member:Store.update','get:loaded','own-keys:undefined','descriptor:markets','get:markets','call:Store.update','member:Owner.fetchBalance','call:Owner.fetchBalance'])await run('fetchAndSetPrivateSnapshotHelper',{last:9001,throwAt});
 for(const extra of [{},{disposed:true},{reject:'helper'},{reject:'helper',errorMessage:undefined},{changeAt:'call:Owner.fetchAndSetPrivateSnapshotHelper',change:'disposed'},{loopNow:true},{throwAt:'member:Owner.fetchAndSetPrivateSnapshotHelper'},{throwAt:'call:Owner.fetchAndSetPrivateSnapshotHelper'}])await run('tick',extra);
 for(const throwAt of ['get:isDisposed','member:Owner.emitAndLogError','get:message','call:Owner.emitAndLogError','global:loop','member:Owner.getTickInterval','call:Owner.getTickInterval','call:loop','member:Owner.tick','call:Owner.tick'])await run('tick',{throwAt,reject:throwAt.includes('message')||throwAt.includes('emitAndLogError')?'helper':null});
 for(const method of ['fetchBalance','fetchAndSetBalance']){
  for(const opts of ['record','undefined','null','false'])for(const disposed of [false,true])await run(method,{opts,disposed});
  for(const extra of [{reject:'raw'},{changeAt:'call:Owner.fetchRawBalance',change:'disposed'},{throwAt:'member:Owner.fetchRawBalance'},{throwAt:'call:Owner.fetchRawBalance'},{throwAt:'get:isDisposed'}])await run(method,extra);
 }
 for(const throwAt of ['get:store','member:Store.update','get:loaded','descriptor:balance','get:balance','call:Store.update'])await run('fetchAndSetBalance',{throwAt});
 for(const disposed of [false,true])for(const returnThen of ['throw','resolve','reject'])await run('fetchBalance',{disposed,returnThen});
 for(const extra of [{},{disposed:true},{reject:'positions'},{reject:'positions',errorMessage:undefined},{reject:'positions',errorKind:'undefined'},{reject:'positions',errorKind:'string'},{reject:'positions',errorKind:'symbol'},{throwAt:'member:Owner.fetchPositions'},{throwAt:'call:Owner.fetchPositions'},{throwAt:'get:isDisposed'},{throwAt:'call:Store.addOrUpdatePositionsFull'},{reject:'positions',throwAt:'member:Owner.log'},{reject:'positions',throwAt:'call:Owner.log'},{reject:'positions',throwAt:'get:message'},{reject:'positions',errorMessage:undefined,throwAt:'global:String'},{reject:'positions',errorMessage:undefined,throwAt:'call:String'}])await run('syncLeverageFromExchange',extra);
 for(const signer of [false,true])for(const ttl of [undefined,null,-0,0,17,NaN])await run('getPrivateWsAuthToken',{signer,ttl});
 for(const extra of [{signer:false,initSigner:false},{signer:false,reject:'ensure'},{reject:'auth'},{reject:'auth',errorMessage:undefined},{signer:false,reject:'ensure',errorKind:'undefined'},{signer:false,reject:'ensure',errorKind:'string'},{reject:'auth',errorKind:'symbol'},{changeAt:'get:signerAdapter',changeNth:2,change:'signer',nextSigner:false}])await run('getPrivateWsAuthToken',extra);
 for(const throwAt of ['get:signerAdapter','member:Owner.ensureSignerAdapter','call:Owner.ensureSignerAdapter','member:Signer.getAuthToken','call:Signer.getAuthToken','member:Owner.warnSignerUnavailable','member:Owner.log','get:message','call:Owner.log','call:Owner.warnSignerUnavailable'])await run('getPrivateWsAuthToken',{signer:!throwAt.includes('ensure')&&!throwAt.includes('warn'),reject:throwAt.includes('warn')?'ensure':throwAt.includes('log')||throwAt.includes('message')?'auth':null,throwAt});
 await run('getPrivateWsAuthToken',{reject:'auth',errorMessage:symbol});await run('syncLeverageFromExchange',{reject:'positions',errorMessage:symbol});
 for(const extra of [{},{signer:false},{signerDispose:'noncallable'},{signerDispose:false},{disposed:true},{changeAt:'get:dispose',change:'signer',nextSigner:false}])await run('dispose',extra);
 for(const throwAt of ['global:super.dispose','call:super.dispose','get:ohlcvFallbackFetchByKey','member:Map.clear','call:Map.clear','get:publicWebsocket','member:Public.dispose','call:Public.dispose','get:privateWebsocket','member:Private.dispose','call:Private.dispose','get:signerAdapter','get:dispose','call:Signer.dispose'])await run('dispose',{throwAt});
 const counts={fetchAndSetPrivateSnapshot:4,fetchAndSetPrivateSnapshotHelper:42,tick:18,fetchBalance:19,fetchAndSetBalance:19,syncLeverageFromExchange:17,getPrivateWsAuthToken:31,dispose:20};for(const[name,n]of Object.entries(counts))assert.equal(rows.filter(r=>r.method===name).length,n,'independent '+name+' floor');assert.equal(rows.length,170,'independent total floor');
 const payload={file,members,constants,imports,severity,types,rows};const contentHash=require('node:crypto').createHash('sha256').update(JSON.stringify(payload)).digest('hex');process.stdout.write(JSON.stringify({...payload,contentHash}));
})().catch(e=>{process.stderr.write(e.stack+'\n');process.exitCode=1});
