'use strict';
const fm={symbol:'PF_X',base:'BTC',quote:'USD',tradeable:true,status:'online',contractValueTradePrecision:'4tail',tickSize:0.125,contractSize:2.75,marginLevels:[{initialMargin:'0.04tail'}],contractType:'inverse',lastTradingTime:17};
const sm={wsname:'XBT/USD',status:'online',pair_decimals:2,lot_decimals:4,ordermin:'0.125tail',altname:'XBTUSD'};
const m={id:'BTC/USD',symbol:'XBT/USD',prettySymbol:'BTCUSD',active:true,isSpot:true,info:{pairName:'XXBTZUSD',altname:'XBTUSD'},limits:{leverage:{max:31.5}}};
const future={...m,id:'BTC/USD',symbol:'PF_X',isSpot:false};
const t={a:['19.5'],b:['17.25'],c:['23.75'],v:['91','29.125'],o:'5.5'};
const o={descr:{pair:'XBTUSD',ordertype:'stop-loss',type:'buy',price:'17.25tail'},vol:'29.125',vol_exec:'5.5',oflags:'post,fciq',opentm:1723638896.125};
const p={symbol:'PF_X',side:'LONG',size:'-17.25tail',price:'19.5',unrealizedPnl:'-5.5',maxFixedLeverage:'7.25'};
const accounts={cash:{balances:{usd:'17.25',USDT:'19.5',btc:'2.75',skip:0}},flex:{type:'multiCollateralMarginAccount',portfolioValue:'101.25',balanceValue:'99',availableMargin:'-5.5',pnl:'7.25',initialMargin:'11.5',maintenanceMargin:'13.75',currencies:{USD:{quantity:'29.5',value:'31.25',available:'17.75',collateral:'19.5'},BTC:{quantity:'2',value:'37',available:'1',collateral:'5'}}},fi_x:{auxiliary:{pv:'41.25',af:'43.5',pnl:'-47.75',currency:'USD'}}};
const fill=(id,time)=>({fill_id:id,symbol:'pf_x',fillTime:`2026-08-14T12:${time}:00Z`,side:'buy',price:'17.25',size:'19.5',fillType:'maker',realized_pnl:'-23.75',order_id:'order-A'});
const clone=x=>structuredClone(x);
const rows={};
function family(name,data,store,extras=[]){rows[name]=[{data,store},...extras.map(x=>({data:clone(data),store:clone(store),...x})),{data:null,store},{data:false,store},{data:{},store},{data,store,fail:true},{data,store,syncFail:true},{data,store,fail:true,after:{...store,markets:[{symbol:'after'}],tickers:[{symbol:'after'}],orders:[{id:'after'}],positions:[{id:'after'}],balance:{total:79.5}}}];}
family('futuresMarkets',{instruments:[fm,{...fm,symbol:'PI_NO'},{...fm,tradeable:false}]},{markets:[{symbol:'fallback'}]},[
 {data:{instruments:[{...fm,symbol:undefined,symbolName:'PF_ALIAS',contractValueTradePrecision:0,tickSize:0,contractSize:0,marginLevels:[]}] }},
 {data:{instruments:[{...fm,contractValueTradePrecision:null,marginLevels:[{initialMargin:'bad'}]}]}},
 {data:{instruments:[{...fm,contractValueTradePrecision:-2,status:'cancel_only'}]}},
 {data:{instruments:[null]}},{data:{instruments:{}}},{data:{instruments:[{symbol:17}]}},
]);
family('spotMarkets',{result:{XXBTZUSD:sm,DARK:{...sm,wsname:'XBT/USD'},'ignored.d':null,BAD:{wsname:'BTC/EUR'},EMPTY:{}}},{markets:[{symbol:'fallback'}]},[
 {data:{result:{P:{...sm,wsname:'XBT/USDT',pair_decimals:null,lot_decimals:null}}}},
 {data:{result:{P:{...sm,wsname:'ETH/USDC',pair_decimals:0,lot_decimals:0,status:'cancel_only'}}}},
 {data:{result:{P:null}}},{data:{result:{P:{wsname:'XBT'}}}},{data:{result:{P:{wsname:17}}}},
]);
family('spotTickers',{result:{XXBTZUSD:t}},{markets:[m,{...m,symbol:'missing',info:{}}],tickers:[{symbol:'fallback'}]},[
 {data:{result:{XXBTZUSD:{...t,a:null,b:[],c:['bad'],v:[0,'bad'],o:0}}}},
 {data:{result:{XXBTZUSD:{...t,c:['-0'],o:'bad'}}}},
 {after:{markets:[{...m,symbol:'AFTER'}]}}, {data:{result:{XXBTZUSD:null}}},
]);
family('spotOrders',{result:{open:{ORDER_A:o,ORDER_B:{...o,descr:{...o.descr,pair:'MISSING'},vol:'3',vol_exec:'5',opentm:0}}}},{markets:[m],orders:[{id:'fallback'}]},[
 {data:{result:{open:{X:{...o,descr:{...o.descr,pair:'BTCUSD',ordertype:'market'},oflags:undefined}}}}},
 {data:{result:{open:{X:{...o,vol:'bad',vol_exec:'0',oflags:['post'],opentm:'2.75'}}}}},
 {store:{markets:[m,{...m,symbol:'OVERRIDE',prettySymbol:'other'}],orders:[]}},
 {data:{result:{open:{X:null}}}}, {data:{result:{open:{X:{...o,oflags:17}}}}},
]);
family('futuresPositions',{openPositions:[p]},{markets:[future,{...future,symbol:'FAKE'},m],marketsBySymbol:{PF_X:future},tickers:[{symbol:'PF_X',mark:29.125}],positions:[{id:'PF_X:long',notional:71.25,liquidationPrice:73.5}]},[
 {data:{openPositions:[{...p,maxFixedLeverage:undefined}]},margin:{PF_X:'cross'},leverage:{PF_X:13.25}},
 {data:{openPositions:[{...p,maxFixedLeverage:undefined}]},store:{markets:[],marketsBySymbol:{},tickers:[{symbol:'PF_X',mark:2}],positions:[]}},
 {data:{openPositions:[{...p,maxFixedLeverage:null,size:0}]},margin:{PF_X:'isolated'}},
 {data:{openPositions:[{...p,side:'short',size:'bad',maxFixedLeverage:'bad'}]}},
 {data:{openPositions:[null]}},{data:{openPositions:{}}},
]);
family('futuresBalance',{result:'success',accounts},{balance:{total:79.5}},[
 {data:{result:'unauthorized'}},{data:{result:'success',accounts:{cashAccount:{balances:{usd:'17.25',btc:'2',zero:0,bad:'bad'}}}}},
 {data:{result:'success',accounts:{flex:{...accounts.flex,portfolioValue:0,balanceValue:'37.5'}}}},
 {data:{result:'success',accounts:{flex:{...accounts.flex,portfolioValue:0,balanceValue:0}}}},
 {data:{result:'success',accounts:{other:{auxiliary:{portfolioValue:'17.25',availableMargin:'5.5',pnl:'-3'}}}}},
 {data:{result:'success',accounts:{fi_null:null}}},
 {data:{result:'success',accounts:{fi_a:{auxiliary:{pv:'5',af:'7',pnl:'1',currency:'BTC'}},other:{auxiliary:{pv:'9',af:'2'}}}}},
]);
rows.spotRefresh=[{balance:{total:17.25},store:{loaded:{orders:true,balance:false}}},{balance:{total:17.25},store:{loaded:{}},after:{loaded:{tickers:true,balance:false}}},{store:{loaded:{}},fail:true},{balance:{},store:{loaded:{}},updateFail:true},{store:{loaded:null},balance:{}}];
const page=[fill('A','34'),fill('B','33')];
const opts={from:Date.parse('2026-08-14T12:30:00Z'),to:Date.parse('2026-08-14T12:35:00Z')};
const store={marketsBySymbol:{PF_X:{symbol:'PF_CANON'}}};
rows.futuresFills=[
 {pages:[{fills:page},{fills:[fill('B','33'),fill('C','30')]}],opts,store},
 {pages:[{fills:page},{fills:[]}],opts:{...opts,limit:1.5},store},
 {pages:[{fills:[fill('FUTURE','39'),fill('OLD','29'),...page]}],opts,store},
 {pages:[{fills:[{fill_id:'bad'}]}],opts,store},
 {pages:[{fills:page}],opts,store},
 {pages:[{fills:Array.from({length:100},(_,i)=>fill('ID-'+i,'34'))}],opts,store},
 {pages:[{result:'error',error:'denied'}],opts,store},
 {pages:[{}],opts,store}, {pages:[null],opts,store},
 {pages:[{fills:page}],opts,store,fail:true},
 {pages:[{fills:page}],opts,store,failAt:1},
 {pages:[{fills:page}],opts,store,disposed:true},
 {public:true,opts,store},
 {pages:[{fills:[]}],opts:{from:0,to:0,limit:0},store},
 {pages:[{fills:[]}],opts:{},store},
 {pages:[{fills:[{fill_id:'bad'}]}],opts,store,warnFail:true},
 {pages:[{fills:page}],opts,store,fail:true,emitFail:true},
];
module.exports=rows;
