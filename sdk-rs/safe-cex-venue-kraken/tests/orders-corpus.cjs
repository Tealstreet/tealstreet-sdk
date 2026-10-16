const originalDate=Date;
const orderSource=fs.readFileSync(futuresFile,'utf8');
const orderAst=ts.createSourceFile(futuresFile,orderSource,ts.ScriptTarget.Latest,true);
const triggerDecl=orderAst.statements.flatMap(s=>ts.isVariableStatement(s)?Array.from(s.declarationList.declarations):[]).filter(d=>d.name.getText(orderAst)==='parseTriggerSignal');
assert.equal(triggerDecl.length,1,'actual trigger helper');
const triggerCompiled=ts.transpileModule('('+triggerDecl[0].initializer.getText(orderAst)+')',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const trigger=vm.runInNewContext(triggerCompiled,{OrderStopTriggerType:types.OrderStopTriggerType});
function fallbackOrder(tag,n){return {id:tag,symbol:'STORE-'+tag,status:types.OrderStatus.Open,type:types.OrderType.Limit,side:types.OrderSide.Buy,price:n+1.25,amount:n+2.5,filled:n+3.75,remaining:n+4.125,reduceOnly:true,lastUpdate:n+5.5,info:{tag},timeInForce:types.OrderTimeInForce.PostOnly};}
async function ordersCase(config){
 const trace=[];let clockCalls=0;let release;
 const readError=new Error('orders read failure');readError.name='ReadError';
 const traceError=new Error('orders trace failure');traceError.name='TraceError';
 const clockError=new Error('orders clock failure');clockError.name='ClockError';
 const old=[fallbackOrder('OLD',101)],late=[fallbackOrder('LATE',211)];
 const store={orders:old};
 class HostDate extends originalDate {constructor(...args){if(args.length===0){clockCalls++;if(config.clockFail)throw clockError;super(config.now===undefined?1770900000123.75:config.now)}else super(...args)}}
 const fn=method(futuresFile,'KrakenExchange','fetchOrders',{
  ENDPOINTS:futuresTypes.ENDPOINTS,OrderType:types.OrderType,OrderSide:types.OrderSide,OrderStatus:types.OrderStatus,OrderTimeInForce:types.OrderTimeInForce,parseTriggerSignal:trigger,Date:HostDate,
  console:{trace(error){trace.push({kind:'trace',name:error.name,message:error.message,readIdentity:error===readError,clockIdentity:error===clockError});if(config.replaceTrace)store.orders=late;if(config.traceFail)throw traceError}}
 },{store,xhr:{get(endpoint){trace.push({kind:'read',endpoint});if(config.syncFail)throw readError;return new Promise((resolve,reject)=>{release=()=>config.readFail?reject(readError):resolve({data:config.raw===null?undefined:JSON.parse(config.raw)})})}}});
 const promise=fn();const prefixTrace=trace.length;if(config.replaceAwait)store.orders=late;if(release)release();
 let result,failure=null,alias=false,own=[];try{result=await promise;alias=result===store.orders;own=result.map(o=>({sl:Object.hasOwn(o,'slTriggerType'),tp:Object.hasOwn(o,'tpTriggerType')}));}catch(e){failure={name:e.name,message:e.message,traceIdentity:e===traceError}}
 const captured=result&&capture(result);const storeBefore=capture(store.orders);
 if(alias&&result.length)result[0].id='ALIASED';
 return {config,prefixTrace,trace,clockCalls,result:captured,failure,alias,own,storeBefore,storeAfterAlias:capture(store.orders)};
}
async function ordersCorpus(){
 const cases=[];
 for(const raw of [null,'null','false','0','"primitive"','{}','[]','{"openOrders":null}','{"openOrders":false}','{"openOrders":0}','{"openOrders":""}','{"openOrders":{}}','{"openOrders":"bad"}','{"openOrders":[]}','{"openOrders":[null]}','{"openOrders":[0,false,"text",[],{}]}'])cases.push(await ordersCase({raw}));
 const order={order_id:'order-雪',instrument:'PF_XBTUSD',orderType:'lmt',direction:0,limitPrice:'17.25',unfilledSize:'19.5',filledSize:'23.75',reduceOnly:false,lastUpdateTime:'2026-02-12T12:34:56.789Z',triggerSignal:'last'};
 for(const orderType of ['lmt','LIMIT','post','ioc','fok','stop','stp','take_profit','trailing_stop','mkt','custom-lmt-type',null,false,0,'',[],{}])cases.push(await ordersCase({raw:JSON.stringify({openOrders:[{...order,orderType,type:'post',stopPrice:'31.125'}]})}));
 for(const direction of [null,false,true,'0','buy','BUY','sell',1,-0])cases.push(await ordersCase({raw:JSON.stringify({openOrders:[{...order,direction,side:'BUY'}]})}));
 for(const extra of [{direction:undefined,side:'SELL'},{direction:undefined,side:0},{orderType:'stop',triggerSignal:'mark'},{orderType:'take_profit',triggerSignal:'spot'},{orderType:'trailing_stop',triggerSignal:'unknown'},{orderType:'stop',triggerSignal:null},{order_id:0,orderId:73,instrument:false,symbol:['A','B']},{limitPrice:null,limit_price:'37.5',unfilledSize:null,qty:'41.25',filledSize:null,filled:'43.75'},{orderType:false,type:'ioc',reduce_only:'yes'},{lastUpdateTime:'not a date'},{lastUpdateTime:17},{lastUpdateTime:'2026-02-12T12:34:56'},{lastUpdateTime:false,lastUpdateTimestamp:'2026-02-12T12:34:56+03:00'},{lastUpdateTime:0,lastUpdateTimestamp:0},{lastUpdateTime:null,lastUpdateTimestamp:null}])cases.push(await ordersCase({raw:JSON.stringify({openOrders:[{...order,...extra}]})}));
 cases.push(await ordersCase({raw:'{"openOrders":[{"unfilledSize":-0,"filledSize":-0,"price":1e400,"orderType":"stop","stopPrice":"Infinity","triggerSignal":"last"}]}'}));
 cases.push(await ordersCase({raw:'{"openOrders":[{"unfilledSize":"bad","filledSize":"Infinity","price":-1e400,"direction":-0},{"orderType":"stop","stop_price":"-Infinity","filledSize":"bad"}]}'}));
 for(const config of [{raw:null,syncFail:true},{raw:null,readFail:true},{raw:null,readFail:true,replaceAwait:true},{raw:'{"openOrders":[null]}',replaceTrace:true},{raw:null,readFail:true,traceFail:true},{raw:'{"openOrders":[{}]}',clockFail:true},{raw:'{"openOrders":[{}]}',now:1e16},{raw:'{"openOrders":[{},{}]}',now:-0}])cases.push(await ordersCase(config));
 return {cases};
}
