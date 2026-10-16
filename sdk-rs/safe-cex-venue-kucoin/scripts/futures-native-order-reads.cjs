'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),{root,load}=require('./tables.cjs');
const env={...load(path.join(root,'packages/safe-cex/src/types.ts')),...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),...load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.types.ts'))};
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoin.exchange.ts'),source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),cls=source.statements.find(ts.isClassDeclaration);
function install(context,MapType=Map){for(const name of ['fetchNormalOrders','fetchAlgoOrders','fetchOrders','reconcileFetchedOrderRemainders','listenOHLCV','listenOrderBook']){const member=cls.members.find(member=>member.name?.getText(source)===name);assert(member?.initializer,name);const code=ts.transpileModule('module.exports='+member.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,module={exports:{}};new Function('module','Map',...Object.keys(env),code).call(context,module,MapType,...Object.values(env));context[name]=module.exports;}}
function tag(value){if(value===undefined)return{undefined:true};if(typeof value==='number')return{number:String(value),negativeZero:Object.is(value,-0)};if(Array.isArray(value))return Array.prototype.map.call(value,tag);if(value&&typeof value==='object')return Object.fromEntries(Object.keys(value).map(key=>[key,tag(value[key])]));return value;}
async function book(kind){
 const algo=kind.startsWith('algo-'),suffix=kind.split('-').slice(1).join('-'),emitted=[],requests=[],raw={marker:71};let reads=0;
 const a={id:'A',marker:11},b={id:'B',marker:13},items=[a,b],ma={id:'mapped-A',marker:17},mc={id:'mapped-child',marker:19},mb={id:'mapped-B',marker:23};
 let envelope={code:'200000',data:{items}};
 if(suffix==='numeric-code')envelope={code:200000,msg:raw};
 if(suffix==='default')envelope={code:'no',msg:0};
 if(suffix==='nonarray')envelope.data.items={marker:29};
 if(suffix==='missing-data')delete envelope.data;
 if(suffix==='live-data')Object.defineProperty(envelope,'data',{get(){return{items:++reads===1?[]:items};}});
 const xhr={get(...args){assert.equal(this,xhr);requests.push(args);return suffix==='transport'?Promise.reject(Error('transport')):Promise.resolve({data:envelope});}};
 const context={xhr,emitAndLogError(value){assert.equal(this,context);emitted.push(value);}};install(context);
 const mapper=function(...args){assert.equal(this,undefined);assert.equal(args.length,3);assert.equal(args[2],items);assert.equal(args[1],args[0]===a?0:1);assert.equal(args[0],args[1]===0?a:b);return args[0]===a?[ma,mc]:[mb];};context.mapOrder=mapper;
 if(suffix==='mapper-get'){context.mapOrder=()=>{throw Error('stale mapper');};const flatMap=items.flatMap;Object.defineProperty(items,'flatMap',{get(){reads++;context.mapOrder=mapper;return function(...args){assert.equal(this,items);assert.equal(args.length,1);assert.equal(args[0],mapper);return flatMap.call(this,args[0]);};}});}
 const result=await context[algo?'fetchAlgoOrders':'fetchNormalOrders']();
 if(suffix==='numeric-code'){assert.equal(emitted[0],raw);assert.equal(raw.marker,71);raw.marker=101;}
 if(!['numeric-code','default','nonarray','missing-data','transport'].includes(suffix)){assert.equal(result[0],ma);assert.equal(result[1],mc);assert.equal(result[2],mb);}
 return{value:tag(result),emitted:emitted.map(tag),requests:requests.map(args=>args.map(tag)),reads};
}
function row(id,type,marker){return{id,type,marker};}
async function combined(kind){
 const normal=row('fresh-normal','limit',29),stop=row('fresh-stop','stop_market',31),sn=row('stored-normal','limit',11),ss=row('stored-stop','stop_market',17),sp=row('stored-profit','take_profit_market',19),st=row('stored-trailing','trailing_stop_market',23),late=row('late-stop','stop_market',41),first=[sn,ss,sp,st],second=kind==='combined-live-store'?[late]:first;
 const callbacks=[],trace=[],payloads=[],result={marker:37},failure=Error('normal failure');let reads=0,resolveNormal;
 function watch(array){const filter=Array.prototype.filter;Object.defineProperty(array,'filter',{value:function(callback){callbacks.push([callback.name,callback.length]);assert.equal(callback.constructor,Function);return filter.call(this,callback);}});}
 watch(first);if(second!==first)watch(second);
 const context={store:{}};Object.defineProperty(context.store,'orders',{get(){return ++reads===1?first:second;}});install(context);
 const normalFailed=['combined-normal-failed','combined-both-failed','combined-live-store'].includes(kind),stopFailed=['combined-stop-failed','combined-both-failed','combined-live-store'].includes(kind);
 context.fetchNormalOrders=function(...args){assert.equal(this,context);assert.equal(args.length,0);trace.push('normal');return kind==='combined-reject'?Promise.reject(failure):kind==='combined-pending'?new Promise(resolve=>{resolveNormal=resolve;}):Promise.resolve(normalFailed?null:[normal]);};
 context.fetchAlgoOrders=function(...args){assert.equal(this,context);assert.equal(args.length,0);trace.push('algo');return Promise.resolve(stopFailed?null:[stop]);};
 context.reconcileFetchedOrderRemainders=function(...args){assert.equal(this,context);assert.equal(args.length,1);trace.push('reconcile');payloads.push(args[0]);return result;};
 let value;try{const running=context.fetchOrders();if(kind==='combined-pending'){for(let i=0;i<8;i++)await Promise.resolve();assert.deepEqual(trace,['normal']);resolveNormal([normal]);}value=await running;}catch(error){assert.equal(kind,'combined-reject');assert.equal(error,failure);assert.deepEqual(trace,['normal']);assert.equal(payloads.length,0);return{error:error.message,trace,payloads:[],callbacks,reads};}
 assert.equal(value,result);const expected=kind==='combined-live-store'?[sn,late]:[...(normalFailed?[sn]:[normal]),...(stopFailed?[ss,sp,st]:[stop])];assert.equal(payloads[0].length,expected.length);expected.forEach((value,index)=>assert.equal(payloads[0][index],value));
 return{value:tag(value),trace,payloads:payloads.map(tag),callbacks,reads};
}
function reconcile(kind){
 const previous=kind==='reconcile-negative'?-1:kind==='reconcile-infinite'?Infinity:kind==='reconcile-numeric-string'?'3':kind==='reconcile-negative-zero'?-0:kind==='reconcile-equal'?10:3;
 const raw={marker:71},stored={id:kind==='reconcile-distinct-id'?7:'same',extra:{remainingNative:previous}},extra={remainingNative:10,nested:raw},order={id:kind==='reconcile-distinct-id'?'7':'same',symbol:'BTC',remaining:2.5,extra,info:raw},fetched=[order],orders=kind==='reconcile-no-existing'?[]:kind==='reconcile-duplicate'?[{id:'same',extra:{remainingNative:1}},stored]:[stored];let constructs=0,callbacks=0,contractCalls=0;const gets=[];
 function watch(array){const map=Array.prototype.map;Object.defineProperty(array,'map',{value:function(callback){assert.equal(callback.name,'');assert.equal(callback.length,1);assert.equal(callback.constructor,Function);callbacks++;return map.call(this,callback);}});}
 watch(orders);watch(fetched);
 const MapType=class extends Map{constructor(entries){super(entries);assert.equal(new.target,MapType);constructs++;}get(key){gets.push(key);return super.get(key);}};
 const context={store:{orders},contractSizeFor(...args){assert.equal(this,context);assert.equal(args.length,1);assert.equal(args[0],'BTC');contractCalls++;return .25;}};install(context,MapType);
 const value=context.reconcileFetchedOrderRemainders(fetched);assert.notEqual(value,fetched);assert.equal(value.length,1);const changed=['reconcile-clamp','reconcile-duplicate','reconcile-negative-zero'].includes(kind);if(changed){assert.notEqual(value[0],order);assert.notEqual(value[0].extra,extra);}else{assert.equal(value[0],order);assert.equal(value[0].extra,extra);}assert.equal(value[0].info,raw);assert.equal(value[0].extra.nested,raw);assert.equal(order.remaining,2.5);assert.equal(extra.remainingNative,10);raw.marker=101;
 return{value:tag(value),original:tag(order),constructs,callbacks,gets:gets.map(tag),contractCalls};
}
function delegate(kind){
 const argument={marker:11},callback=()=>{throw Error('callback invoked eagerly');},cleanup={marker:17};let calls=0;const socket={},name=kind==='delegate-ohlcv'?'listenOHLCV':'listenOrderBook';socket[name]=function(...args){assert.equal(this,socket);assert.equal(args.length,2);assert.equal(args[0],argument);assert.equal(args[1],callback);calls++;return cleanup;};const context={publicWebsocket:socket};install(context);const value=context[name](argument,callback);assert.equal(value,cleanup);assert.equal(value.marker,17);cleanup.marker=101;return{value:tag(value),calls};
}
(async()=>{const cases=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(cases.length,34);const results=[];for(const kind of cases)results.push(kind.startsWith('combined-')?await combined(kind):kind.startsWith('reconcile-')?reconcile(kind):kind.startsWith('delegate-')?delegate(kind):await book(kind));process.stdout.write(JSON.stringify(results));})().catch(error=>{console.error(error);process.exitCode=1;});
