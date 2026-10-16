const {privateApi}=require('./load-starbase.cjs');
const {captureHeader,numberBits}=require('./special-numbers.cjs');

function order(extra={}){
 return {
  id:'order-1',instrumentName:'BTC-PERP-INTX',legacySavedSymbol:'BTC-PERP',state:'open',type:'limit',
  timeInForce:'good_til_cancelled',direction:'buy',price:3,label:'label',postOnly:false,reduceOnly:false,
  creationTimestampMs:100,lastUpdateTimestampMs:101,gatewayAmount:2,gatewayFilledAmount:0,
  ...extra,
 };
}
function reverseKeys(value){return Object.fromEntries(Object.entries(value).reverse());}
function hydrate(value){
 if(Array.isArray(value))return value.map(hydrate);
 if(value&&typeof value==='object'){
  if(Object.keys(value).length===1&&typeof value.$number==='string'){
   if(value.$number==='NaN')return NaN;
   if(value.$number==='Infinity')return Infinity;
   if(value.$number==='-Infinity')return -Infinity;
   if(value.$number==='-0')return -0;
  }
  return Object.fromEntries(Object.entries(value).map(([key,item])=>[key,hydrate(item)]));
 }
 return value;
}
function bits(value){return numberBits(value);}
function collect(value,path='',result={}){
 if(typeof value==='number'){result[path]=bits(value);return result;}
 if(Array.isArray(value))value.forEach((item,index)=>collect(item,`${path}[${index}]`,result));
 else if(value&&typeof value==='object')Object.entries(value).forEach(([key,item])=>collect(item,path?`${path}.${key}`:key,result));
 return result;
}
const cases=[
 ['market',order({type:'market',price:{$number:'-0'}})],
 ['status-filled',order({state:'filled'})],
 ['status-cancelled',order({state:'cancelled'})],
 ['status-rejected',order({state:'rejected'})],
 ['status-untriggered',order({state:'untriggered'})],
 ['status-triggered',order({state:'triggered'})],
 ['sell-side',order({direction:'sell'})],
 ['fill-or-kill',order({timeInForce:'fill_or_kill'})],
 ['immediate-or-cancel',order({timeInForce:'immediate_or_cancel'})],
 ['post-only-precedence',order({postOnly:true})],
 ['optional-gateway-fields',order({averagePrice:3.5,gatewayContracts:9})],
 ['stop-market',order({type:'stop_market',trigger:'index_price',triggerPrice:4})],
 ['take-market',order({type:'take_market',trigger:'mark_price',triggerPrice:4})],
 ['trailing-stop',order({type:'trailing_stop',trigger:'last_price'})],
 ['market-price',order({price:'market_price'})],
 ['missing-amount',order({gatewayAmount:undefined})],
 ['missing-filled',order({gatewayFilledAmount:undefined})],
 ['filled-nonzero',order({gatewayFilledAmount:1})],
 ['missing-reduce-only',order({reduceOnly:undefined})],
 ['stop-limit',order({type:'stop_limit',trigger:'mark_price'})],
 ['take-limit',order({type:'take_limit',trigger:'mark_price'})],
 ['good-til-day',order({timeInForce:'good_til_day'})],
 ['missing-trigger',order({type:'stop_market'})],
 ['blocker-order',order({type:'stop_limit',gatewayAmount:undefined,gatewayFilledAmount:1,reduceOnly:undefined,timeInForce:'good_til_day'})],
 ['extra-own-property',order({runtimeOnly:{retained:true}})],
 ['extra-blocked-property',order({type:'stop_limit',runtimeOnly:{retained:true}})],
 ['blocked-key-order',reverseKeys(order({type:'stop_limit'}))],
 ['numeric-bits',order({type:'take_market',trigger:'mark_price',price:{$number:'-0'},averagePrice:{$number:'-0'},creationTimestampMs:{$number:'-0'},lastUpdateTimestampMs:{$number:'NaN'},gatewayAmount:{$number:'-0'},gatewayFilledAmount:{$number:'-0'},gatewayContracts:{$number:'-0'},triggerPrice:{$number:'-0'}})],
 ['numeric-stop-loss-bits',order({type:'stop_market',trigger:'mark_price',triggerPrice:{$number:'-0'}})],
 ['info-identity',order()],
];
function capture(fn){try{return {ok:fn()};}catch(error){return {error:{name:error.name,message:error.message}};}}
process.stdout.write(JSON.stringify(cases.map(([name,input])=>{
 const gateway=hydrate(input);
 const result=capture(()=>privateApi.normalizeCoinbaseStarbaseOrder(gateway));
 let identityBehavior;
 if(name==='info-identity'&&result.ok?.status==='ready'){
  identityBehavior={sameReference:result.ok.value.info===gateway,orderPriceBefore:result.ok.value.price,infoPriceBefore:result.ok.value.info.price};
  gateway.price=11;
  identityBehavior.orderPriceAfter=result.ok.value.price;
  identityBehavior.infoPriceAfter=result.ok.value.info.price;
 }
 const ready=result.ok?.status==='ready',blocked=result.ok?.status==='blocked';
 return {...captureHeader(),name,input,result,identityBehavior,floatBits:collect(result),infoKeys:ready?Object.keys(result.ok.value.info):[],gatewayKeys:blocked?Object.keys(result.ok.gateway):[]};
})));
