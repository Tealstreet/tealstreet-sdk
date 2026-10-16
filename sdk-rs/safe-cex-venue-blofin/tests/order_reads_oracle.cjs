const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,load,ts}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const enums=load(path.join(root,'packages/safe-cex/src/types.ts')),constants=load(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts'));
const {DefaultStore,isTerminalOrder}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const helpers={...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),...load(path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts')),...load(path.join(root,'packages/safe-cex/src/utils/normalizeExecutionPositionSide.ts')),...load(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.order-history.ts'))};
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts'),ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true),klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name?.text==='BlofinExchange');
let active;const context={...enums,...constants,...helpers,isTerminalOrder,BLOFIN_ACTIVE_ORDERS_PAGE_SIZE:100,BLOFIN_MAX_ORDER_PAGES:50,virtualClock:{getCurrentTimeMs:()=>{const now=1000+(++active.clocks);active.trace.push(['clock',now]);return now;}},sleep:async ms=>{active.trace.push(['sleep',ms]);},isKnownNonSwapInstrument:null};
for(const name of ['BLOFIN_ACTIVE_ORDERS_PAGE_SIZE','BLOFIN_MAX_ORDER_PAGES']){const d=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.text===name);assert(d&&ts.isNumericLiteral(d.initializer));assert.equal(Number(d.initializer.text),context[name]);}
const initializer=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.text==='isKnownNonSwapInstrument');
const normalized=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.text==='getNormalizedInstType');
const getNormalizedInstType=new Function(ts.transpileModule('return '+normalized.initializer.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)();
context.isKnownNonSwapInstrument=new Function('getNormalizedInstType','BLOFIN_SUPPORTED_INST_TYPE',ts.transpileModule('return '+initializer.initializer.getText(ast),{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(getNormalizedInstType,ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.text==='BLOFIN_SUPPORTED_INST_TYPE').initializer.text);
const names=['fetchOrders','fetchOrdersWithRetry','fetchPaginatedOrderRows','fetchNormalOrders','fetchAlgoOrders','fetchTriggerOrders','fetchHistoricalOrders','fetchHistoricalFills','mapOrders','addInlineTpSlFallbackOrders','marketIdToSymbol','symbolToMarketId'];const factories={};
for(const name of names){const m=klass.members.find(m=>m.name?.text===name);assert(m&&ts.isPropertyDeclaration(m)&&ts.isArrowFunction(m.initializer),'method_shape:'+name);factories[name]=new Function(...Object.keys(context),ts.transpileModule(`return function(){return ${m.initializer.getText(ast)}}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(...Object.values(context));}
const market={id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:0.01,max:100},leverage:{min:1,max:50}},contractSize:0.01,info:{contractValue:'0.01'}};
const normal=(id='N1',extra={})=>({orderId:id,instId:'BTC-USDT',side:'buy',orderCategory:'normal',orderType:'limit',state:'live',size:'12',filledSize:'2',price:'101.5',...extra});
const algo=(id='A1',extra={})=>({tpslId:id,instId:'BTC-USDT',side:'sell',size:'7',tpTriggerPrice:'149.5',slTriggerPrice:'73.25',...extra});
const trigger=(id='T1')=>({algoId:id,instId:'BTC-USDT',side:'sell',orderType:'trigger',triggerPrice:'105.25',size:'4',reduceOnly:false,state:'live'});
const stored={id:'STORED',symbol:'BTCUSDT',type:'limit',side:'buy',status:'open',price:111,amount:3,filled:1,remaining:2,reduceOnly:false,lastUpdate:999,info:{witness:'retain'}};
const historical=(id='H1',time='1700000001000',extra={})=>({orderId:id,instId:'BTC-USDT',side:'buy',orderType:'limit',state:'filled',price:'101.5',averagePrice:'102.25',size:'12',filledSize:'7',createTime:time,updateTime:'1700000002000',positionSide:'net',...extra});
const fill=(id='F1',extra={})=>({tradeId:id,orderId:'N1',instId:'BTC-USDT',side:'buy',fillPrice:'101.5',fillSize:'7',fillTime:'1700000001000',fee:'-0.35',feeCurrency:'USDT',fillPnl:'2.75',positionSide:'Close Long',...extra});
const url=constants.ENDPOINTS,cases=[];function add(name,args,queues={},extra={}){cases.push({name,args,queues,...extra});}const page=rows=>({code:'0',data:rows});
for(const [name,key,row] of [['fetchNormalOrders','UNFILLED_ORDERS',normal()],['fetchAlgoOrders','UNFILLED_ALGO_ORDERS',algo()],['fetchTriggerOrders','UNFILLED_TRIGGER_ORDERS',trigger()]]){add(name,[],{[url[key]]:[page([row]),page([])]});add(name,[],{[url[key]]:[page([])]});}
add('fetchPaginatedOrderRows',[url.UNFILLED_ORDERS,'orderId','normal',{}],{[url.UNFILLED_ORDERS]:[page([normal('N1')]),page([normal('N2')]),page([])]});
add('fetchPaginatedOrderRows',[url.UNFILLED_ALGO_ORDERS,'tpslId','TPSL',{}],{[url.UNFILLED_ALGO_ORDERS]:[page([algo('A1')]),page([algo('A1')])]});
add('fetchPaginatedOrderRows',[url.UNFILLED_TRIGGER_ORDERS,'algoId','trigger',{orderType:'trigger'}],{[url.UNFILLED_TRIGGER_ORDERS]:[page([{algoId:17}])]});
add('fetchPaginatedOrderRows',[url.UNFILLED_ORDERS,'orderId','normal',{}],{[url.UNFILLED_ORDERS]:[{code:'5',msg:'no rows',data:null}]});
add('fetchPaginatedOrderRows',[url.UNFILLED_ORDERS,'orderId','normal',{}],{[url.UNFILLED_ORDERS]:[page({length:0})]});
add('fetchPaginatedOrderRows',[url.UNFILLED_ORDERS,'orderId','normal',{after:'override',limit:'2'}],{[url.UNFILLED_ORDERS]:[page([])]});
add('fetchPaginatedOrderRows',[url.UNFILLED_ORDERS,'orderId','normal',{}],{[url.UNFILLED_ORDERS]:Array.from({length:50},(_,i)=>page([{orderId:'P'+(i+1)}]))});
add('fetchPaginatedOrderRows',[url.UNFILLED_ORDERS,'orderId','normal',{}],{[url.UNFILLED_ORDERS]:[...Array.from({length:49},(_,i)=>page([{orderId:'P'+(i+1)}])),page([])]});
const emptyQueues=()=>({[url.UNFILLED_ORDERS]:[page([])],[url.UNFILLED_ALGO_ORDERS]:[page([])],[url.UNFILLED_TRIGGER_ORDERS]:[page([])]});
add('fetchOrders',[['IGNORED']],emptyQueues(),{stored:[stored]});
add('fetchOrdersWithRetry',[3],{[url.UNFILLED_ORDERS]:[page([normal('N1',{tpTriggerPrice:'149.5',slTriggerPrice:'73.25'})]),page([])],[url.UNFILLED_ALGO_ORDERS]:[page([algo()]),page([])],[url.UNFILLED_TRIGGER_ORDERS]:[page([trigger()]),page([])]});
add('fetchOrdersWithRetry',[2],Object.fromEntries([url.UNFILLED_ORDERS,url.UNFILLED_ALGO_ORDERS,url.UNFILLED_TRIGGER_ORDERS].map(u=>[u,[{throw:'retry probe'},page([])]])));
add('fetchOrdersWithRetry',[3],Object.fromEntries([url.UNFILLED_ORDERS,url.UNFILLED_ALGO_ORDERS,url.UNFILLED_TRIGGER_ORDERS].map(u=>[u,Array.from({length:3},()=>({throw:'final probe'}))])),{stored:[stored]});
add('fetchOrdersWithRetry',[0],{}, {stored:[stored]});
add('fetchHistoricalOrders',[{symbol:'BTCUSDT',from:1,to:2,limit:200}],{[url.ORDERS_HISTORY]:[page([historical('H1'),historical('H2','1700000003000'),historical('invalid','0')])]});
add('fetchHistoricalOrders',[{}],{[url.ORDERS_HISTORY]:[{code:'5',data:[historical()]}]});
add('fetchHistoricalOrders',[{symbol:'MISSING'}]);add('fetchHistoricalOrders',[{}],{}, {publicOnly:true});
add('fetchHistoricalOrders',[{}],{[url.ORDERS_HISTORY]:[{throw:'history failed',hint:'venue hint'}]});
add('fetchHistoricalOrders',[{limit:0}],{[url.ORDERS_HISTORY]:[page({})]});
add('fetchHistoricalFills',[{symbol:'BTCUSDT',from:1700000000,to:1700000002,limit:0}],{[url.FILLS]:[page([fill()])]});
add('fetchHistoricalFills',[{symbol:'BTCUSDT',orderId:'N1',limit:100}],{[url.FILLS]:[page([fill('F1'),fill('F2'),fill('F1',{fee:'-0.55'}),fill('F3',{orderId:'OTHER'}),fill('F4',{instType:'SPOT'})])]});
add('fetchHistoricalFills',[{symbol:'BTCUSDT',orderId:'N1'}],{[url.FILLS]:[{code:'9',msg:'Operation is not supported'}],[url.ORDERS_HISTORY]:[page([fill('',{tradeId:undefined}),fill('',{tradeId:undefined})])]});
add('fetchHistoricalFills',[{symbol:'BTCUSDT'}],{[url.FILLS]:[page([])],[url.ORDERS_HISTORY]:[{throw:'fallback rejection',messageHint:'OPERATION IS NOT SUPPORTED'}]});
add('fetchHistoricalFills',[{symbol:'BTCUSDT'}],{[url.FILLS]:[{code:0,data:[fill('alias',{fillPrice:'bad',fillPx:'103.25',fillSize:'',filledSize:'-9',fee:'',fillFee:'-0.65',fillTime:'bad',execTime:'1700000004'})]}]});
add('fetchHistoricalFills',[{symbol:'BTCUSDT'}],{[url.FILLS]:[page([fill('bad',{fillPrice:'bad',fillSize:'bad'}),fill('generic',{fillPrice:undefined,fillSize:undefined,price:'1',size:'-1'})])]});
add('fetchHistoricalFills',[{symbol:'BTCUSDT'}],{[url.FILLS]:[{throw:'real rejection',hint:'remote failure'}]});
add('fetchHistoricalFills',[{symbol:'MISSING'}]);add('fetchHistoricalFills',[{}]);add('fetchHistoricalFills',[{symbol:'BTCUSDT'}],{}, {publicOnly:true});
add('fetchHistoricalFills',[{symbol:'BTCUSDT',limit:2.5,from:1700000001000,to:1700000004000}],{[url.FILLS]:[page([fill('F1'),fill('F2',{fillTime:'1700000002000'}),fill('F3',{fillTime:'1700000003000'}),fill('F4',{fillTime:'1700000004000'})])]});
add('fetchHistoricalFills',[{symbol:'17'}],{[url.FILLS]:[page([fill('numeric-unmapped',{instId:17})])]},{market:{...market,id:'OTHER',symbol:'17'}});
add('fetchHistoricalFills',[{symbol:'17'}],{[url.FILLS]:[page([fill('string-unmapped',{instId:'17'})])]},{market:{...market,id:'OTHER',symbol:'17'}});
add('fetchHistoricalFills',[{symbol:'BTCUSDT'}],{[url.FILLS]:[page([fill('numeric-mapped',{instId:17})])]},{market:{...market,id:'17'}});
assert.equal(cases.length,39,'independent_case_floor');
const raw=v=>JSON.stringify(v,(_,v)=>v===undefined?{__undefined:true}:typeof v==='number'&&!Number.isFinite(v)?{__nativeNumber:String(v)}:Object.is(v,-0)?{__nativeNumber:'-0'}:v);
async function run(input){const inputRaw=raw(input),store=new DefaultStore();store.update({markets:[input.market??market],orders:input.stored??[]});const state={store,isPublicDataOnly:input.publicOnly??false,trace:[],clocks:0};active=state;
 for(const [n,f]of Object.entries(factories))state[n]=f.call(state);state.emitAndLogError=v=>{state.trace.push(['emit',v??{__undefined:true}]);};state.log=(text,severity)=>state.trace.push(['log',text,severity]);
 const queues=Object.fromEntries(Object.entries(input.queues??{}).map(([u,v])=>[u,[...v]]));state.xhr={get:async(endpoint,{params})=>{state.trace.push(['get',endpoint,Object.fromEntries(Object.entries(params).filter(([,v])=>v!==undefined))]);const queue=queues[endpoint];assert(queue&&queue.length,'independent_route_queue:'+endpoint);const v=queue.shift();if(v.throw){const err=Error(v.throw);if(v.hint!==undefined)err.response={data:{msg:v.hint}};if(v.messageHint!==undefined)err.response={data:{message:v.messageHint}};throw err;}return {data:v};}};
 let result;try{result={value:await state[input.name](...input.args)}}catch(e){result={error:e.message}}return {inputRaw,name:input.name,result,trace:state.trace,stored:store.orders};}
(async()=>{const outputs=[];for(const input of cases)outputs.push(await run(input));assert.equal(outputs[0].result.value[0].amount,0.12);assert.equal(outputs[0].result.value[0].remaining,0.1);assert.equal(outputs[25].result.value[0].qty,0.07);assert.equal(outputs[25].result.value[0].price,101.5);assert.equal(outputs[25].result.value[0].timestamp,1700000001);assert.equal(outputs[26].result.value.length,2);assert.equal(outputs[26].result.value[0].commission,0.55);assert.equal(outputs[13].result.value.length,49);assert.equal(outputs[12].result.error,'BloFin normal order pagination exceeded 50 pages');
 assert.equal(outputs[36].result.value.length,0,'strict_numeric_fallback');assert.equal(outputs[37].result.value.length,1,'string_fallback');assert.equal(outputs[38].result.value.length,1,'numeric_own_lookup');
 const boundaries=[
 await run({name:'fetchHistoricalFills',args:[{symbol:'BTCUSDT',limit:NaN}],queues:{[url.FILLS]:[page([])],[url.ORDERS_HISTORY]:[page([])]}}),
 await run({name:'fetchHistoricalFills',args:[{symbol:'BTCUSDT'}],queues:{[url.FILLS]:[page([fill('currency',{feeCurrency:17})])]}}),
 await run({name:'fetchHistoricalFills',args:[{symbol:'BTCUSDT',from:-1e308}],queues:{[url.FILLS]:[page([])],[url.ORDERS_HISTORY]:[page([])]}}),
 ];
 assert(Number.isNaN(boundaries[0].trace[0][2].limit),'source_nan_limit_admitted');assert.equal(boundaries[1].result.value[0].commissionAsset,17,'source_nonstr_currency_admitted');assert.equal(boundaries[2].trace[0][2].begin,-Infinity,'source_negative_time_overflow_admitted');

 console.log(raw({header:{architecture:process.arch,nodeVersion:process.version},cases:outputs,boundaries,stoppedAfter:39}));})().catch(e=>{console.error(e);process.exitCode=1});
