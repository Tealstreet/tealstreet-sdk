'use strict';
const ticker={symbol:'BTCUSDC',last:5.5},mapped={id:'MAPPED',side:'sell',symbol:'BTCUSDC',opaque:{retained:true}};
const tick={symbol:'PERP_BTC_USDC',close:17.25,amount:31.5,volume:23.125,bid:5.75,ask:91.25,price:73.5,fundingRate:' -1.25e-3junk',fundingTs:'1723638896.125tail'};
const defaults={mapped,tickers:{BTCUSDC:ticker},data:[tick],parsed:null};
const cases={};
for(const name of ['handleTickersStreamEvents','handleBBOStreamEvents','handleMarkPricesStreamEvents','handleFundingRateStreamEvents'])cases[name]=[
 {},{disposed:true},{data:[{...tick,symbol:'SPOT_BTC_USDC'},{...tick,symbol:'PERP_OTHER_USDC'},tick]},
 {data:[{...tick,symbol:'PERP_BTC_USDC_extra'},tick],fail:'updateTicker'},
 {data:[{...tick,close:{original:'close'},amount:-0,volume:NaN,bid:-0,ask:NaN,price:-0,funding_rate:'91',funding_ts:'0',fundingRate:undefined,fundingTs:undefined}]},
 {data:[tick,tick],append:true},
 {data:[{...tick,fundingRate:undefined,fundingTs:undefined,funding_rate:' -0',funding_ts:'Infinity'}],removeCamel:true},
 {data:[{...tick,fundingRate:'junk',fundingTs:null}]},
];
const execution={status:'FILLED',orderId:1723638896.125,executedPrice:'23.75',executedQuantity:'5.5'};
cases.handlePositionReport=[{data:{positions:[{id:'p1'},{id:'p2'}]}},{data:{positions:[]}},{data:null},{data:{positions:{id:'bad'}}},{data:{positions:[{id:'p1'}]},mapped:null},{data:{positions:[{id:'p1'}]},fail:'mapPosition'},{data:{positions:[{id:'p1'}]},fail:'addPosition'},{data:{positions:[{id:'p1'}]},append:true}];
cases.handleExecutionReport=['NEW','REPLACED','PARTIAL_FILLED','PARTIALLY_FILLED','FILLED','CANCELLED','OTHER'].map(status=>({data:{...execution,status}}));
cases.handleExecutionReport.push({data:execution,mapped:null},{data:{...execution,status:'NEW'},mapped:false},{data:{status:'FILLED',order_id:'snake',executed_price:91.25,executed_quantity:17.5}},{data:execution,mutateStatus:'NEW'},{data:execution,fail:'mapLimitOrder'},{data:execution,fail:'status'},{data:execution,fail:'fill'},{data:{...execution,status:'PARTIALLY_FILLED'},fail:'addOrder'});
const algo={algoStatus:'FILLED',orderId:0,algoOrderId:19.25,id:'fallback',side:'BUY',symbol:'PERP_BTC_USDC',averageExecutedPrice:37.25,totalExecutedQuantity:2.75,childOrders:[{algoOrderId:'child'}]};
cases.handleAlgoExecutionReport=['NEW','REPLACED','PARTIAL_FILLED','PARTIALLY_FILLED','FILLED','CANCELLED','OTHER'].map(algoStatus=>({data:{...algo,algoStatus}}));
cases.handleAlgoExecutionReport.push({data:[algo,{...algo,orderId:'ORDER',childOrders:[]}]},{data:{...algo,orderId:null,algoOrderId:0,id:0,side:'UNKNOWN'}},{data:{algo_status:'FILLED',algo_order_id:'snake',side:'SELL',symbol:'PERP_BTC_USDC',averageExecutedPrice:5.5,totalExecutedQuantity:13.25,childOrders:[{algo_order_id:0}]}},{data:{...algo,algoStatus:'NEW'},mappedOrders:[]},{data:{...algo,algoStatus:'NEW'},fail:'mapAlgoOrder'},{data:algo,fail:'fill'},{data:algo,fail:'status'},{data:[{...algo,algoStatus:'NEW'}],append:true});
cases.onMessage=[{parsed:{event:'ping'}},{parsed:{event:'pong'}},{parsed:{event:'auth',success:true}},{parsed:{event:'auth',success:0}},{parsed:{topic:'executionreport',data:execution}},{parsed:{topic:'algoexecutionreportv2',data:algo}},{parsed:{topic:'position',data:{positions:[{id:'p1'}]}}},{parsed:null},{parsed:false},{parsed:{topic:'unknown'}},{disposed:true,parsed:{event:'ping'}},{parsed:{event:'ping'},fail:'diagnostic'},{parsed:{event:'ping'},fail:'parse'},{parsed:{event:'ping'},fail:'send'},{parsed:{event:'pong'},fail:'pong'},{parsed:{topic:'executionreport',data:execution},integrate:true}];
module.exports={defaults,cases};
