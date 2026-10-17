const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..'),cache=new Map();
function compile(text,file){const r=ts.transpileModule(text,{fileName:file,reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});assert(!r.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));return r.outputText;}
function load(file){if(cache.has(file))return cache.get(file);const module={exports:{}};cache.set(file,module.exports);const req=spec=>{
 if(spec.startsWith('@tealstreet/'))return load(path.join(root,'packages',spec.slice('@tealstreet/'.length)+'.ts'));
 if(spec.endsWith('/virtualClock'))return {virtualClock:{getCurrentTimeMs:()=>5678}};
 if(spec.startsWith('.'))return load(path.resolve(path.dirname(file),spec+'.ts'));
 return require(require.resolve(spec,{paths:[path.join(root,'node_modules')]}));
};vm.runInThisContext(`(function(require,module,exports){${compile(fs.readFileSync(file,'utf8'),file)}\n})`,{filename:file})(req,module,module.exports);cache.set(file,module.exports);return module.exports;}
const types=load(path.join(root,'packages/safe-cex/src/types.ts'));
const constants=load(path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.types.ts'));
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const {adjust,subtract}=load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts'));
const {inverseObj}=load(path.join(root,'packages/safe-cex/src/utils/inverseObj.ts'));
const {omitUndefined}=load(path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts'));
const guards=load(path.join(root,'packages/safe-cex/src/exchanges/clientOrderId.ts'));
const deps=Object.fromEntries(['chunk','flatten','omit','times'].map(n=>[n,require(require.resolve('lodash/'+n,{paths:[path.join(root,'node_modules')]}))]));
const names=['createClientOrderId','getOrderPositionSide','formatCreateOrder','formatCreateTrailingStopLossOrder','placeOrder','placeOrders','placeOrderBatch','placeOrderBatchOld'];
function extract(file,className,names){const src=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),cls=src.statements.filter(s=>ts.isClassDeclaration(s)&&s.name?.text===className);assert.equal(cls.length,1);return names.map(name=>{const matches=cls[0].members.filter(m=>m.name?.getText(src)===name);assert.equal(matches.length,1,name+' census');return matches[0].getText(src);}).join('\n');}
const file=path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts');
const base=extract(path.join(root,'packages/safe-cex/src/exchanges/base.ts'),'BaseExchange',['resolveCallerClientOrderId','assertCallerClientOrderIds','assertSingleNativeClientOrderId','wouldSplitOrder','assertCallerClientOrderIdUnsplit','rejectPreflight']);
let uuidValue='',uuidCount=0;
const members=extract(file,'BingxExchange',[...names,'getOrFetchPositionMode','isLikelyTradfiMarket','buildTradfiOneWayModeError','normalizeOrderPlacementError','mapOrder','clientOrderIdRule','_accountPositionMode']);
const Placement=vm.runInNewContext(compile(`class Parent{${base}} class Placement extends Parent{${members}} Placement;`,file),{...types,...constants,...deps,...guards,adjust,subtract,inverseObj,omitUndefined,uuid:()=>uuidCount++===0?uuidValue:uuidValue+'-'+uuidCount,virtualClock:{getCurrentTimeMs:()=>5678}},{filename:file});
const market=(id='BTC-USDT',symbol='BTCUSDT')=>({id,symbol,base:id.split('-')[0],quote:'USDT',active:true,precision:{amount:0.25,price:0.25},limits:{amount:{min:0.1,max:5},leverage:{min:1,max:50}}});
const pos=side=>({id:'BTCUSDT:'+side,symbol:'BTCUSDT',side,entryPrice:100,notional:300,leverage:5,marginType:'cross',positionMode:'hedged',unrealizedPnl:1,contracts:3,liquidationPrice:80});
const tick={id:'BTC-USDT',symbol:'BTCUSDT',bid:99,ask:101,last:100,mark:100,index:100,percentage:1,fundingRate:0,fundingTime:0,volume:9,quoteVolume:900,openInterest:0};
const opts={symbol:'BTCUSDT',type:'limit',side:'buy',amount:2,price:99.88};
const raw=(id,type='LIMIT')=>({orderId:id,symbol:'BTC-USDT',type,side:'BUY',status:'NEW',price:'99.75',origQty:'2',executedQty:'0',timeInForce:'GTC',reduceOnly:false});
const payloads=[{quantity:2,side:'BUY',symbol:'BTC-USDT',type:'LIMIT'},{quantity:1,side:'SELL',symbol:'BTC-USDT',type:'MARKET'}];
function fixture(method,variant){const input={markets:[market()],positions:[pos('long'),pos('short')],tickers:[tick],opts:{...opts},orders:[{...opts},{...opts,side:'sell',type:'market'}],payloads,uuid:'ABCD-0123',mode:'true',override:null,replies:[{code:0,data:{order:raw('accepted-a')}},{code:0,data:{order:raw('accepted-b','MARKET')}}]};
 if(method==='createClientOrderId')input.uuid=['ABCD-0123','F'.repeat(60),'','Aa19Bb'][variant];
 if(method==='getOrderPositionSide'){if(variant===1)input.mode='false';if(variant===2){input.opts.side='sell';input.opts.type='take_profit_market';input.override='hedged';}if(variant===3){input.opts.reduceOnly=true;input.override='hedged';}}
 if(method==='formatCreateOrder'){if(variant===1){input.opts={...opts,type:'market',side:'sell',amount:12.5};input.positions=[];}if(variant===2)input.opts={...opts,type:'stop_market',side:'buy',amount:0,close:true,slTriggerType:'last_price'};if(variant===3)input.opts.clientOrderId='UPPER';}
 if(method==='formatCreateTrailingStopLossOrder'){input.opts={...opts,type:'trailing_stop_market',side:'sell',price:90};if(variant===1)input.opts.price=100;if(variant===2)input.tickers=[];if(variant===3)input.positions=[];}
 if(['placeOrder','placeOrders','placeOrderBatch'].includes(method)){if(variant===1)input.replies=[{code:'0',msg:'strict code'},{code:0,data:{order:raw('accepted-b')}}];if(variant===2)input.replies=[{reject:{message:'transport cause',msg:'wire denied'}},{code:0,data:{order:raw('accepted-b')}}];if(variant===3){if(method==='placeOrders')input.orders=[{...opts,clientOrderId:'same'},{...opts,clientOrderId:'same'}];else if(method==='placeOrder')input.opts.clientOrderId='UPPER';else input.replies=[{code:0,data:{}},{code:0,data:{order:{orderID:0,orderId:17}}}];}}
 if(method==='placeOrderBatchOld'){if(variant===0)input.payloads=[payloads[0]];if(variant===1)input.replies=[{code:0}];if(variant===2){input.payloads=Array.from({length:6},()=>payloads[0]);input.replies=[{code:9,msg:'Msg: lock fail okay'},{code:0,data:{order:raw('legacy-tail')}}];}if(variant===3)input.replies=[[{code:0,data:{order:{id:'legacy'}}}]];}
 return structuredClone(input);}
function observe(value){if(typeof value==='number'){if(Number.isNaN(value))return {number:'NaN'};const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:b.toString('hex')};}if(typeof value==='function')return {callable:String(value)};if(value===undefined)return {undefined:true};if(Array.isArray(value))return value.map(observe);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));return value;}
function setup(input){uuidValue=input.uuid;uuidCount=0;const venue=new Placement(),store=new DefaultStore(),calls=[],reports=[];store.update({markets:input.markets,positions:input.positions,tickers:input.tickers});venue.store=store;venue._exchangeName='bingx';venue.emitAndLogError=(...args)=>reports.push(args);let cursor=0;
 const dispatch=async(method,url,body)=>{calls.push({method,url,...(body===undefined?{}:{body})});if(method==='GET')return {data:{code:0,data:{dualSidePosition:input.mode}}};const response=cursor<input.replies.length?input.replies[cursor++]:{code:0,data:{order:raw('fallback')}};if(response?.reject){const e=new Error(response.reject.message);e.response={data:{msg:response.reject.msg}};throw e;}return {data:response};};
 venue.xhr={get:url=>dispatch('GET',url)};venue.unlimitedXHR={post:(url,body)=>dispatch('POST',url,body)};return {venue,store,calls,reports};}
function invoke(venue,method,input){if(method==='createClientOrderId')return venue.createClientOrderId();if(method==='getOrderPositionSide')return venue.getOrderPositionSide(input.opts,input.override??undefined);if(method==='placeOrders')return venue.placeOrders(input.orders);if(method==='placeOrderBatch'||method==='placeOrderBatchOld')return venue[method](input.payloads);return venue[method](input.opts);}
function state(store){return store.orders.map(o=>Object.fromEntries(['id','symbol','status','type','side','price','amount','filled','remaining','reduceOnly','close','timeInForce','lastUpdate','info'].map(k=>[k,o[k]])));}
async function run(method,input){const f=setup(input);let output;try{output={ok:observe(await invoke(f.venue,method,input))};}catch(e){output={error:{name:e.name,message:e.message}};}return {output,calls:observe(f.calls),reports:observe(f.reports),state:observe(state(f.store))};}
(async()=>{const cases=[];for(const method of names)for(let variant=0;variant<4;variant++){const input=fixture(method,variant);cases.push({method,variant,input,...await run(method,structuredClone(input))});}
 const extras=[];
 for(const [label,method,edit] of [
 ['attached','formatCreateOrder',i=>{i.opts.stopLoss=90;i.opts.takeProfit=110;}],
 ['split-id','formatCreateOrder',i=>{i.opts.amount=12;i.opts.clientOrderId='caller';}],
 ['missing-market','formatCreateOrder',i=>i.opts.symbol='MISSING'],
 ['stop-no-position','formatCreateOrder',i=>{i.positions=[];i.opts={...i.opts,type:'take_profit_market',amount:0};}],
 ['tradfi','formatCreateOrder',i=>{i.markets=[market('XAUUSD-USDT','XAUUSDUSDT')];i.positions=[];i.opts.symbol='XAUUSDUSDT';i.mode='false';}],
 ['uuid-lower','formatCreateOrder',i=>i.opts.clientOrderId='caller'],
 ['trailing-upper-rate','formatCreateTrailingStopLossOrder',i=>{i.opts.type='trailing_stop_market';i.opts.price=250;}],
 ['legacy-reject','placeOrderBatchOld',i=>{i.payloads=[payloads[0]];i.replies=[{reject:{message:'legacy cause',msg:'legacy denied'}}];}],
 ['null-current','placeOrderBatch',i=>i.replies[0]=null],
 ['null-legacy','placeOrderBatchOld',i=>{i.payloads=[payloads[0]];i.replies=[null];}],
 ['legacy-null-msg','placeOrderBatchOld',i=>{i.payloads=payloads;i.replies=[{code:9,msg:null}];}],
 ['legacy-number-msg','placeOrderBatchOld',i=>{i.payloads=payloads;i.replies=[{code:9,msg:42}];}]
 ]){const input=fixture(method,0);edit(input);extras.push({label,method,input,...await run(method,structuredClone(input))});}
 const nan={};for(const method of ['formatCreateOrder','placeOrder','placeOrders']){const input=fixture(method,0);input.opts.amount=NaN;input.orders=[input.opts];nan[method]=await run(method,input);}
 const utf=setup(fixture('createClientOrderId',0));uuidValue='a'.repeat(37)+'😀';const id=utf.venue.createClientOrderId();
 const arbitrary=fixture('getOrderPositionSide',0);arbitrary.opts.reduceOnly='false';const arbitrarySide=await run('getOrderPositionSide',arbitrary);
 const trailing=fixture('formatCreateTrailingStopLossOrder',0);trailing.opts.type='trailing_stop_market';trailing.opts.side='constructor';trailing.opts.price=90;const arbitraryTrailing=await run('formatCreateTrailingStopLossOrder',trailing);
 const callable=fixture('placeOrderBatchOld',1),c=setup(callable);c.venue.unlimitedXHR.post=async(url,body)=>{c.calls.push({method:'POST',url,body});return {data:{code:0,forEach:fn=>fn({code:0,data:{order:{id:'callable-id'}}})}}};const callableIds=await c.venue.placeOrderBatchOld(callable.payloads);
 const inherited=fixture('placeOrderBatch',0);inherited.replies=[{code:0,data:{order:{...raw('inherited-tif'),timeInForce:'constructor'}}}];inherited.payloads=[payloads[0]];const inheritedBatch=await run('placeOrderBatch',inherited);
 const lateInput=fixture('placeOrders',0);lateInput.markets.push(market('XAUUSD-USDT','XAUUSDUSDT'));lateInput.orders=[{...opts,symbol:'MISSING'},{...opts,symbol:'XAUUSDUSDT'}];lateInput.mode='false';const late=setup(lateInput),get=late.venue.xhr.get;let release;
 late.venue.xhr.get=async url=>{if(late.calls.length===1){late.calls.push({method:'GET',url});await new Promise(resolve=>{release=resolve});return {data:{code:0,data:{dualSidePosition:'true'}}};}return get(url);};
 let early;try{await late.venue.placeOrders(lateInput.orders);}catch(e){early=e.message;}const before=uuidCount;assert.equal(typeof release,'function');release();for(let n=0;n<10;n++)await Promise.resolve();const lateSibling={early,before,after:uuidCount,calls:observe(late.calls)};
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},names,cases,extras,boundary:{lateSibling,nan,utf:{units:Array.from({length:id.length},(_,i)=>id.charCodeAt(i))},arbitrarySide,arbitraryTrailing,callableIds,inheritedBatch}}));
})().catch(e=>{console.error(e);process.exitCode=1;});
