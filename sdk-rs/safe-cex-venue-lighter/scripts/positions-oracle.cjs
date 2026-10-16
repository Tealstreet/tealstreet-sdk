'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..'),file='packages/safe-cex/src/exchanges/lighter/lighter.exchange.ts',source=fs.readFileSync(path.join(root,file),'utf8'),tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
const cls=tree.statements.find(ts.isClassDeclaration),selected=cls.members.filter(m=>m.name?.getText(tree)==='fetchPositions');assert.equal(selected.length,1);const body=selected[0].getText(tree);
const bindings=[['uniqBy','lodash/uniqBy',true],['createPositionId','../../utils/createPositionId',false],['PositionSide','../../types',false],['PositionMode','../../types',false]];
const imports=bindings.map(([name,module,def])=>{const a=tree.statements.filter(ts.isImportDeclaration).flatMap(n=>{const clause=n.importClause;if(!clause)return[];if(def&&clause.name?.text===name)return[{body:n.getText(tree),module:n.moduleSpecifier.text,type:!!clause.isTypeOnly}];return clause.namedBindings&&ts.isNamedImports(clause.namedBindings)?clause.namedBindings.elements.filter(e=>e.name.text===name&&!e.propertyName).map(e=>({body:n.getText(tree),module:n.moduleSpecifier.text,type:!!(clause.isTypeOnly||e.isTypeOnly)})):[]});assert.equal(a.length,1);assert.equal(a[0].module,module);assert.equal(a[0].type,false);return a[0]});
const typeFile='packages/safe-cex/src/types.ts',idFile='packages/safe-cex/src/utils/createPositionId.ts',typeSource=fs.readFileSync(path.join(root,typeFile),'utf8'),idSource=fs.readFileSync(path.join(root,idFile),'utf8'),typeTree=ts.createSourceFile(typeFile,typeSource,ts.ScriptTarget.Latest,true),idTree=ts.createSourceFile(idFile,idSource,ts.ScriptTarget.Latest,true);
for(const name of ['PositionSide','PositionMode']){const a=idTree.statements.filter(ts.isImportDeclaration).flatMap(n=>n.importClause?.namedBindings&&ts.isNamedImports(n.importClause.namedBindings)?n.importClause.namedBindings.elements.filter(e=>e.name.text===name&&!e.propertyName).map(e=>({module:n.moduleSpecifier.text,type:!!(n.importClause.isTypeOnly||e.isTypeOnly)})):[]);assert.equal(a.length,1);assert.equal(a[0].module,'../types');assert.equal(a[0].type,false);}
const enumBodies=['PositionSide','PositionMode'].map(name=>{const a=typeTree.statements.filter(n=>ts.isEnumDeclaration(n)&&n.name.text===name);assert.equal(a.length,1);return a[0].getText(typeTree)}),idBodies=idTree.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(idTree)==='createPositionId')).map(n=>n.getText(idTree));assert.equal(idBodies.length,1);
function compile(s){const out=ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal(out.diagnostics.length,0);return out.outputText}
const scope=vm.createContext(vm.constants.DONT_CONTEXTIFY);scope.exports={};vm.runInContext(compile('class Subject {'+body+'}exports.Subject=Subject;'),scope);
const aux=vm.createContext({exports:{}});vm.runInContext(compile(enumBodies.join('\n')+'\n'+idBodies[0]),aux);
const lodashPath=require.resolve('lodash/uniqBy',{paths:[root]}),uniqBy=require(lodashPath),lodashVersion=require(require.resolve('lodash/package.json',{paths:[root]})).version;
const SourceArray=vm.runInContext('Array',scope),SourcePromise=vm.runInContext('Promise',scope),SourceObject=vm.runInContext('Object',scope),SourceError=vm.runInContext('Error',scope),sourceThen=SourcePromise.prototype.then,arrayIterator=SourceArray.prototype[Symbol.iterator],arrayMethods=Object.fromEntries(['map','filter','reduce','push'].map(n=>[n,SourceArray.prototype[n]])),sourceIsArray=SourceArray.isArray;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));let c;
function tag(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='boolean')return{kind:'boolean',value:v};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')}}if(typeof v==='bigint')return{kind:'bigint',value:String(v)};if(typeof v==='string')return{kind:'string',units:units(v)};if(typeof v==='symbol')return{kind:'symbol'};if(typeof v==='function'){if(!c.ids.has(v))c.ids.set(v,'Callback'+c.callbacks++);return{kind:'function',id:c.ids.get(v)}}if(!c.ids.has(v))c.ids.set(v,'Literal'+c.literal++);if(v instanceof SourcePromise){c.opaque.add(v);if(!c.observed.has(v)){c.observed.add(v);sourceThen.call(v,undefined,()=>{});}}const id=c.ids.get(v),result={kind:'object',id};if(!c.opaque.has(v)){c.suppress++;try{result.array=Array.isArray(v);result.fields=(v instanceof SourceError?['name','message']:Object.keys(v)).map(key=>({keyUnits:units(key),value:tag(v[key])}));result.error=v instanceof SourceError;result.symbols=Object.getOwnPropertySymbols(v).filter(k=>Object.prototype.propertyIsEnumerable.call(v,k)).map(key=>({key:tag(key),value:tag(v[key])}));}finally{c.suppress--;}}return result;}
function completion(v,ok){if(ok)return{kind:'ok',value:tag(v)};if(v===c.thrown)return{kind:'thrown',value:tag(v)};if(v instanceof SourceError)return{kind:'generated',name:v.name,message:units(v.message),value:tag(v)};return{kind:'thrown',value:tag(v)}}
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
function callback(cb,args){c.events.push({op:'invoke',depth:c.depth,id:tag(cb).id,args:args.map(tag)});try{const v=cb(...args);c.events.push({op:'invoke-result',depth:c.depth,result:completion(v,true)});return v}catch(error){c.events.push({op:'invoke-result',depth:c.depth,result:completion(error,false)});throw error}}
function iteratorGet(array){return event({op:'member',object:tag(array),key:'@@iterator',name:'Array.iterator'},()=>callable('Array.iterator',array,()=>{const iter=arrayIterator.call(array),id='Iterator'+c.iterators++;const proxy=new Proxy(iter,{get(target,key){if(key==='next')return event({op:'member',object:{kind:'object',id},key:'next',name:'Iterator.next'},()=>callable('Iterator.next',proxy,()=>{const step=target.next(),rid='IteratorResult'+c.iteratorResults++;const result=new Proxy(step,{get(t,k){return event({op:'get',object:{kind:'object',id:rid},key:k},()=>Reflect.get(t,k))}});return register(rid,result)}));if(key==='return'&&c.i.iteratorReturn)return event({op:'member',object:{kind:'object',id},key,name:'Iterator.return'},()=>callable('Iterator.return',proxy,()=>{c.closed++;if(c.i.returnThrows)throw c.returnThrown;return SourceObject.create(null)}));return event({op:'get',object:{kind:'object',id},key},()=>Reflect.get(target,key));}});return register(id,proxy)}));}
function member(target,id,key,work){Object.defineProperty(target,key,{configurable:true,get(){return event({op:'member',object:tag(target),key,name:id+'.'+key},()=>callable(id+'.'+key,target,work))}})}

Object.defineProperty(SourceArray.prototype,Symbol.iterator,{configurable:true,get(){return iteratorGet(this)}});
for(const name of ['map','filter','reduce'])Object.defineProperty(SourceArray.prototype,name,{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:name,name:'Array.'+name},()=>callable('Array.'+name,receiver,(cb,...rest)=>{
 if(name==='reduce'&&++c.reductions===1&&c.i.customAccumulator)return callback(cb,[c.i.customAccumulator==='null'?null:undefined,receiver[0],0,receiver]);
 return arrayMethods[name].call(receiver,(...args)=>callback(cb,args),...rest);
}))}});
Object.defineProperty(SourceArray.prototype,'push',{configurable:true,get(){const receiver=this;return event({op:'member',object:tag(receiver),key:'push',name:'Array.push'},()=>callable('Array.push',receiver,(...args)=>arrayMethods.push.apply(receiver,args)))}});
Object.defineProperty(scope,'Array',{configurable:true,get(){return event({op:'global',name:'Array'},()=>SourceArray)}});
Object.defineProperty(SourceArray,'isArray',{configurable:true,get(){return event({op:'global',name:'Array.isArray',receiver:tag(SourceArray)},()=>callable('Array.isArray',SourceArray,value=>sourceIsArray(value)))}});

function prepare(input){
 const i={...input};c={i,ids:new Map(),opaque:new Set(),observed:new WeakSet(),depth:0,suppress:0,events:[],seen:{},injected:0,callables:0,callbacks:0,literal:0,records:[],pending:[],settled:new Set(),iterators:0,iteratorResults:0,closed:0,reductions:0,modeSets:[]};
 c.thrown=register('Token',{});c.returnThrown=register('ReturnToken',{});c.symbol=Symbol('key');register('Array',SourceArray);
 const symbolA='A/\ud800',symbolB='B/\ud801';
 const makeMarket=(id,symbol,max,mode)=>object('Market'+id,{id,symbol,limits:object('Limits'+id,{leverage:object('Leverage'+id,{max})}),configuredMargin:mode});
 const a=makeMarket('1',symbolA,i.maxZero?0:20,0),b=makeMarket('2',symbolB,9,1);
 const markets=i.noMarkets?SourceArray.of():SourceArray.of(a,b);
 const rawA=object('RawA',{marketId:i.noMatchingMarket?'missing':i.legacy?undefined:'1',market_id:i.legacy?'1':undefined,symbol:symbolA,marginMode:i.marginMissing?undefined:i.legacy?undefined:i.marginOne?1:0,margin_mode:i.legacy?1:undefined,leverage:i.leverageZero?0:i.leverageNegative?-2:3});
 const rawB=object('RawB',{marketId:'2',symbol:symbolB,marginMode:undefined,leverage:7});
 const mappedA=object('MappedA',{id:symbolA,symbol:symbolA,contracts:17,leverage:3});
 const mappedB=object('MappedB',{id:symbolB,symbol:symbolB,contracts:19,leverage:7});
 const positions=i.empty?SourceArray.of():i.twoPositions?SourceArray.of(rawA,rawB):SourceArray.of(i.nullRaw?null:rawA);
 const account=i.accountNull?null:object('Account',{positions:i.positionsUndefined?undefined:i.positionsObject?object('NotArray'):positions});
 const store=object('Store',{markets,marketsById:object('ById',i.lookupBySymbol?{}:{'1':a,'2':b}),marketsBySymbol:object('BySymbol',i.noMatchingMarket?{}:{[symbolA]:a,[symbolB]:b})});
 const modes=register('ModeMap',new Map(i.noCachedMode?[]:[[symbolB,0]]));member(modes,'ModeMap','get',key=>Map.prototype.get.call(modes,key));member(modes,'ModeMap','set',(key,value)=>{c.modeSets.push([key,value]);return Map.prototype.set.call(modes,key,value)});
 const owner=new scope.exports.Subject();register('Owner',owner);
 for(const[key,value]of Object.entries({store,marginModeBySymbol:modes}))Object.defineProperty(owner,key,{get(){return event({op:'get',object:{kind:'object',id:'Owner'},key},()=>value)}});
 member(owner,'Owner','fetchAccountSnapshot',()=>provider('account',account,i.rejectAccount?c.thrown:undefined));
 // Authored dependency binding. Its full implementation is mapped by another symbol.
 Object.defineProperty(owner,'mapPosition',{get(){return event({op:'get',object:{kind:'object',id:'Owner'},key:'mapPosition'},()=>{
   const f=callable('Owner.mapPosition',owner,position=>position===null||i.mapNull?null:i.mapUndefined?undefined:position===rawB?mappedB:mappedA);
   Object.defineProperty(f,'bind',{get(){return event({op:'member',object:tag(f),key:'bind',name:'Function.bind'},()=>callable('Function.bind',f,(...args)=>Function.prototype.bind.apply(f,args)))}});return f;
 })}});
 member(owner,'Owner','resolvePositionLeverage',p=>p.leverage);
 member(owner,'Owner','parseMarginMode',v=>v===0||v===1?v:undefined);
 member(owner,'Owner','marginModeToMarginType',v=>v===1?'isolated':'cross');
 member(owner,'Owner','getMarketConfiguredMarginMode',m=>m.configuredMargin);
 for(const name of ['PositionSide','PositionMode'])scope[name]=new Proxy(aux.exports[name],{get(t,key){return event({op:'constant',name:name+'.'+key},()=>t[key])}});
 global('createPositionId',(...args)=>aux.exports.createPositionId(...args));global('uniqBy',(...args)=>uniqBy(...args));
 c.owner=owner;c.modes=modes;c.initialModes=Array.from(modes.entries()).map(pair=>pair.map(tag));c.args=[i.symbolsUndefined?undefined:SourceArray.of('IGNORED'),i.nullOptions?null:i.defaultOptions?undefined:object('Options',{allowRecentAccountSnapshot:i.allowRecent})];
}
async function flush(){await new Promise(r=>setImmediate(r))}
const rows=[];
async function run(input={}){
 prepare(input);let output=null;try{const p=c.owner.fetchPositions(...c.args);sourceThen.call(p,v=>output=completion(v,true),e=>output=completion(e,false))}catch(e){output=completion(e,false)}await flush();
 const steps=[{phase:'initial',events:c.events,output,action:null}];c.events=[];
 for(let n=0;n<4;n++){const p=c.pending.find(p=>!c.settled.has(p.name));if(!p)break;c.settled.add(p.name);if(p.reject===undefined)p.resolve(p.value);else p.rejection(p.reject);await flush();steps.push({phase:p.name,events:c.events,output,action:'provider'});c.events=[];if(n===3)throw Error('authored provider floor4 stopped3')}
 assert.ok(output,'authored completed position read');if(input.throwAt)assert.equal(c.injected,1,'authored injection reached '+input.throwAt);
 rows.push({initialModes:c.initialModes,args:c.args.map(tag),records:c.records,steps,providers:c.pending.map(p=>({name:p.name,value:tag(p.value),reject:p.reject===undefined?null:tag(p.reject)})),modeSets:c.modeSets.map(pair=>pair.map(tag)),finalModes:Array.from(c.modes.entries()).map(pair=>pair.map(tag))});
}
(async()=>{
 for(const input of [{},{empty:true},{twoPositions:true},{mapNull:true},{mapUndefined:true},{accountNull:true},{positionsUndefined:true},{positionsObject:true},{noMarkets:true},{noMatchingMarket:true},{lookupBySymbol:true},{legacy:true},{marginMissing:true},{marginOne:true},{noCachedMode:true},{leverageZero:true},{leverageNegative:true},{leverageZero:true,maxZero:true,mapNull:true},{defaultOptions:true},{nullOptions:true},{symbolsUndefined:true},{allowRecent:true},{rejectAccount:true},{nullRaw:true},{customAccumulator:'null'},{customAccumulator:'undefined'}])await run(input);
 for(const throwAt of ['get:allowRecentAccountSnapshot','member:Owner.fetchAccountSnapshot','call:Owner.fetchAccountSnapshot','global:Array','global:Array.isArray','member:Array.map','get:mapPosition','member:Function.bind','call:Owner.mapPosition','member:Array.filter','member:Array.reduce','get:marketId','get:symbol','call:Owner.resolvePositionLeverage','call:Owner.parseMarginMode','call:ModeMap.set','call:Owner.marginModeToMarginType','call:ModeMap.get','call:Owner.getMarketConfiguredMarginMode','global:createPositionId','call:createPositionId','member:Array.push','global:uniqBy','call:uniqBy'])await run({throwAt,noCachedMode:throwAt==='call:Owner.getMarketConfiguredMarginMode'});
 assert.equal(rows.length,50,'independent authored positions floor');
 const value=index=>rows[index].steps.at(-1).output.value;
 assert.equal(value(0).fields.length,2,'authored actual plus B placeholder');assert.equal(value(2).fields.length,2,'authored actual positions replace placeholders');
 assert.equal(value(1).fields.length,2,'authored empty positions still create placeholders');
 assert.equal(value(8).fields.length,1,'authored missing markets retains actual mapped row');
 assert.equal(value(17).fields[0].value.fields.find(f=>String.fromCharCode(...f.keyUnits)==='leverage').value.bits,'4024000000000000','authored placeholder fallback leverage10');
 for(const index of [24,25]){const events=rows[index].steps.flatMap(s=>s.events);assert.equal(events.filter(e=>e.op==='call'&&e.name==='Owner.resolvePositionLeverage').length,1,'authored nullish assignment PutValue after leverage RHS');assert.equal(rows[index].steps.at(-1).output.kind,'generated','authored nullish accumulator throws')}
 const contentHash=crypto.createHash('sha256').update(JSON.stringify({body,imports,enumBodies,idSource,lodashVersion})).digest('hex');process.stdout.write(JSON.stringify({contentHash,rows}));
})().catch(e=>{process.stderr.write(String(e.stack||e));process.exitCode=1});
