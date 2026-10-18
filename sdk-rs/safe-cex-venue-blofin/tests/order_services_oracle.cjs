const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {root,load,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const constants=load(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.types.ts'));
const {DefaultStore,isTerminalOrder}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const {adjust}=load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts'));
const {omitUndefined}=load(path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts'));
const {inverseObj}=load(path.join(root,'packages/safe-cex/src/utils/inverseObj.ts'));
const {createPositionId}=load(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(s=>ts.isClassDeclaration(s)&&s.name?.text==='BlofinExchange');
const names=['formatTriggerOrder','formatAlgoOrder','formatNormalOrder','getOrFetchPositionMode','getOrFetchMarginType','fetchLeverageForSymbol','changePositionMode','placeTriggerOrder','cancelSymbolOrders','getPositionSide','toBlofinTriggerType','applyConfirmedPositionMode'];
let active;
const context={...enums,...constants,adjust,omitUndefined,inverseObj,createPositionId,isTerminalOrder,times:dependencies('lodash/times'),uniqBy:dependencies('lodash/uniqBy'),uuid:()=>{const id='u'+String(++active.uuidCount).padStart(3,'0');active.trace.push(['uuid',id]);return id;},process:{env:{NODE_ENV:'production'}}};
const factories={};
for(const name of names){const member=klass.members.find(m=>m.name?.text===name);assert(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),'method_shape:'+name);const code=ts.transpileModule(`return function(){return ${member.initializer.getText(ast)}};`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;factories[name]=new Function(...Object.keys(context),code)(...Object.values(context));}
const market=(info={lotSize:'3',minSize:'0.2'})=>({id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:0.01,max:100},leverage:{min:1,max:50}},contractSize:0.001,info});
const order=(id,type,price,symbol='BTCUSDT',side='buy',amount=0)=>({id,symbol,type,price,side,amount,status:'open',filled:0,remaining:amount,reduceOnly:false,lastUpdate:123,info:{witness:id}});
const pos=(side,contracts,positionMode,marginType)=>({id:'BTCUSDT:'+side,symbol:'BTCUSDT',side,contracts,positionMode,marginType,entryPrice:101,notional:102,leverage:3,unrealizedPnl:4,liquidationPrice:5});
const base={symbol:'BTCUSDT',type:'limit',side:'buy',amount:12.5,price:101.375,stopLoss:73.125,takeProfit:149.375};
function fmt(name,opts,extra={}){return {kind:'format',name,opts:{...base,...opts},...extra};}
const formats=[
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105.375,tpTriggerType:'index_price',slTriggerType:'mark_price'}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105.375},{source:{instId:'BTC-USDT',marginMode:'isolated',positionSide:'short',triggerPriceType:'index'}}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105},{source:{instId:'ETH-USDT'}}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105,reduceOnly:true}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105,close:true}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105,timeInForce:'ImmediateOrCancel'}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105,amount:0}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:0}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105},{market:market({lotSize:'1tail',minSize:'1'})}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105,amount:6.76,price:0},{market:market({minSize:'0.2'}),production:false}),
 fmt('formatTriggerOrder',{type:'conditional',triggerPrice:105},{market:market(null)}),
 fmt('formatNormalOrder',{}),fmt('formatNormalOrder',{timeInForce:'PostOnly'}),fmt('formatNormalOrder',{type:'market'}),
 fmt('formatNormalOrder',{price:undefined}),fmt('formatNormalOrder',{reduceOnly:true}),fmt('formatNormalOrder',{clientOrderId:'callerA9'}),
 fmt('formatNormalOrder',{amount:310},{market:market({lotSize:'1'})}),fmt('formatNormalOrder',{amount:300},{market:market({lotSize:'1'})}),
 fmt('formatNormalOrder',{}, {market:market({lotSize:'0'})}),fmt('formatNormalOrder',{}, {market:market({lotSize:'0.5tail'})}),
 fmt('formatNormalOrder',{}, {production:false,proxy:true}),fmt('formatNormalOrder',{amount:Infinity}),
 fmt('formatAlgoOrder',{type:'take_profit_market',amount:0},{orders:[order('sl1','stop_market',71.125),order('sl2','stop_market',72.375),order('wrong','stop_market',99,'ETHUSDT')]}),
 fmt('formatAlgoOrder',{type:'stop_market',amount:0},{orders:[order('tp1','take_profit_market',155.375)]}),
 fmt('formatAlgoOrder',{type:'stop_market',amount:1.2}),fmt('formatAlgoOrder',{type:'stop_market',price:undefined,stopLoss:undefined,takeProfit:undefined}),
 fmt('formatAlgoOrder',{type:'take_profit_market',price:105.375,stopLoss:undefined,takeProfit:undefined}),
 fmt('formatAlgoOrder',{type:'trailing_stop_market',amount:3,stopLoss:undefined,takeProfit:undefined}),
];
assert.equal(formats.length,29,'independent_format_floor');
const services=[
 {name:'getOrFetchPositionMode',args:['BTCUSDT',true],positions:[pos('long',0,'hedged','cross')],responses:[]},
 {name:'getOrFetchPositionMode',args:[null,false],mode:'hedged',responses:[]},
 {name:'getOrFetchPositionMode',args:[null,true],mode:'hedged',responses:[{code:'0',data:{positionMode:'net_mode'}}]},
 {name:'getOrFetchPositionMode',args:[],responses:[{code:0,msg:'numeric code is failure',data:{positionMode:'net_mode'}}]},
 {name:'getOrFetchPositionMode',args:[],responses:[{code:'0',data:{positionMode:'other'}}]},
 {name:'getOrFetchPositionMode',args:[],responses:[{code:'0',data:null}]},
 {name:'getOrFetchMarginType',args:['BTCUSDT',true],positions:[pos('long',0,'hedged','cross'),pos('short',2,'hedged','isolated')],responses:[]},
 {name:'getOrFetchMarginType',args:[null,false],margin:'isolated',responses:[]},
 {name:'getOrFetchMarginType',args:[null,true],margin:'cross',responses:[{code:'0',data:{marginMode:'isolated'}}]},
 {name:'getOrFetchMarginType',args:[],responses:[{code:'5',msg:'margin denied'}]},
 {name:'getOrFetchMarginType',args:[],responses:[{code:'0',data:{marginMode:'other'}}]},
 {name:'getOrFetchMarginType',args:[],responses:[{throw:'transport probe'}]},
 {name:'fetchLeverageForSymbol',args:['MISSING'],responses:[]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'isolated',responses:[{code:'0',data:[{instId:'BTC-USDT',positionSide:'short',leverage:'17tail',marginMode:'isolated'},{instId:'BTC-USDT',positionSide:'long',leverage:'11.5',marginMode:'cross'}]}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{code:'0',data:[{positionSide:'net',leverage:'9',marginMode:'isolated'}]}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{code:'0',data:[{positionSide:'other',leverage:'7'}]}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{code:'0',data:[]}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{code:'0',data:{length:0}}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{code:'0',data:[{positionSide:'long',leverage:'Infinity'}]}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{code:'5',msg:'leverage denied'}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{throw:'HTTP probe',responseMsg:'remote hint'}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{code:'0',data:[null]}]},
 {name:'fetchLeverageForSymbol',args:['BTCUSDT'],margin:'cross',responses:[{throw:'HTML failure probe',responseBodyText:'<h1>Failure</h1>'}]},
 {name:'changePositionMode',args:[false],positions:[pos('long',1,'hedged','isolated'),pos('short',2,'hedged','cross')],responses:[{code:'0',data:{positionMode:'net_mode'}}]},
 {name:'changePositionMode',args:[false],responses:[{code:'0',data:{positionMode:'long_short_mode'}}]},
 {name:'changePositionMode',args:[true],mode:'oneway',responses:[{code:'5',msg:'mode denied'}]},
 {name:'placeTriggerOrder',args:[{instId:'BTC-USDT',size:'12',triggerPrice:'105.5'}],responses:[{code:'0',data:{algoId:'A17'}}]},
 {name:'placeTriggerOrder',args:[{}],responses:[{code:'0',data:{algoId:17,code:'0'}}]},
 {name:'placeTriggerOrder',args:[{}],responses:[{code:'0',msg:'outer',data:{code:'7',msg:'inner',algoId:'A'}}]},
 {name:'placeTriggerOrder',args:[{}],responses:[{code:0,data:{algoId:'A'}}]},
 {name:'placeTriggerOrder',args:[{}],responses:[{code:'0',data:{algoId:0}}]},
 {name:'cancelSymbolOrders',args:['BTCUSDT'],orders:[order('a','limit',1),order('b','stop_market',2),order('c','limit',3,'ETHUSDT')],responses:[]},
 {name:'cancelSymbolOrders',args:['MISSING'],orders:[order('a','limit',1)],responses:[]},
 {name:'cancelSymbolOrders',args:['BTCUSDT'],orders:[order('a','limit',1)],responses:[],cancelThrow:'cancel probe'},
];
assert.equal(services.length,34,'independent_service_floor');
function raw(value){return JSON.stringify(value,(_,v)=>typeof v==='number'&&!Number.isFinite(v)?{__nativeNumber:String(v)}:v)}
function snapshot(store){return {positions:store.positions.map(p=>({id:p.id,entryPrice:p.entryPrice,positionMode:p.positionMode,marginType:p.marginType,contracts:p.contracts})),leverage:store.fetchedLeverage}}
async function run(input){
 const store=new DefaultStore();store.update({markets:[input.market??market()],orders:input.orders??[],positions:input.positions??[],fetchedLeverage:{KEEP:{buyLeverage:3,sellLeverage:5,positionMode:'oneway'}}});
 const state={store,options:{tealstreetProxy:input.proxy??false},_accountPositionMode:input.mode,_accountMarginType:input.margin,trace:[],uuidCount:0};active=state;
 context.process.env.NODE_ENV=input.production===false?'development':'production';
 for(const [name,factory] of Object.entries(factories))state[name]=factory.call(state);
 state.emitAndLogError=(value)=>{state.trace.push(['emit',value===undefined?{undefined:true}:value])};
 state.log=(text,severity)=>{state.trace.push(['log',text,severity])};
 state.resolveCallerClientOrderId=opts=>{state.trace.push(['client',opts.clientOrderId??null]);return opts.clientOrderId};
 state.assertSingleNativeClientOrderId=(id,count)=>{state.trace.push(['split',id??null,count]);if(id!==undefined&&count!==1)throw Error('caller ID split probe')};
 state.cancelAlgoOrders=async orders=>{state.trace.push(['cancelAlgo',orders.map(o=>o.id)]);store.removeOrders(orders.map(o=>({id:o.id})))};
 state.cancelOrders=async orders=>{state.trace.push(['cancel',orders.map(o=>o.id)]);if(input.cancelThrow)throw Error(input.cancelThrow)};
 const responses=[...(input.responses??[])];
 const xhr=async(method,url,payload,params)=>{state.trace.push(['request',method,url,payload??null,params??null]);assert(responses.length,'independent_response_queue');const response=responses.shift();if(response.throw){const error=Error(response.throw);if(response.responseMsg)error.response={data:{msg:response.responseMsg}};if(response.responseBodyText)error.response={data:response.responseBodyText};throw error}return {data:response}};
 state.xhr={get:(url,config)=>xhr('GET',url,null,config?.params),post:(url,payload)=>xhr('POST',url,payload,null)};
 if(input.kind==='format'){
  state.getOrFetchMarginType=async symbol=>{state.trace.push(['margin',symbol]);return input.margin??'cross'};
  state.getOrFetchPositionMode=async symbol=>{state.trace.push(['mode',symbol]);return input.mode??'hedged'};
 }
 let result;
 try {const value=input.kind==='format'?await state[input.name](input.opts,input.source):await state[input.name](...(input.args??[]));result={value:value===undefined?{undefined:true}:value};}
 catch(error){result={error:typeof error==='object'?error.message:String(error)}}
 return {inputRaw:raw(input),name:input.name,result,trace:state.trace,cache:{mode:state._accountPositionMode??null,margin:state._accountMarginType??null},store:snapshot(store)};
}
(async()=>{
 const cases=[];for(const input of [...formats,...services])cases.push(await run(input));assert.equal(cases.length,63);
 // Fixed independent anchors before JSON output or native consumption.
 assert.equal(cases[0].result.value.size,'12');assert.equal(cases[0].result.value.orderPrice,'101.5');assert.equal(cases[0].result.value.attachAlgoOrders[0].tpTriggerPrice,'149.5');assert.equal(cases[0].result.value.attachAlgoOrders[0].slTriggerPrice,'73.25');
 assert.equal(cases[17].result.value.length,5);assert.equal(cases[18].result.value.length,3);assert.equal(cases[19].result.value.length,0);
 const boundary=fmt('formatNormalOrder',{amount:10001,stopLoss:undefined,takeProfit:undefined},{market:{...market({lotSize:'1'}),limits:{amount:{min:1,max:1},leverage:{min:1,max:50}}}});const admitted=await run(boundary);assert.equal(admitted.result.value.length,10001,'source_allocation_admitted');
 const remainderBoundary=fmt('formatNormalOrder',{amount:9999.5,stopLoss:undefined,takeProfit:undefined},{market:{...market({lotSize:'0.5'}),limits:{amount:{min:0.5,max:1},leverage:{min:1,max:50}}}});const remainder=await run(remainderBoundary);assert.equal(remainder.result.value.length,10001,'source_remainder_allocation_admitted');
 console.log(JSON.stringify({header:{architecture:process.arch,nodeVersion:process.version},cases,stoppedAfter:{formats:29,services:34},allocationWitness:10001,allocationRemainderWitness:10001}));
})().catch(error=>{console.error(error);process.exitCode=1});
