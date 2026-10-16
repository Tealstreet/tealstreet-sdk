const {write}=require('./load-starbase.cjs');
const {captureHeader,numberBits}=require('./special-numbers.cjs');
const intents=[];
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
function floatBits(value){
 return numberBits(value);
}
function collectFloatBits(value,path='',result={}){
 if(typeof value==='number'){result[path]=floatBits(value);return result;}
 if(Array.isArray(value))value.forEach((item,index)=>collectFloatBits(item,`${path}[${index}]`,result));
 else if(value&&typeof value==='object')Object.entries(value).forEach(([key,item])=>collectFloatBits(item,path?`${path}.${key}`:key,result));
 return result;
}
function intent(type,extra={}){return {savedSymbol:'BTC-PERP-INTX',side:'buy',size:{totalBaseAmount:2,source:'coinbase-technical-migration-guide',unit:'base'},type,...extra};}
for(const [type,extra] of [
 ['limit',{price:10}],['stop_limit',{price:10,trigger:'mark_price',triggerPrice:11}],
 ['take_limit',{price:10,trigger:'last_price',triggerPrice:9}],['market',{}],
 ['stop_market',{trigger:'index_price',triggerPrice:9}],['take_market',{trigger:'mark_price',triggerPrice:11}],
 ['market_limit',{}],['trailing_stop',{trigger:'last_price',triggerOffset:2}],
])intents.push({name:'buildCoinbaseStarbasePlaceOrder',input:intent(type,extra)});
intents.push(
 {name:'buildCoinbaseStarbaseBuy',input:{...intent('limit',{price:10}),side:undefined}},
 {name:'buildCoinbaseStarbaseSell',input:{...intent('market',{})}},
 {name:'buildCoinbaseStarbaseBuy',input:{...intent('market_limit',{}),side:undefined}},
 {name:'buildCoinbaseStarbaseSell',input:{...intent('market_limit',{})}},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('limit')},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('market',{trigger:'mark_price'})},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('limit',{price:0})},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('limit',{price:10,timeInForce:'PostOnly',postOnly:false})},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('limit',{price:10,rejectPostOnly:true})},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('limit',{price:10,size:{totalContracts:2,source:'gateway-contract-count',unit:'contracts'}})},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('limit',{price:10,size:{totalContracts:2,source:'gateway-contract-count',unit:'contracts'},unmodeledRuntimeField:{retained:true}})},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('limit',{price:{$number:'-0'},size:{totalContracts:{$number:'NaN'},source:'gateway-contract-count',unit:'contracts'}})},
 {name:'buildCoinbaseStarbasePlaceOrder',input:intent('limit',{price:10,validUntil:{gatewayIntegerTimestamp:123,unit:'upstream-unspecified'}})},
);
const leg=(type,extra={})=>({direction:'sell',size:{totalBaseAmount:1,source:'coinbase-technical-migration-guide',unit:'base'},reduceOnly:true,trigger:'mark_price',type,...extra});
for(const type of ['take_limit','take_market','stop_limit','stop_market']){
 const price=type.endsWith('_limit')?{price:5}:{};
 const triggerPrice=type==='take_limit'?{}:{triggerPrice:6};
 const value={legs:[leg(type,{...price,...triggerPrice}),leg(type.startsWith('take_')?'stop_market':'take_market',{triggerPrice:4})],linkedOrderType:'one_triggers_one_cancels_other'};
 intents.push({name:'buildOtoco',input:value});
}
intents.push({name:'buildOtoco',input:{legs:[leg('take_limit',{price:5}),leg('take_market',{triggerPrice:4})],linkedOrderType:'one_triggers_one_cancels_other',triggerFillCondition:'first_hit'}});
intents.push({name:'buildOtoco',input:{legs:[leg('take_limit',{price:5}),leg('take_limit',{price:4})],linkedOrderType:'one_triggers_one_cancels_other'}});
intents.push({name:'buildOtocoLeg',input:leg('take_limit',{price:5})});
intents.push({name:'buildOtocoLeg',input:leg('stop_market',{triggerPrice:3,timeInForce:'PostOnly'})});
intents.push({name:'buildOtocoLeg',input:leg('take_market',{triggerPrice:3,price:5})});
intents.push({name:'validateOtocoLegShape',input:leg('take_limit',{price:5})});
intents.push({name:'validateOtocoLegShape',input:leg('take_market',{triggerPrice:5})});
intents.push({name:'validateOrderShape',input:{type:'limit',price:1}});
intents.push({name:'validateOrderShape',input:{type:'market'}});
intents.push({name:'validateOrderShape',input:{type:'trailing_stop',trigger:'mark_price',triggerOffset:2}});
intents.push({name:'validateOrderShape',input:{type:'stop_limit',price:1,trigger:'mark_price',triggerPrice:2}});
function capture(fn){try{const result=fn();return result===undefined?{undefined:true}:{ok:result};}catch(error){return {error:{name:error.name,message:error.message}};}}
process.stdout.write(JSON.stringify(intents.map(test=>{
 const input=hydrate(test.input),result=capture(()=>write[test.name](input));
 return {...test,...captureHeader(),result,floatBits:collectFloatBits(result)};
})));
