const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {root,ts,load,capture}=require('./types-oracle.cjs');
const types=load(path.join(root,'packages/safe-cex/src/types.ts')),venue=load(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const {createPositionId}=load(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts'));
function tree(file){return ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);}
function decl(t,name){const n=t.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(t)===name));assert.ok(n,name);return n.getText(t);}
const t=tree(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.exchange.ts')),u=tree(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts'));
const cls=t.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='WeexExchange');
const methods=['fetchPositions','placeOrder','placeOrders','updateOrder','placeRegularOrderBatch','formatOrderPayload','inferPositionSide','isBatchableRegularOrder','parseOrderResult'];
const fields=cls.members.filter(n=>ts.isPropertyDeclaration(n)&&methods.includes(n.name.getText(t)));assert.equal(fields.length,9,'trading::source_methods');
const helpers=['toNumber','toIntegerString','normalizeTimestampToMs','normalizeOrderSide','normalizeOrderStatus','normalizeOrderType','normalizeTriggerType','normalizePositionSide','normalizeOrder','normalizeAlgoOrder','normalizePosition','fakePositionsForMarket'];
const math=tree(path.join(root,'packages/safe-cex/src/utils/safe-math.ts'));
const cache=math.statements.filter(n=>n.getText(math).startsWith('for (let i = 0;'));
assert.equal(cache.length,1);
const omit=tree(path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts'));
const source=[decl(math,'POW10_CACHE'),...cache.map(n=>n.getText(math)),...['pow10','afterDecimal','adjust'].map(n=>decl(math,n)),decl(omit,'omitUndefined'),decl(t,'extractArray'),...helpers.map(n=>decl(u,n)),'export class Slice {'+fields.map(n=>n.getText(t)).join('\n')+'}'].join('\n');
const compiled=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(compiled.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
function enc(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='string')return{kind:'string',units:units(v)};if(typeof v==='number')return{kind:'number',bits:capture(v).bits};if(typeof v==='boolean')return{kind:'boolean',value:v};if(Array.isArray(v))return{kind:'array',items:Array.from({length:v.length},(_,i)=>i in v?enc(v[i]):{kind:'hole'})};return{kind:'object',fields:Object.entries(v).map(([k,x])=>[k,enc(x)])};}
const opts={symbol:'BTC',type:'limit',side:'buy',amount:1.23,price:10.24,clientOrderId:'A'};
const market={symbol:'BTC',precision:{price:.1,amount:.01}};
const raw={orderId:'100',symbol:'BTC',side:'BUY',origQty:'-7',executedQty:'2',price:'13',stopPrice:'14',status:'FILLED',type:'LIMIT',timeInForce:'GTX',updateTime:1700000000000};
const pos={symbol:'BTC',side:'SHORT',size:'-3',openValue:'-33',leverage:'7',unrealizePnl:'-2',realizePnl:'3',liquidatePrice:'17',marginSize:'4',separatedMode:'SEPARATED',marginType:'ISOLATED',updatedTime:1700000000000};
const cases=[];
function add(id,method,patch={}){const input={method,opts:{...opts},raw:{...raw},market:{...market},fallback:{},orders:[{...opts}],update:{order:{id:'100',...opts},update:{price:11.37}},markets:[{...market},{symbol:'ETH',precision:{price:.1,amount:.01}}],configs:{},stored:[{id:'OLD',symbol:'BTC'},{id:'ETH',symbol:'ETH'}],data:[],replies:[],publicOnly:false,accountMode:'oneway',fail:'',clientResult:'provided',holes:[],...patch};cases.push({id,input});}
add('order_distinct','normalizeOrder');add('order_aliases','normalizeOrder',{raw:{id:0,s:'ETH',orderSide:'SELL',quantity:'4',cumFillSize:'9',actualPrice:0,triggerPrice:'12',algoStatus:'CANCELED',orderType:'STOP',reduceOnly:'false',tpTriggerPrice:'15',slTriggerPrice:'16',TpWorkingType:'MARK_PRICE',SlWorkingType:'INDEX_PRICE',updatedTime:1700000000}});
add('order_no_id','normalizeOrder',{raw:{symbol:'BTC',side:'BUY'}});add('order_no_symbol','normalizeOrder',{raw:{orderId:'1',side:'BUY'},market:{}});add('order_bad_side','normalizeOrder',{raw:{...raw,side:'bad'}});add('order_zero_take','normalizeOrder',{raw:{...raw,tpTriggerPrice:0,takeProfit:99,slTriggerPrice:null,stopLoss:16}});
add('order_market_fallback','normalizeOrder',{raw:{id:'1',side:'BUY',price:0},market:{symbol:'ETH'}});add('order_signed_zero','normalizeOrder',{raw:{...raw,price:'-0',stopPrice:'-0',origQty:'-0',executedQty:'-0'}});
add('algo_virtual_pair','normalizeAlgoOrder',{raw:{algoId:'200',symbol:'BTC',side:'SELL',quantity:'-4',orderType:'STOP',triggerPrice:'12',tpTriggerPrice:'17',slTriggerPrice:'9',tpPrice:'5',slPrice:'6',algoStatus:'NEW'}});
add('algo_protective','normalizeAlgoOrder',{raw:{algoId:'200',symbol:'BTC',side:'BUY',quantity:'2',orderType:'STOP_MARKET',planType:'STOP_LOSS',reduceOnly:true,triggerPrice:'9'}});
add('algo_null_info','normalizeAlgoOrder',{raw:{id:'100',symbol:'BTC',side:'BUY',algoId:null,orderId:null,orderType:null,planType:null,algoStatus:null,status:null}});
add('algo_virtual_no_base','normalizeAlgoOrder',{raw:{algoId:'200',symbol:'BTC',orderSide:'SELL',tpTriggerPrice:17,slTriggerPrice:0,size:2}});
add('algo_missing_parent','normalizeAlgoOrder',{raw:{symbol:'BTC',side:'BUY',tpTriggerPrice:9}});add('algo_tpsl','normalizeAlgoOrder',{raw:{...raw,algoId:'1',orderType:'TAKE_PROFIT',algoType:'TPSL'}});
add('position_distinct','normalizePosition',{raw:pos});add('position_fallback','normalizePosition',{raw:{symbol:'ETH',positionSide:'LONG',positionAmt:0,averagePrice:'123',marginType:'bad',separatedType:'bad'},fallback:{leverage:'9',marginType:'isolated',positionMode:'hedged'}});
add('position_missing_symbol','normalizePosition',{raw:{},market:{}});add('position_market_symbol','normalizePosition',{raw:{size:2,notional:26}});add('position_zero_leverage','normalizePosition',{raw:{...pos,leverage:0},fallback:{leverage:99}});
add('payload_limit','formatOrderPayload');add('payload_market','formatOrderPayload',{opts:{...opts,type:'market',price:0}});add('payload_limit_zero','formatOrderPayload',{opts:{...opts,price:0}});add('payload_mint','formatOrderPayload',{opts:{...opts,clientOrderId:null}});
add('payload_conditional','formatOrderPayload',{opts:{...opts,type:'conditional',triggerPrice:12.36,takeProfit:17.39,stopLoss:8.22,reduceOnly:true}});
add('payload_conditional_market','formatOrderPayload',{opts:{...opts,type:'conditional',price:0,triggerPrice:12}});add('payload_invalid_trigger','formatOrderPayload',{opts:{...opts,type:'conditional',triggerPrice:'12'}});
add('payload_stop','formatOrderPayload',{opts:{...opts,type:'stop_market',side:'sell',reduceOnly:true}});add('payload_take','formatOrderPayload',{opts:{...opts,type:'take_profit_market',tpTriggerType:'index_price'}});
add('payload_missing_price','formatOrderPayload',{opts:{...opts,type:'stop_market',price:0}});add('payload_trailing','formatOrderPayload',{opts:{...opts,type:'trailing_stop_market'}});add('payload_missing_market','formatOrderPayload',{markets:[]});
add('single_regular','placeOrder');add('single_conditional','placeOrder',{opts:{...opts,type:'conditional',triggerPrice:12}});add('single_post_only','placeOrder',{opts:{...opts,timeInForce:'PostOnly'}});
add('single_array','placeOrder',{replies:[[{orderId:'101'},{orderId:'102'}]]});add('single_rejection','placeOrder',{replies:[{success:false,errorMessage:'DENIED'}]});add('single_remote_failure','placeOrder',{fail:'request:end',traceErrors:true});add('single_factory_failure','placeOrder',{fail:'request:start',traceErrors:true});add('single_resolver_failure','placeOrder',{fail:'resolve'});
const order=(n,patch={})=>({...opts,clientOrderId:'C'+n,...patch});
add('batch_reordered','placeRegularOrderBatch',{orders:[order(1,{timeInForce:'PostOnly'}),order(2)],reverseReply:true});add('batch_missing_id','placeRegularOrderBatch',{orders:[order(1,{clientOrderId:''})]});add('batch_duplicate','placeRegularOrderBatch',{orders:[order(1),order(1)]});
add('batch_shape','placeRegularOrderBatch',{replies:[{}]});add('batch_count','placeRegularOrderBatch',{orders:[order(1),order(2)],replies:[[]]});add('batch_unknown','placeRegularOrderBatch',{replies:[[{clientOrderId:'OTHER',success:true,orderId:'100'}]]});
add('batch_duplicate_echo','placeRegularOrderBatch',{orders:[order(1),order(2)],replies:[[{clientOrderId:'C1',success:true,orderId:'100'},{clientOrderId:'C1',success:true,orderId:'101'}]]});
add('batch_missing_success','placeRegularOrderBatch',{replies:[[{clientOrderId:'A',orderId:'100'}]]});add('batch_null_error_code','placeRegularOrderBatch',{replies:[[{clientOrderId:'A',success:false,errorCode:null}]]});add('batch_bad_order_id','placeRegularOrderBatch',{replies:[[{clientOrderId:'A',success:true,orderId:'0'}]]});
add('batch_partial_reject','placeRegularOrderBatch',{orders:[order(1,{timeInForce:'PostOnly'}),order(2)],replies:[[{clientOrderId:'C2',success:false,errorCode:'NO'},{clientOrderId:'C1',success:true,orderId:'123'}]]});
add('batch_response_hole','placeRegularOrderBatch',{orders:[order(1),order(2)],replyHoles:[0],customIterator:true});
add('orders_chunking','placeOrders',{orders:Array.from({length:7},(_,i)=>order(i+1))});add('orders_mixed','placeOrders',{orders:[order(1),order(2,{type:'conditional',triggerPrice:12}),order(3)]});add('orders_empty','placeOrders',{orders:[]});add('orders_assert_failure','placeOrders',{fail:'assertIds'});add('orders_body_error_close','placeOrders',{orders:[order(1)],fail:'format',customIterator:true});add('orders_live_append','placeOrders',{orders:[order(1,{type:'conditional',triggerPrice:12})],appendOrder:true});
add('update_regular','updateOrder');add('update_conditional','updateOrder',{update:{order:{id:'100',...opts,type:'conditional',price:12,info:{price:'8'}},update:{price:13,amount:3}}});add('update_cancel_failure','updateOrder',{fail:'cancel:end'});
add('update_stop','updateOrder',{update:{order:{id:'100',...opts,type:'stop_market',info:{executePrice:'8'}},update:{price:12.39}}});add('update_take','updateOrder',{update:{order:{id:'100',...opts,type:'take_profit_market'},update:{price:12.39}}});
add('update_quantity','updateOrder',{update:{order:{id:'100',...opts,type:'stop_market'},update:{amount:2}}});add('update_virtual','updateOrder',{update:{order:{id:'100__sl',...opts,type:'stop_market'},update:{price:1}}});add('update_rejected','updateOrder',{update:{order:{id:'100',...opts,type:'stop_market'},update:{price:1}},replies:[{success:false}]});
add('positions_open_and_fake','fetchPositions',{data:[pos,{...pos,symbol:'BTC',size:1,openValue:4}],configs:{BTC:{positionMode:'hedged',marginType:'isolated',crossLeverage:7}},accountMode:'hedged'});
add('positions_requested','fetchPositions',{symbols:['BTC'],data:[pos,{...pos,symbol:'ETH'}]});add('positions_empty_requested','fetchPositions',{symbols:[],data:[]});add('positions_public','fetchPositions',{publicOnly:true});add('positions_failure_filtered','fetchPositions',{symbols:['BTC'],fail:'request:end',traceErrors:true});add('positions_failure_empty','fetchPositions',{symbols:[],fail:'request:end'});add('positions_default_leverage','fetchPositions',{symbols:['BTC'],configs:{BTC:{crossLeverage:0,isolatedLongLeverage:0}},data:[]});
add('algo_virtual_zero_execution','normalizeAlgoOrder',{raw:{algoId:'1',symbol:'BTC',side:'BUY',quantity:2,tpTriggerPrice:17,tpPrice:0}});
add('batch_null_error_message','placeRegularOrderBatch',{replies:[[{clientOrderId:'A',success:false,errorMessage:null}]]});
add('single_null_result','placeOrder',{replies:[null]});
add('positions_failure_unfiltered','fetchPositions',{fail:'request:end',traceErrors:true});
// Optional production-binding supplement; the original76-case corpus is unchanged.
const bindingIds = process.argv[2] === 'binding-batch-ids' ? JSON.parse(process.argv[3]) : null;
if (bindingIds) {
  cases.length = 0;
  for (const [index, id] of bindingIds.entries()) {
    add('binding_id_' + index, 'placeOrders', {
      orders: [order(1, {clientOrderId:'synthetic-client-1',timeInForce:'PostOnly'})],
      replies: [[{clientOrderId:'synthetic-client-1',success:true,orderId:id}]],
    });
  }
}
async function run(c){const input=structuredClone(c.input),events=[],requests=[],reported=[],updates=[],resolved=[],infer=[],prefix=[];let initial;let requestCount=0,appendDone=false,instance,error=null,result;
 const check=stage=>{if(input.fail===stage)throw new RangeError('fail:'+stage);};const record=stage=>{events.push(stage);check(stage);};
 function requestError(stage){const error=new RangeError('fail:'+stage);error.response={data:{errorMessage:'VENUE',msg:'REMOTE',message:'ALT'}};if(input.traceErrors){const message=error.message;Object.defineProperty(error,'message',{get(){events.push('error:message');return message;}});error.response.data=new Proxy(error.response.data,{get(t,k){events.push('error:'+k);return t[k];}});const response=error.response;Object.defineProperty(error,'response',{get(){events.push('error:response');return response;}});}return error;}
 const scope={exports:{},...types,...venue,createPositionId,omitBy:require('lodash/omitBy'),uniqBy:require('lodash/uniqBy'),uuid(){record('uuid');return '0123456789-0123456789-0123456789-0123456789';}};
 vm.runInNewContext(compiled.outputText,scope);instance=new scope.exports.Slice();
 instance.store={markets:input.markets,marketsBySymbol:Object.fromEntries(input.markets.map(m=>[m.symbol,m])),positions:input.stored,updateOrder(a,b){record('storeUpdate');updates.push({a:structuredClone(a),b:structuredClone(b)});}};
 instance.accountPositionMode=input.accountMode;instance.isPublicDataOnly=input.publicOnly;
 Object.defineProperty(instance,'emitAndLogError',{get(){events.push('errorSink');return message=>{record('emit');reported.push(message);};}});
 instance.resolveCallerClientOrderId=opts=>{record('resolve');resolved.push(structuredClone(opts));return opts.clientOrderId;};instance.assertCallerClientOrderIds=orders=>record('assertIds');instance.getSymbolConfigFromCache=symbol=>{record('config');return input.configs[symbol];};
 const format=instance.formatOrderPayload;instance.formatOrderPayload=opts=>{record('format');return format(opts);};const inf=instance.inferPositionSide;instance.inferPositionSide=opts=>{record('infer');infer.push(structuredClone(opts));return inf(opts);};
 instance.cancelOrders=orders=>{record('cancel:start');return Promise.resolve().then(()=>{record('cancel:end');});};
 instance.xhr={get(url){return request('GET',url,undefined);},post(url,payload){return request('POST',url,payload);}};
 function request(method,url,payload){events.push('request:start');if(input.fail==='request:start')throw requestError('request:start');requests.push({method,url,payload:structuredClone(payload)});const index=requestCount++;
  return new Promise((resolve,reject)=>setImmediate(()=>{events.push('request:end');if(input.fail==='request:end')return reject(requestError('request:end'));
   let data=index<input.replies.length?input.replies[index]:url===venue.ENDPOINTS.POSITIONS?input.data:url===venue.ENDPOINTS.BATCH_ORDERS?payload.batchOrders.map((p,i)=>({clientOrderId:p.newClientOrderId,success:true,orderId:String(100+i)})):url===venue.ENDPOINTS.MODIFY_TP_SL_ORDER?{success:true}:{orderId:'100',success:true};
   if(input.reverseReply&&Array.isArray(data))data.reverse();for(const i of input.replyHoles??[])delete data[i];if(input.customIterator&&Array.isArray(data))attachIterator(data);
   if(input.appendOrder&&!appendDone){input.orders.push(order(9));appendDone=true;}resolve({data});}));}
 function attachIterator(a){Object.defineProperty(a,Symbol.iterator,{value(){let i=0;return{next(){return i<a.length?{value:a[i++],done:false}:{done:true};},return(){record('iteratorClose');return{done:true};}};}});}
 for(const i of input.holes)delete input.orders[i];if(input.customIterator)attachIterator(input.orders);
 try{if(['normalizeOrder','normalizeAlgoOrder'].includes(input.method))result=scope.exports[input.method](input.raw,input.market);else if(input.method==='normalizePosition')result=scope.exports.normalizePosition(input.raw,input.market,input.fallback);else{const arg=input.method==='formatOrderPayload'||input.method==='placeOrder'?input.opts:input.method==='updateOrder'?input.update:input.method==='fetchPositions'?input.symbols:input.orders;const p=instance[input.method](arg);prefix.push(...events);initial=structuredClone({requests,reported,updates,resolved,infer});result=await p;}}catch(e){error={name:e.name,message:e.message};}
 return{id:c.id,input:enc(c.input),expected:enc({result,error,requests,reported,updates,resolved,infer,initial}),prefix,events};}
(async()=>{const rows=[];for(const c of cases)rows.push(await run(c));assert.equal(rows.length,bindingIds ? bindingIds.length :76,'trading::fixture_floor');process.stdout.write(JSON.stringify(rows));})().catch(e=>{console.error(e);process.exitCode=1;});
