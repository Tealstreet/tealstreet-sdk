const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..'),sources=new Map();
function evaluate(file,imports,globals={}){
 const source=fs.readFileSync(path.join(root,file),'utf8');sources.set(file,source);
 const result=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
 assert.equal(result.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 const output={};vm.runInNewContext(result.outputText,{exports:output,...globals,require(n){assert.ok(Object.hasOwn(imports,n),n);return imports[n]}});return output;
}
const typeSource=fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8');sources.set('packages/safe-cex/src/types.ts',typeSource);
const tree=ts.createSourceFile('types',typeSource,ts.ScriptTarget.Latest,true),types={};
vm.runInNewContext(ts.transpileModule(tree.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(tree)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:types});
Object.assign(types,evaluate('packages/logging/src/LogSeverity.ts',{}));
const U={kind:'undefined'},N=value=>({kind:'number',value}),B=value=>({kind:'bigint',value});
function dec(v){if(v?.kind==='undefined')return undefined;if(v?.kind==='number')return v.value==='NaN'?NaN:v.value==='Infinity'?Infinity:v.value==='-0'?-0:v.value;if(v?.kind==='bigint')return BigInt(v.value);if(Array.isArray(v))return v.map(dec);if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,dec(v)]));return v}
function enc(v){if(v===undefined)return U;if(v===null)return{kind:'null'};if(typeof v==='boolean')return{kind:'boolean',value:v};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')}}if(typeof v==='bigint')return{kind:'bigint',decimal:String(v)};if(typeof v==='string')return{kind:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};if(Array.isArray(v))return{kind:'array',items:Array.from({length:v.length},(_,i)=>i in v?enc(v[i]):{kind:'hole'})};return{kind:'object',fields:Object.entries(v).filter(([,v])=>typeof v!=='function').map(([k,v])=>[k,enc(v)])}}
const rows=[],add=(id,method,value,extra={})=>rows.push({id,method,value,...extra});
for(const[id,value]of [['primary',{status:' filled ',order_status:'CANCELLED'}],['alias',{order_status:' cancelled '}],['state',{state:' new '}],['empty',{status:'',state:'FILLED'}],['null',{status:null,state:'open'}],['zero',{status:0,state:'open'}],['false',{status:false}],['undefined',{}],['unicode',{status:'\ufeffstraße\u00a0'}],['surrogate',{status:'\ud800abc'}]])add('status-'+id,'normalizeOrderStatus',value);
for(const[id,value]of [['canonical','USER_CANCELLED'],['alternate','USER_CANCELED'],['spaces',' user_cancelled '],['other','liquidated'],['number',0],['null',null],['undefined',U],['unicode','\ufeffuser_canceled\u00a0']])add('reason-'+id,'isUserCancelledReason',value);
for(const[id,value]of [
 ['long',{size:0,marketId:7,side:'BUY',realizedPnl:'23.5tail'}],['short',{size:'-0',market_id:7,positionSide:'sell'}],['both',{qty:'0',symbol:'BTC'}],['negative-sign',{position:'0',marketIndex:7,position_sign:-1}],['positive-sign',{contracts:'0',market_index:7,sign:1}],
 ['null-market',{size:0,market_index:null}],['hex-prefix',{size:'0x10',symbol:'BTC'}],['zero-prefix',{size:'0junk',symbol:'BTC'}],['nonzero',{size:'1',symbol:'BTC'}],['missing',{symbol:'BTC'}],['nan',{size:N('NaN'),symbol:'BTC'}],['infinity',{size:N('Infinity'),symbol:'BTC'}],['empty-symbol',{size:0,symbol:'',marketId:7}],['no-targets',{size:0,symbol:'MISSING'}],['realized-fallback',{size:0,symbol:'BTC',realized_profit:'bad'}],['negative-zero-pnl',{size:0,symbol:'BTC',realizedPnl:'-0'}],
])add('closed-'+id,'resolveExplicitlyClosedPositionUpdates',value);
for(const[id,value,auth]of [['auth',{channel:'account:7',auth:'token'},true],['noauth',{channel:'account:7'},false],['missing',{},false],['zero',{channel:0},true],['big',{channel:'account:7',value:B('17')},true],['keys',{'10':'ten','2':'two',channel:'account',extra:U},false]])add('send-'+id,'sendSubscribePayload',value,{auth});
for(const method of ['throttleFetchOrders','throttleFetchPositions','throttleFetchBalance'])for(const mode of ['default','zero','replace','disposed-before','disposed-after','parent-disposed-after','reject','call-throw'])add(method+'-'+mode,method,{}, {mode});
for(const mode of ['skip-before','skip-after','store-throw','getter-throw','override'])add('throttleFetchOrders-'+mode,'throttleFetchOrders',{}, {mode});
for(const mode of ['store-throw','clock-parent-swap'])add('throttleFetchPositions-'+mode,'throttleFetchPositions',{}, {mode});
for(const mode of ['task-ok','task-throw','task-reject','disposed-before'])add('schedule-'+mode,'scheduleThrottledTask',{}, {mode});
const floors={normalizeOrderStatus:10,isUserCancelledReason:8,resolveExplicitlyClosedPositionUpdates:16,sendSubscribePayload:6,throttleFetchOrders:13,throttleFetchPositions:10,throttleFetchBalance:8,scheduleThrottledTask:4};
for(const[method,count]of Object.entries(floors))assert.equal(rows.filter(r=>r.method===method).length,count);assert.equal(rows.length,75);
async function run(row){
 const events=[],calls=[],updates=[],consoleErrors=[],timers=[],cancelled=[],originals=new Map();let subject,fetchRelease,skipCount=0,taskCount=0;
 function error(stage){const e=new Error('injected:'+stage);originals.set(e,stage);return e}
 function receipt(name,args=[]){events.push(name);calls.push({name,args})}
 const positions=[{id:'long',symbol:'BTC',side:'long',contracts:2,entryPrice:10,notional:20,unrealizedPnl:30,liquidationPrice:40,realizedPnl:9,lastUpdate:1,extra:'keep'},
 {id:'short',symbol:'BTC',side:'short',contracts:-3,entryPrice:11,notional:21,unrealizedPnl:31,liquidationPrice:41,realizedPnl:14,lastUpdate:2,extra:'other'},
 {id:'zero',symbol:'BTC',side:'long',contracts:0},{id:'other',symbol:'ETH',side:'long',contracts:4}];
 const fetched=[{id:'fetched',price:7}],store={positions,marketsById:{'7':{symbol:'BTC'},null:{symbol:'BTC'}}};
 for(const name of ['addOrUpdateOrdersFull','addOrUpdatePositionsFull'])Object.defineProperty(store,name,{value(value){receipt(name,[value]);if(row.mode==='store-throw')throw error('store');updates.push({name,value,identity:value===fetched})}});
 const parent={store,isDisposed:false,lastWebsocketPositionsUpdate:17};const alternateParent={store,isDisposed:false,lastWebsocketPositionsUpdate:29};
 Object.defineProperty(parent,'log',{value(...args){receipt('log',args)}});
 Object.defineProperty(parent,'shouldSkipOrderReconciliationFetch',{value(){receipt('skip');skipCount++;return row.mode==='skip-before'||row.mode==='skip-after'&&skipCount>1}});
 for(const name of ['fetchOrders','fetchPositions','fetchAndSetBalance'])Object.defineProperty(parent,name,{value(...args){receipt(name,args);if(row.mode==='call-throw')throw error('fetch');return new Promise((resolve,reject)=>{fetchRelease=()=>row.mode==='reject'?reject(error('fetch')):resolve(fetched)})}});
 class Base{constructor(p){this.parent=p;this.store=p.store;this.isDisposed=false}sendWithLog(payload){receipt('send',[payload])}}
 const exported=evaluate('packages/safe-cex/src/exchanges/lighter/lighter.ws-private.ts',{
 '@tealstreet/json-parse/src/jsonParse':{jsonParse:JSON.parse},'../../config/config':{},'../../transport/registry':{},'../../types':types,'../../utils/get-key':{},'../../utils/virtualClock':{virtualClock:{getCurrentTimeMs(){receipt('now');if(row.mode==='clock-parent-swap')subject.parent=alternateParent;return 12345.25}}},'../base.ws':{BaseWebSocket:Base},'./lighter.ws-policy':{},
 },{setTimeout(callback,delay){receipt('setTimeout',[delay]);const handle={timer:timers.length+1};timers.push({callback,delay,handle});return handle},clearTimeout(handle){receipt('clearTimeout',[handle]);cancelled.push(handle)},console:{error(prefix,e){receipt('console');consoleErrors.push({prefix,original:originals.get(e)||null,message:e.message})}}});
 subject=new exported.LighterPrivateWebsocket(parent);
 if(row.mode==='getter-throw')Object.defineProperty(subject,'scheduleThrottledTask',{get(){receipt('scheduler-get');throw error('scheduler')}});
 if(row.mode==='override')subject.scheduleThrottledTask=(...args)=>{receipt('scheduler-override',[args[0],args[1],args[3]]);return 99};
 const raw=dec(row.value),opts={skipIfRecent:row.mode==='skip-before'||row.mode==='skip-after'};
 const resultOf=p=>Promise.resolve(p).then(value=>({kind:'value',value:enc(value)}),e=>({kind:'error',name:e.name,message:enc(e.message),original:originals.get(e)||null}));
 let result,callbackResult,beforeFetch,primaryValue;
 try{
  if(row.method==='scheduleThrottledTask'){
   const task=()=>{taskCount++;receipt('task');if(row.mode==='task-throw')throw error('task');return row.mode==='task-reject'?Promise.reject(error('task')):Promise.resolve()};
   result=await resultOf(subject.scheduleThrottledTask('ordersThrottleTimeout',12.5,task,'custom-prefix'));
  }else if(row.method.startsWith('throttle')){
   const delay=row.mode==='zero'?0:undefined;
   result=await resultOf(row.method==='throttleFetchOrders'?subject[row.method](delay,opts):subject[row.method](delay));
   if(row.mode==='replace')result=await resultOf(row.method==='throttleFetchOrders'?subject[row.method](0,opts):subject[row.method](0));
  }else {primaryValue=row.method==='sendSubscribePayload'?subject[row.method](raw,row.auth):subject[row.method](raw);result=await resultOf(primaryValue);}
 }catch(e){result={kind:'error',name:e.name,message:enc(e.message),original:originals.get(e)||null}}
 if(timers.length){
  if(row.mode==='disposed-before')subject.isDisposed=true;
  const timer=timers.at(-1);const callback=timer.callback();
  beforeFetch={state:enc({ordersThrottleTimeout:subject.ordersThrottleTimeout,positionsThrottleTimeout:subject.positionsThrottleTimeout,balanceThrottleTimeout:subject.balanceThrottleTimeout}),updates:enc(updates),fetchPending:!!fetchRelease};
  if(row.mode==='disposed-after')subject.isDisposed=true;if(row.mode==='parent-disposed-after')parent.isDisposed=true;
  if(fetchRelease)fetchRelease();callbackResult=await resultOf(callback);
 }
 const aliases=row.method==='resolveExplicitlyClosedPositionUpdates'&&result.kind==='value'?primaryValue.map(([old,updated])=>({oldIndex:positions.indexOf(old),infoIdentity:updated.info===raw,distinct:updated!==old})):undefined;
 return{...row,value:enc(raw),opts:enc(opts),result,callbackResult,beforeFetch,calls:enc(calls),updates:enc(updates),consoleErrors:enc(consoleErrors),events,
  timers:enc(timers.map(t=>({delay:t.delay,handle:t.handle}))),cancelled:enc(cancelled),taskCount,aliases,
  state:enc({isDisposed:subject.isDisposed,ordersThrottleTimeout:subject.ordersThrottleTimeout,positionsThrottleTimeout:subject.positionsThrottleTimeout,balanceThrottleTimeout:subject.balanceThrottleTimeout,parentDisposed:parent.isDisposed,parentUpdate:parent.lastWebsocketPositionsUpdate,alternateUpdate:alternateParent.lastWebsocketPositionsUpdate}),positions:enc(positions)};
}
(async()=>{const cases=[];for(const row of rows)cases.push(await run(row));process.stdout.write(JSON.stringify({count:75,sources:Object.fromEntries([...sources].map(([p,s])=>[p,crypto.createHash('sha256').update(s).digest('hex')])),cases}))})().catch(e=>{console.error(e);process.exitCode=1});
