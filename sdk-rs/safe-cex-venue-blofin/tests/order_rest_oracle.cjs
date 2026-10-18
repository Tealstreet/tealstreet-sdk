const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,load,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const constants=load(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts'));
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const helpers=load(path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name?.text==='BlofinExchange');
const names=['updateOrder','cancelOrders','setLeverage','placeOrder','placeOrders','changeMarginType','placeOrderBatch','placeAlgoOrderBatch','cancelNormalOrders','cancelAlgoOrders','cancelTriggerOrders','isAlgoOrder','isAlreadyGoneCancelRow','parsePositivePrice','mapBlofinTriggerType','placeTriggerOrder','applyConfirmedMarginType'];
let active;
const scope={...enums,...constants,...helpers,chunk:dependencies('lodash/chunk'),partition:dependencies('lodash/partition'),uniqBy:dependencies('lodash/uniqBy'),mapSeries:dependencies('p-iteration').mapSeries,forEachSeries:dependencies('p-iteration').forEachSeries,sleep:async ms=>{active.trace.push(['sleep',ms]);},BLOFIN_MULTI_TRADE_POSITION_SELECTION_CODE:'102143',BLOFIN_MULTI_TRADE_UNSUPPORTED_MESSAGE:'BloFin Multi-Position mode is not supported in Tealstreet yet. Turn off Multi-Position mode on BloFin, then retry from Tealstreet.'};
const factories={};
for(const name of names){const m=klass.members.find(m=>m.name?.text===name);assert(m&&ts.isPropertyDeclaration(m)&&ts.isArrowFunction(m.initializer),'method_shape:'+name);const code=ts.transpileModule(`return function(){return ${m.initializer.getText(ast)}};`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;factories[name]=new Function(...Object.keys(scope),code)(...Object.values(scope));}
// Pin lexical constants against the maintained source, rather than silently inventing their values.
for(const name of ['BLOFIN_MULTI_TRADE_POSITION_SELECTION_CODE','BLOFIN_MULTI_TRADE_UNSUPPORTED_MESSAGE']){
 const d=ast.statements.filter(ts.isVariableStatement).flatMap(s=>s.declarationList.declarations).find(d=>d.name.text===name);assert(d&&ts.isStringLiteral(d.initializer));assert.equal(scope[name],d.initializer.text);
}
const market={id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:0.01,max:100},leverage:{min:2,max:50}},contractSize:0.001,info:{lotSize:'1'}};
const order=(id,type='limit',extra={})=>({id,status:'open',symbol:'BTCUSDT',type,side:'buy',price:101.5,amount:12,filled:0,remaining:12,reduceOnly:false,lastUpdate:123,extra:{remainingNative:7.5},...extra});
const trigger=(id='T1',extra={})=>order(id,'conditional',{info:{orderType:'trigger',algoId:id,instId:'BTC-USDT',orderPrice:'104.75',attachAlgoOrders:[{slTriggerPrice:'73.25',tpTriggerPrice:'149.5',slTriggerPriceType:'mark',tpTriggerPriceType:'index'}]},...extra});
const pos=(id,lev=3,symbol='BTCUSDT')=>({id,symbol,side:'long',contracts:1,entryPrice:101,notional:102,leverage:lev,unrealizedPnl:4,liquidationPrice:5,positionMode:'hedged',marginType:'isolated'});
const opts=(type='limit',extra={})=>({symbol:'BTCUSDT',type,side:'buy',amount:12,price:101.5,...extra});
const payload=(size='12',extra={})=>({instId:'BTC-USDT',size,price:'101.5',...extra});
const ok={code:'0',data:[]},cases=[];
function add(name,args,extra={}){cases.push({name,args,...extra});}
add('placeOrderBatch',[[payload(),payload('3',{reduceOnly:true})]],{responses:[{code:'0',data:[{orderId:'N1',msg:' \ufeffSUCCESS '},{orderId:'N2',msg:null},{orderId:17},{msg:'order placed'}]}]});
add('placeOrderBatch',[[...Array.from({length:21},(_,i)=>payload(String(i+1)))]],{responses:[{code:'0',data:[{orderId:'B20'}]},{code:'0',data:[{orderId:'B21'}]}]});
add('placeOrderBatch',[[]]);
add('placeOrderBatch',[[payload()]],{responses:[{code:0,msg:'outer',data:[{msg:'inner'}]}]});
add('placeOrderBatch',[[payload()]],{responses:[{code:'0',data:[{orderId:'bad',msg:'failure'}]}]});
add('placeOrderBatch',[[payload()]],{responses:[{code:'0',data:[{orderId:{msg:'object ID refusal'},msg:'success'}]}]});
add('placeAlgoOrderBatch',[[payload(),payload('4')]],{responses:[{code:'0',data:{tpslId:'A1'}},{code:'0',data:{}}]});
add('placeAlgoOrderBatch',[[payload()]],{responses:[{code:'102143',msg:'raw multi-position',data:[{msg:'ignored'}]}]});
add('placeAlgoOrderBatch',[[payload()]],{responses:[{code:'5',msg:'outer',data:[{msg:'inner'}]}]});
add('placeAlgoOrderBatch',[[payload()]],{responses:[{code:'0',data:{tpslId:17}}]});
add('placeOrder',[opts()],{responses:[{code:'0',data:[{orderId:'normal'}]}]});
add('placeOrder',[opts('conditional')],{responses:[{code:'0',data:{algoId:'trigger'}}]});
add('placeOrder',[opts('stop_market')],{responses:[{code:'0',data:{tpslId:'algo'}}]});
add('placeOrder',[opts()],{normalSize:'0'});
add('placeOrder',[opts()],{guardThrow:true});
add('placeOrders',[[opts(),opts('stop_market'),opts('conditional')]],{responses:[{code:'0',data:{algoId:'trigger-first'}},{code:'0',data:[{orderId:'normal-second'}]},{code:'0',data:{tpslId:'algo-third'}}]});
add('placeOrders',[[]]);
add('placeOrders',[[opts(),opts('conditional')]],{normalSize:'0'});
add('placeOrders',[[opts(),opts('conditional')]],{guardThrow:true});
add('cancelNormalOrders',[[order('N1'),order('A1','stop_market'),order('x','limit',{symbol:'MISSING'})]],{orders:[order('N1')],responses:[ok]});
add('cancelNormalOrders',[[...Array.from({length:21},(_,i)=>order('N'+i))]],{responses:[ok,ok]});
add('cancelNormalOrders',[[order('N1'),order('N2')]],{orders:[order('N1'),order('N2')],responses:[{code:'5',data:[{orderId:'N1',code:'1000'},{orderId:'N2',code:'8',msg:'real failure'}]}]});
add('cancelNormalOrders',[[order('N1')]],{orders:[order('N1')],responses:[{code:'5',data:[{orderId:'N1',msg:'FILLED, TRIGGERED, CANCELED OR DOES NOT EXIST'}]}]});
add('cancelNormalOrders',[[order('N1')]],{responses:[{code:'5',msg:'empty failure',data:[]}]});
add('cancelNormalOrders',[[order('N1')]],{responses:[{throw:'network error',hint:{venue:'opaque hint'}}]});
add('cancelAlgoOrders',[[order('12__stop_loss','stop_market'),order('12__take_profit','take_profit_market')]],{orders:[order('12__stop_loss','stop_market'),order('12__take_profit','take_profit_market'),order('123__stop_loss','stop_market')],responses:[{code:'5',data:[{tpslId:'12',code:'1000'}]}]});
add('cancelAlgoOrders',[[...Array.from({length:11},(_,i)=>order('A'+i+'__stop_loss','stop_market'))]],{responses:[ok,ok]});
add('cancelAlgoOrders',[[order('A','stop_market')]],{responses:[{code:'5',data:[{code:'1000',msg:'no reconciliation ID'}]}]});
add('cancelTriggerOrders',[[trigger()]],{responses:[{code:'0',data:{algoId:'T1',code:'0'}}]});
add('cancelTriggerOrders',[[trigger()]],{orders:[trigger()],responses:[{code:'5',data:{algoId:'T1',code:'1000'}}]});
add('cancelTriggerOrders',[[trigger(),trigger('T2',{info:{orderType:'trigger',algoId:'T2',instId:'WRONG'}})]]);
add('cancelTriggerOrders',[[trigger()]],{responses:[{code:'0',data:{algoId:'WRONG',msg:'identity mismatch'}}]});
add('cancelOrders',[[order('N__take_profit','take_profit_market'),order('N__stop_loss','stop_market')]],{orders:[order('N'),order('N__take_profit','take_profit_market'),order('N__stop_loss','stop_market')],responses:[ok]});
add('cancelOrders',[[order('A__take_profit','take_profit_market',{amount:0})]],{orders:[order('A__take_profit','take_profit_market',{amount:0,tpTriggerType:'index_price'}),order('A__stop_loss','stop_market',{amount:0,price:73.25,slTriggerType:'mark_price'})],responses:[ok]});
add('cancelOrders',[[trigger(),order('A__stop_loss','stop_market'),order('N')]],{responses:[ok,ok,{code:'0',data:{algoId:'T1'}}]});
add('cancelOrders',[[]]);
add('changeMarginType',[true,'BTCUSDT'],{positions:[pos('P1')],orders:[order('N1')],fetchedPositions:[pos('P1',11)],responses:[{code:'0'}]});
add('changeMarginType',[false,'BTCUSDT'],{responses:[{code:'5',msg:'margin denied'}]});
add('changeMarginType',[false],{responses:[{code:'0'}]});
add('setLeverage',['BTCUSDT',70],{positions:[pos('P1')],margin:'isolated',mode:'hedged',responses:[{code:'0'},{code:'0'}]});
add('setLeverage',['BTCUSDT',1],{responses:[{code:'0'}]});
add('setLeverage',['BTCUSDT',9],{positions:[pos('P1')],margin:'isolated',mode:'hedged',responses:[{code:'0'},{code:'5',msg:'short denied'}]});
add('setLeverage',['BTCUSDT',3],{positions:[pos('P1')]});
add('setLeverage',['BTCUSDT',7],{leverage:{buyLeverage:7,sellLeverage:7}});
add('setLeverage',['BTCUSDT',7],{leverage:{buyLeverage:7,sellLeverage:7,isFetching:true},responses:[{code:'0'}]});
add('setLeverage',['MISSING',10]);
add('updateOrder',[{order:trigger(),update:{price:105.25,amount:9}}],{responses:[{code:'0',data:{algoId:'T1'}},{code:'0',data:{algoId:'T2'}}]});
add('updateOrder',[{order:trigger('T1',{info:{orderType:'trigger',algoId:'WRONG'}}),update:{price:105}}]);
add('updateOrder',[{order:order('N'),update:{price:104.5,amount:8.25}}],{orders:[order('N')],responses:[ok]});
add('updateOrder',[{order:order('N__take_profit','take_profit_market'),update:{price:151.5}}],{orders:[order('N'),order('N__stop_loss','stop_market',{price:73.25}),order('N__take_profit','take_profit_market',{price:149.5})],responses:[ok]});
add('updateOrder',[{order:order('A__take_profit','take_profit_market'),update:{price:151.5}}],{orders:[order('A__stop_loss','stop_market',{price:73.25}),order('A__take_profit','take_profit_market',{price:149.5})],responses:[ok]});
add('cancelAlgoOrders',[[order('A','stop_market')]],{responses:[{code:'5',msg:'outer error',data:[null]}]});
// The confirmed marginMode wins over the argument and re-types only the flat position.
add('changeMarginType',[true,'BTCUSDT'],{positions:[{...pos('P0'),contracts:0},pos('P1')],fetchedPositions:[pos('P1',11)],responses:[{code:'0',data:{marginMode:'cross'}}]});
assert.equal(cases.length,53,'independent_case_floor');
const raw=value=>JSON.stringify(value,(_,v)=>v===undefined?{__undefined:true}:v);
async function run(input){
 const inputRaw=raw(input);
 const store=new DefaultStore();store.update({markets:[market],positions:input.positions??[],orders:input.orders??[],fetchedLeverage:{KEEP:{buyLeverage:3,sellLeverage:5},...(input.leverage?{BTCUSDT:input.leverage}:{})}});
 const state={store,trace:[]};active=state;
 for(const [name,factory] of Object.entries(factories))state[name]=factory.call(state);
 state.emitAndLogError=value=>state.trace.push(['emit',value===undefined?{__undefined:true}:value]);
 state.resolveCallerClientOrderId=opts=>{state.trace.push(['guard',opts.clientOrderId??null]);if(input.guardThrow)throw Error('caller ID rejection')};
 state.assertCallerClientOrderIds=opts=>{state.trace.push(['batchGuard',opts.length]);if(input.guardThrow)throw Error('caller ID rejection')};
 state.getOrFetchMarginType=async(symbol,refetch)=>{state.trace.push(['margin',symbol??null,!!refetch]);return input.margin??'cross'};
 state.getOrFetchPositionMode=async()=>{state.trace.push(['mode']);return input.mode??'oneway'};
 state.fetchPositions=async symbols=>{state.trace.push(['fetchPositions',symbols]);return input.fetchedPositions??[]};
 state.formatNormalOrder=async opts=>{state.trace.push(['normal',opts]);return [payload(input.normalSize??'12')]};
 state.formatAlgoOrder=async opts=>{state.trace.push(['algo',opts]);return payload()};
 state.formatTriggerOrder=async(opts,source)=>{state.trace.push(['trigger',opts,source??null]);return payload()};
 // Cancellation preservation/update delegate through this capability. The placement rows themselves use their real methods.
 if(['cancelOrders','updateOrder'].includes(input.name))state.placeOrder=async opts=>{state.trace.push(['place',opts]);return ['replacement']};
 const responses=[...(input.responses??[])];state.xhr={post:async(url,payload)=>{
  state.trace.push(['request',url,payload]);assert(responses.length,'independent_response_queue');const response=responses.shift();
  if(response.throw){const error=Error(response.throw);if(response.hint!==undefined)error.response={data:{msg:response.hint}};throw error;}return {data:response};
 }};
 let result;try{result={value:await state[input.name](...input.args)}}catch(error){result={error:error.message}}
 return {inputRaw,name:input.name,result,trace:state.trace,positions:store.positions,leverage:store.fetchedLeverage,orders:store.orders};
}
(async()=>{
 if(process.argv[2]==='goneIdLookup'){
  const cases=[];
  for(const [label,id,key] of [['normal-number',123,'123'],['normal-string','123','123'],['normal-boolean',true,'true'],['normal-array',[123],'123'],['normal-object',{},'[object Object]'],['normal-zero',0,'0']]){
   const orders=[order(key),order(key+'4')];
   cases.push({label,...await run({name:'cancelOrders',args:[[orders[0]]],orders,responses:[{code:'1',data:[{code:'1000',orderId:id,msg:'already gone'}]}]})});
  }
  for(const [label,id] of [['algo-number',123],['algo-string','123']]){
   const orders=[order('123__stop_loss','stop_market'),order('1234__stop_loss','stop_market')];
   cases.push({label,...await run({name:'cancelOrders',args:[[orders[0]]],orders,responses:[{code:'1',data:[{code:'1000',tpslId:id,msg:'already gone'}]}]})});
  }
  assert.equal(cases.length,8);
  console.log(raw({market,cases}));return;
 }
 const outputs=[];for(const c of cases)outputs.push(await run(c));
 assert.deepEqual(outputs[0].result.value,['N1','N2']);assert.equal(outputs[1].trace.filter(t=>t[0]==='request')[0][2].length,20);
 assert.deepEqual(outputs[15].result.value,['normal-second','algo-third','trigger-first']);
 assert.deepEqual(outputs[25].orders.map(o=>o.id),['123__stop_loss']);
 assert.equal(outputs[39].positions[0].leverage,50);assert.equal(outputs[41].positions[0].leverage,3);
 const boundaries=[await run({name:'updateOrder',args:[{order:order('A'),update:{price:null}}]})];
 assert.equal(boundaries[0].trace.at(-1)[0],'place');assert.equal(boundaries[0].trace.at(-1)[1].price,101.5,'source_null_update_admitted');
 // Actual-source parallel witness: neither response has settled when both writes exist.
 const delayedStore=new DefaultStore();delayedStore.update({markets:[market],positions:[pos('P1')],fetchedLeverage:{}});
 const starts=[],resolvers=[];
 const delayed={store:delayedStore,getOrFetchMarginType:async()=>enums.MarginType.Isolated,getOrFetchPositionMode:async()=>enums.PositionMode.Hedged,emitAndLogError:()=>{},xhr:{post:(url,payload)=>{starts.push(payload.positionSide);return new Promise(resolve=>resolvers.push(resolve));}}};
 const pending=factories.setLeverage.call(delayed)('BTCUSDT',9).then(()=>null,e=>e.message);
 let iteration=0;for(;iteration<8&&starts.length<2;iteration++)await Promise.resolve();
 assert.deepEqual(starts,['long','short'],'source_parallel_both_started');assert.equal(delayedStore.positions[0].leverage,3,'source_parallel_before_settlement');
 resolvers[0]({data:{code:'0'}});resolvers[1]({data:{code:'5',msg:'short denied'}});assert.equal(await pending,'short denied');assert.equal(delayedStore.positions[0].leverage,3,'source_parallel_failure_no_mutation');
 const immediateStarts=[];
 const immediate={...delayed,xhr:{post:async(url,payload)=>{immediateStarts.push(payload.positionSide);if(payload.positionSide==='long')throw Error('early transport rejection');return {data:{code:'0'}};}}};
 assert.equal(await factories.setLeverage.call(immediate)('BTCUSDT',9).then(()=>null,e=>e.message),'early transport rejection');assert.deepEqual(immediateStarts,['long','short'],'source_parallel_rejection_still_starts_both');
 const zeroWitnesses=[];
 for(const [input,min,max,expected] of [[-0,0,0,false],[-0,-0,-0,true],[0,-0,-0,true],[0,-0,0,false]]){
  const zeroStore=new DefaultStore();zeroStore.update({markets:[{...market,limits:{...market.limits,leverage:{min,max}}}],positions:[],fetchedLeverage:{}});
  const zeroRequests=[];
  const zeroState={store:zeroStore,getOrFetchMarginType:async()=>enums.MarginType.Cross,getOrFetchPositionMode:async()=>enums.PositionMode.OneWay,emitAndLogError:()=>{},xhr:{post:async(url,payload)=>{zeroRequests.push(payload);return {data:{code:'0'}};}}};
  await factories.setLeverage.call(zeroState)('BTCUSDT',input);
  const leverage=zeroStore.fetchedLeverage.BTCUSDT;assert.equal(Object.is(leverage.buyLeverage,-0),expected,'source_zero_native_buy');assert.equal(Object.is(leverage.sellLeverage,-0),expected,'source_zero_native_sell');
  assert.equal(zeroRequests.length,1,'independent_zero_request_floor');assert.equal(zeroRequests[0].leverage,'0','source_zero_wire_after_native');
  zeroWitnesses.push({input:Object.is(input,-0)?'-0':'0',min:Object.is(min,-0)?'-0':'0',max:Object.is(max,-0)?'-0':'0',negative:expected});
 }
 assert.equal(zeroWitnesses.length,4,'independent_zero_floor');
 console.log(raw({header:{architecture:process.arch,nodeVersion:process.version},cases:outputs,boundaries,parallelWitness:{starts,immediateStarts,microtaskIterations:iteration},zeroWitnesses,stoppedAfter:53}));
})().catch(error=>{console.error(error);process.exitCode=1});
