'use strict';
const future={symbol:'pf_x',precision:{amount:0.25,price:0.125},limits:{leverage:{min:2.75,max:31.5}}};
const spot={...future,info:{pairName:'XXBTZUSD'}};
const stores=spot=>({marketsBySymbol:{pf_x:spot},tickersBySymbol:{pf_x:{last:29.125}},positions:[{symbol:'pf_x',leverage:17.25}]});
const opts={symbol:'pf_x',type:'limit',side:'buy',amount:17.37,price:19.43,timeInForce:'PostOnly',reduceOnly:true};
const order={...opts,id:'ORDER_A',amount:17.25,price:19.5,info:{descr:{ordertype:'limit'},triggerSignal:'spot'}};
const clone=x=>structuredClone(x);
const trading={};
function family(name,input,data,market,extras){trading[name]=[{input,data,store:stores(market)},...extras.map(x=>({input:clone(input),data:clone(data),store:stores(market),...x})),{input,data,store:stores(market),fail:true},{input,data,store:stores(market),syncFail:true},{input,data:null,store:stores(market)},{input,data:{},store:stores(market)}];}
const placeExtras=[
 ...['market','stop_market','take_profit_market','trailing_stop_market','conditional'].map(type=>({input:{...opts,type,slTriggerType:'mark_price',tpTriggerType:'index_price'}})),
 ...['ImmediateOrCancel','FillOrKill','GoodTillCancel'].map(timeInForce=>({input:{...opts,timeInForce}})),
 {input:{...opts,price:0,reduceOnly:false}},{input:{...opts,price:-0}},
 {input:{...opts,type:'trailing_stop_market',price:29.125}},
 {input:{...opts,type:'trailing_stop_market',price:'19.5'}},
 {input:{...opts,type:'trailing_stop_market',price:Infinity}},
 {input:{...opts,type:'trailing_stop_market'},store:{...stores(future),tickersBySymbol:{}}},
 {input:{...opts,stopLoss:5.5,takeProfit:7.25},placeOrdersResult:['EXTRA_A','EXTRA_B']},
 {input:{...opts,stopLoss:5.5},placeOrdersFail:true},
 {store:{marketsBySymbol:{}}}, {resolveFail:true}, {adjustFail:true},
 {fail:true,errorResponse:{data:{error:'permission DENIED'}}},
];
family('futuresPlace',opts,{result:'success',sendStatus:{status:'placed',order_id:'NEW_A'}},future,[...placeExtras,
 {data:{result:'success',sendStatus:{status:'filled',order_id:17.25}}},{data:{result:'success'}},
 ...['invalidSize','insufficientAvailableFunds','authentication','quantity','price','margin','ordinary'].map(error=>({data:{result:'error',error}})),
 {data:{result:'success',sendStatus:{status:'invalidSize'}}},
 {fail:true,errorResponse:{data:{result:'error'}}},
 {fail:true,errorResponse:{data:{error:17}}},
]);
family('spotPlace',opts,{result:{txid:['SPOT_A','SPOT_B']}},spot,[...placeExtras,
 {data:{result:{txid:'XY'}}},{data:{result:{txid:{}}}}, {clientId:'INVALID'},
 {store:{...stores(spot),marketsBySymbol:{pf_x:{...spot,info:{}}}}},
]);
family('futuresUpdate',{order,update:{amount:23.43,price:29.43}},{result:'success',editStatus:{status:'edited',orderId:'EDIT_A'}},future,[
 {input:{order:{...order,type:'trailing_stop_market'},update:{price:21}}},
 ...['stop_market','take_profit_market'].map(type=>({input:{order:{...order,type},update:{amount:null,price:0}},placeOrderResult:['REPLACE_A']})),
 {input:{order:{...order,type:'stop_market',slTriggerType:'last_price'},update:{}}},
 {input:{order:{...order,type:'stop_market',info:{triggerSignal:'unknown'}},update:{}}},
 {input:{order:{...order,type:'stop_market',info:{triggerSignal:'constructor'}},update:{}}},
 {input:{order:{...order,type:'stop_market'},update:{}},cancelOrdersFail:true},
 {input:{order:{...order,type:'take_profit_market'},update:{}},placeOrderFail:true},
 {input:{order:{...order,type:'stop_market'},update:{}},placeOrderFail:true,errorMessage:''},
 {input:{order:{...order,type:'stop_market'},update:{}},afterOrder:{amount:71.25,price:73.5},placeOrderResult:['AFTER']},
 {input:{order,update:{}}},{store:{marketsBySymbol:{}}},
 {data:{result:'success',editStatus:{status:'filled',order_id:'ALIAS'}}},
 {data:{result:'success',editStatus:{status:'rejected'}}},
 {data:{result:'error',error:'UPDATE_BAD'}}, {fail:true,errorResponse:{data:{error:'RESPONSE_BAD'}}},
]);
family('spotUpdate',{order,update:{amount:23.43,price:29.43}},{result:{amend_id:'AMEND_A'}},spot,[
 ...['stop_market','take_profit_market'].map((type,i)=>({input:{order:{...order,type,info:{order_type:i?'take-profit':'stop-loss'}},update:{price:29.43}}})),
 {input:{order:{...order,timeInForce:'ImmediateOrCancel'},update:{amount:5}}},
 {input:{order:{...order,info:{descr:{ordertype:'stop-loss-limit'}}},update:{price:5}}},
 {input:{order:{...order,type:'market'},update:{price:5}}},
 {input:{order:{...order,info:{}},update:{price:5}}},
 {input:{order,update:{}}},
 ...[0,-1,NaN,Infinity,null].map(amount=>({input:{order,update:{amount}}})),
 ...[0,-1,NaN,Infinity].map(price=>({input:{order,update:{price}}})),
 {store:{marketsBySymbol:{}}},{data:{result:{amend_id:17}}},{data:{result:{amend_id:''}}},
]);
family('futuresCancel',[order],{result:'success',cancelStatus:{status:'cancelled'}},future,[
 {input:[],data:{result:'success',batchStatus:[]}},
 {input:[order,{...order,id:'ORDER_B'}],data:{result:'success',batchStatus:[{status:'cancelled'},null,{}]}},
 {input:[order,{...order,id:'ORDER_B'}],data:{result:'success',batchStatus:[{status:'cancelled'},{status:'rejected'}]}},
 {input:[],data:{result:'error',error:'BATCH_BAD'}},
 {data:{result:'success',cancelStatus:{status:'rejected'}}},{data:{result:'error',error:'CANCEL_BAD'}},
 {fail:true,errorResponse:{data:{error:'RESPONSE_BAD'}}},
]);
family('leverage',{symbol:'pf_x',leverage:19.125},{result:'success'},future,[
 {input:{symbol:17,leverage:19.125},store:{marketsBySymbol:{17:future}}},
 ...[0,100,-0,NaN,Infinity,-Infinity].map(leverage=>({input:{symbol:'pf_x',leverage}})),
 {store:{marketsBySymbol:{}}},{data:{result:'error',errors:[{message:'PREFERENCE_BAD'}]}},{data:{status:'BAD_REQUEST'}},
 {fail:true,errorResponse:{data:{errors:[{message:'HTTP_ERROR'}],error:'secondary'}}},
 {fail:true,errorResponse:{data:{error:'HTTP_SECONDARY'}}},{logFail:true},
]);
family('margin',{symbol:'pf_x',isolated:true},{result:'success'},future,[
 ...[true,false].map(isolated=>({input:{symbol:17,isolated},store:{marketsBySymbol:{17:future},positions:[]}})),
 {input:{symbol:'pf_x',isolated:false},leverage:{pf_x:19.5},margin:{pf_x:'isolated'}},
 {store:{...stores(future),positions:[]}}, {store:{...stores(future),positions:[{symbol:'pf_x',leverage:0}]}},
 {input:{isolated:true}},{store:{marketsBySymbol:{}}},
 {data:{result:'error',errors:[{message:'MARGIN_BAD'}]}},{data:{status:'BAD_REQUEST'}},
 {input:{symbol:'pf_x',isolated:false},data:{result:'error'}},
 {fetchPositionsFail:true},{fetchPositionsResult:[{id:'POSITION_A'}]}, {addFail:true}, {logFail:true},
 {fail:true,errorResponse:{data:{errors:[{message:'HTTP_ERROR'}]}}},
]);
const options={testnet:false,key:'KEY',secret:'SECRET_INPUT',publicDataOnly:false};
const exchange={name:'kraken'};
const get={url:'/v3/openorders?old=1',method:'GET',params:{z:'space value',a:[1,2]},headers:{old:'header'}};
const post={url:'/v3/sendorder',method:'post',data:{size:17.25,symbol:'PF_X'},headers:{}};
const api={};
const req=input=>({kind:'request',input});const success=input=>({kind:'success',input});const rejected=()=>({kind:'rejected',input:{}});
const sharedCases=[
 {steps:[req(get),req(post),success({data:{result:'success'}})]},
 {steps:[req({url:'/0/private/Balance',method:'post',data:{nonce:'user nonce',volume:17.25}})]},
 {steps:[req({url:'/v3/batchorder',method:'patch',data:{batchOrder:[{order:'cancel',order_id:'A'}]}})]},
 {steps:[req({url:'/v3/openorders',method:'delete',params:{}}),req({url:'/v3/openorders',method:'other',data:{}})]},
 {steps:[req({url:'/v3/sendorder',method:'put',data:'primitive'})]},
 {steps:[req({url:'/v3/openorders',method:17})]},
 {steps:[req({url:17})]},
 {options:{...options,publicDataOnly:true},steps:[req(get)]},
 {options:{...options,key:''},steps:[req(get)]},
 {options:{...options,remoteSigner:{}},steps:[req(get)]},
 {options:{...options,testnet:true},baseKind:'history',steps:[req(get)]},
 {clients:2,steps:[{...req(get),client:0},{...req(get),client:1},{...req(get),client:0}]},
 {counter:999,clock:1723638896789.125,steps:[req(get),req(post)]},
 {qsFail:true,steps:[req(post)]},{hashFail:true,steps:[req(post)]},
 {steps:[req(get),{...req(post),options:{key:'AFTER_KEY',remoteSigner:{}}}]},
 {createFail:true,steps:[]},{retryFail:true,steps:[]},
 ...[401,403,429,500,'401'].map(status=>({errorResponse:{status,data:{error:['HTTP_A','HTTP_B']}},steps:[rejected()]})),
 {steps:[rejected()]},
];
api.futuresApi=sharedCases.map(x=>({options,exchange,clients:1,...clone(x),steps:clone(x.steps)}));
api.futuresApi.push(
 {options,exchange,clients:1,steps:[req({url:'/v3/tickers'}),success({data:null})]},
 ...['authentication','APIKEY','nonce','permission','signature','other'].map(error=>({options,exchange,clients:1,steps:[success({data:{result:'error',error}})]})),
 {options,exchange,clients:1,steps:[success({data:{error:'unlabelled'}})]},
 {options,exchange,clients:1,steps:[success({data:{result:'error',error:17}})]},
);
api.spotApi=sharedCases.map(x=>({options,exchange,clients:1,...clone(x),steps:clone(x.steps)}));
api.spotApi.push(
 {options,exchange,clients:1,steps:[req({url:'/0/public/Ticker'}),success({data:{error:[]}})]},
 ...['EAPI:Invalid key','EAPI:Invalid nonce','EAPI:Invalid signature','EGeneral:Permission denied','EOrder:Rate limit exceeded','EOrder:Insufficient funds','other'].map(error=>({options,exchange,clients:1,steps:[success({data:{error:[error,'secondary']}})]})),
 {options,exchange,clients:1,steps:[success({data:{error:'ignored'}})]},
 {options,exchange,clients:1,errorResponse:{status:500,data:{error:'not-array'}},steps:[rejected()]},
 {options,exchange,clients:1,steps:[success({data:{error:['EAPI:Invalid signature']},config:{url:'/0/private/Balance',method:'post',data:'nonce=1&volume=2'}})]},
 {options,exchange,clients:1,steps:[success({data:{error:['EAPI:Invalid signature']},config:{url:'/0/private/Balance',safeCexSignatureRetry:true}})]},
 {options,exchange,clients:1,steps:[success({data:{error:['EGeneral:Invalid arguments','EAPI:Invalid signature in body']}})]},
 {options,exchange,clients:1,parsed:{volume:'3',nonce:'old',pair:'XBTUSD'},steps:[req({url:'/0/private/AddOrder',method:'post',data:'volume=3&nonce=old&pair=XBTUSD'})]},
);
module.exports={trading,api};
