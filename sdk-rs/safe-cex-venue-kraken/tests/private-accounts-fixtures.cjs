'use strict';
const clone=x=>structuredClone(x);
const store={orders:[{id:'A',amount:17.25,filled:2.5,status:'open',symbol:'X'}],markets:[{symbol:'X'}],marketsBySymbol:{X:{limits:{leverage:{max:31.5}}}}};
const fill={order_id:'A',qty:'5.5tail',price:'19.25tail',instrument:'X',buy:true};
const order={order_id:'A',type:'limit',instrument:'X',direction:0,qty:'17.25',filled:'5.5',limit_price:'19.25',cli_ord_id:'CID',last_update_time:'opaque-time'};
const position={instrument:'X',balance:'-17.25',entry_price:'19.25',mark_price:'23.75',pnl:'-5.5',liquidation_threshold:'29.125',maxFixedLeverage:'7.25'};
const flex={type:'multiCollateralMarginAccount',portfolioValue:'101.25',balanceValue:'99',availableMargin:'-5.5',pnl:'7.25',initialMargin:'11.5',maintenanceMargin:'13.75',currencies:{USD:{quantity:'29.5',value:'31.25',available:'17.75',collateral:'19.5'},BTC:{quantity:'2',value:'37',available:'1',collateral:'5'}}};
const spot={order_id:'A',symbol:'X',order_type:'stop-loss',side:'buy',order_qty:'17.25',cum_qty:'5.5',limit_price:'19.25',exec_type:'new',timestamp:'2026-08-14T12:34:56.125Z'};
const rows={};
function family(name,input,extras){rows[name]=[{input,store:clone(store)},...extras.map(x=>({input:clone(input),store:clone(store),...x}))];}
family('fills',{fills:[fill,fill]},[
 {input:{executions:[{...fill,order_id:undefined,orderId:'A',qty:0,quantity:'3.25',buy:undefined,side:'BUY'}]}},
 {input:[{...fill,buy:null},{...fill,order_id:29,instrument:undefined,symbol:false}]},
 {input:{fills:[]}}, {input:{fills:false,executions:[fill]}},{input:{fills:{},executions:[fill]}},
 {input:null},{input:false},{input:{fills:[{uid:'A',size:'bad',fillPrice:'bad',direction:'sell'}]}},
 {store:{...clone(store),orders:[{id:'A',amount:17.25,filled:'2.5'}]}},
 {input:{fills:[{...fill,qty:100}]}},{input:{fills:[null]}},
 {effectFail:'fill'},{effectFail:'updateOrder'},{effectFail:'status' ,input:{fills:[{...fill,qty:100}]}},{effectFail:'throttle'},
 {afterEffect:{orders:[{id:'A',amount:29.5,filled:11.25}]}},
 {input:{fills:[{...fill,qty:0}]},store:{...clone(store),orders:[{id:'A',amount:-0,filled:0}]}},
]);
family('orders',{orders:[order]},[
 ...['post','ioc','fok','stop','stp','take_profit','trailing_stop','lmt','unlimited','market'].map(type=>({input:{order:{...order,type,stop_price:'23.75'}}})),
 {input:{is_cancel:true,order_id:17}}, {input:{is_cancel:true,orders:[order,order]}},
 {input:{order:{...order,time:5,last_update_time:0,reduce_only:true,direction:'0'}}},
 {input:{feed:'open_orders_snapshot',order:order}}, {input:{orders:[],order}},
 {input:{order:{uid:'B',symbol:17,type:false,qty:'bad',filled:'Infinity',reduceOnly:1,time:[]}}},
 {input:null},{input:{orders:{}}},{input:{orders:[null]}},
 {effectFail:'fullOrders'},{input:{order},effectFail:'addOrder'},{input:{is_cancel:1,order_id:'A'},effectFail:'status'},
]);
family('positions',{positions:[position]},[
 {input:{openPositions:[{...position,maxFixedLeverage:undefined,balance:'5.5'}]},leverage:{X:13.25},margin:{X:'cross'}},
 {input:{positions:[{...position,maxFixedLeverage:null}]},margin:{X:'isolated'}},
 {input:{positions:[{...position,balance:0},{...position,balance:'bad'}]}},
 {input:{positions:[{...position,maxFixedLeverage:undefined}]},margin:{X:''},leverage:{X:null}},
 {input:{positions:[{...position,maxFixedLeverage:undefined}]},store:{orders:[],markets:[],marketsBySymbol:{}}},
 {input:{positions:[]}}, {input:{positions:{}}},{input:null},{input:{positions:[null]}},
 {missingMargin:true,missingLeverage:true,input:{positions:[{...position,maxFixedLeverage:undefined}]}},
 {input:{positions:[{...position,symbol:'X',instrument:undefined,maxFixedLeverage:'bad',mark_price:'bad'}]}},
 {effectFail:'fullPositions'},{input:{positions:[{...position,maxFixedLeverage:undefined}]},store:{...clone(store),marketsBySymbol:{X:{}}}},
]);
family('balances',{accounts:{cash:{balances:{usd:'17.25',USDT:'19.5',btc:'2.75',skip:0}},flex}},[
 {input:{balances:{cashAccount:{balances:{usd:'17.25',btc:'2',zero:0,bad:'bad'}}}}},
 {input:{accounts:{multiCollateral:{...flex,portfolioValue:0,balanceValue:'37.5'}}}},
 {input:{accounts:{flex:{...flex,portfolioValue:0,balanceValue:0}}}},
 {input:{a:{auxiliary:{pv:'17.25',af:'-5.5',pnl:'3'}},b:{auxiliary:{pv:'11.75',af:'7',pnl:'-1'}},skip:null}},
 {input:{a:{auxiliary:{pv:'17.25',af:'5.5',currency:'usd'}},b:{balances:{usd:'21.5',BTC:'3.25'}}}},
 {input:{a:{auxiliary:{pv:'0',af:'bad'}},b:{auxiliary:{pv:'11',currency:'EUR'}}}},
 {input:{accounts:{flex:{...flex,currencies:{a:null,b:17,c:{quantity:0,value:'5',available:'bad',collateral:'bad'}}}}}},
 {input:null},{input:false},{input:'text'},{input:[]},{input:{}},
 {effectFail:'update'}, {input:{accounts:{cash:[],flex:false,other:{auxiliary:{pv:'7'}}}}},
]);
family('parse',spot,[
 ...['canceled','expired','filled','trade','new'].map(exec_type=>({input:{...spot,exec_type}})),
 {input:{...spot,post_only:true,time_in_force:'IOC',limit_price:0,stop_price:'7.25'}},
 {input:{...spot,post_only:1,time_in_force:'IOC',timestamp:1723638896125.75}},
 {input:{...spot,order_id:17,order_qty:'5',cum_qty:'7',timestamp:0,order_type:'unknown',side:'BUY'}},
 {input:{...spot,timestamp:'bad',order_qty:'bad',cum_qty:0}},
 {input:{...spot,symbol:'missing'}},{input:{...spot,order_id:0}},{input:null},
 {store:{...clone(store),markets:[null]}},
 {input:{...spot,order_id:{key:17},symbol:17},store:{...clone(store),markets:[{symbol:17}]}},
]);
family('executions',{data:[{...spot,exec_type:'trade',last_qty:'2.75',avg_price:'23.5'}, {...spot,exec_type:'expired'}]},[
 {input:{type:'snapshot',data:[spot,{...spot,symbol:'missing'}]}},
 {input:{data:[{...spot,exec_type:'filled',cum_qty:'17.25',order_qty:0}]}},
 {input:{data:[{...spot,exec_type:'trade',order_id:'unknown'},{...spot,exec_type:'canceled',order_id:0}]}},
 {input:{data:[spot]}},{input:{data:[]}},{input:{data:{}}},{input:null},{input:{data:[null]}},
 {store:{...clone(store),orders:[{id:'A',amount:null}]},input:{data:[{...spot,exec_type:'trade',order_qty:0}]}},
 {input:{data:[spot,spot]},overrideParse:true},
 {effectFail:'fill'},{effectFail:'addOrder'},{effectFail:'throttle'},
 {afterEffect:{orders:[]}},
]);
const records=[{incomeType:'pnl',symbol:'X',income:17.25},{incomeType:'fee',symbol:'Y',income:-5.5},{incomeType:'pnl',symbol:'X',income:7.25}];
family('range',{start:1723034096789,end:1723638896789,type:'pnl'},[
 {input:{start:0,end:0,type:''}}, {input:{start:-1,end:1723638896789,type:17}},
 {truncated:true,exhaustedChunks:3}, {public:true}, {pagerFail:true}, {pageFail:true}, {pagerSyncFail:true},
 {truncated:true,input:{start:'bad',end:1723638896789,type:'pnl'}}, {truncated:true,effectFail:'warning'},
 {input:{start:1723034096789,end:1723638896789}}, {records:[]}, {pageCalls:2},
 {truncated:true,input:{start:"2026-08-07T12:34:56.789Z",end:1723638896789,type:"pnl"}},
]);
family('history',{startTime:1723034096789,endTime:1723638896789,incomeType:'pnl',symbol:'X',limit:1.5},[
 {input:null},{input:{}},{input:{endTime:0,startTime:0,limit:0}}, {public:true},
 {input:{limit:-1,symbol:'missing'}},{input:{limit:'2',symbol:'X'}},{input:{limit:'bad'}},
 {input:{incomeType:17}}, {rangeFail:true},{rangeSyncFail:true},{rangeFail:true,errorMessage:''},
 {rangeFail:true,effectFail:'incomeError'}, {records:[]}, {afterOptions:{symbol:'Y',limit:0}},
]);
for (const family of ['range','history'])for(const row of rows[family]) row.records=row.records??clone(records);
module.exports=rows;
