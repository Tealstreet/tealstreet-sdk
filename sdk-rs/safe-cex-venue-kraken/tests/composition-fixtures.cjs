'use strict';
const store={orders:[{id:'A',amount:17.25,filled:2.5,status:'open',symbol:'X'}],markets:[{symbol:'X'}],marketsBySymbol:{X:{limits:{leverage:{max:31.5}}}}};
const base={name:'kraken',options:{key:'K',secret:'S'},store,parent:{},publicSocket:{preserved:'public'},privateSocket:{preserved:'private'}};
const clone=x=>structuredClone(x);
const classes={};
for(const [name,spot]of [['KrakenExchange',false],['KrakenSpotExchange',true],['KrakenPublicWebsocket',false],['KrakenSpotPublicWebsocket',true],['KrakenPrivateWebsocket',false],['KrakenSpotPrivateWebsocket',true]]){
 classes[name]=[
 {...clone(base),spot,pair:true,argument:name.endsWith('Exchange')?0:{data:'{}'}},
 {...clone(base),spot,missingBindings:true},
 {...clone(base),spot,seed:{authenticated:true,waitingChallenge:true,xhr:'seed',wsToken:'old'}},
 {...clone(base),spot,effectFail:name.endsWith('Exchange')?'superExchange':'superSocket'},
 ];
}
const fill={feed:'fills',fills:[{order_id:'A',qty:'5.5',price:'19.25',instrument:'X',buy:true}]};
const execution={channel:'executions',type:'update',data:[{order_id:'A',symbol:'X',side:'buy',order_qty:'17.25',cum_qty:'5.5',avg_price:'23.75',last_qty:'2.75',exec_type:'trade'}]};
classes.KrakenPrivateWebsocket.push({...clone(base),spot:false,argument:{data:JSON.stringify(fill)},invoke:'onMessage'});
classes.KrakenSpotPrivateWebsocket.push({...clone(base),spot:true,argument:{data:JSON.stringify(execution)},invoke:'onMessage'});
classes.KrakenPublicWebsocket.push({...clone(base),spot:false,argument:'1h',invoke:'mapTimeframe'});
const candle={timestamp:1723638896.125,open:17.25,high:29.5,low:5.75,close:23.125,volume:31.5};
const changed={...candle,close:37.25};
const poll={options:{symbol:'X',interval:'1m'},pages:[[candle],[candle],[changed]]};
const polling=[
 {...clone(poll),fire:2},
 {...clone(poll),pages:[[],[],[]],fire:2},
 {...clone(poll),cleanupBefore:true},
 {...clone(poll),cleanupOnCallback:true},
 {...clone(poll),callbackFail:true},
 {...clone(poll),fetchFail:true,fire:1},
 {...clone(poll),mutateOptions:{symbol:'Y',interval:'5m'},fire:1},
 {...clone(poll),mutateCandle:{close:73.5},fire:1},
 {...clone(poll),pages:[[{...candle,close:NaN}],[{...candle,close:NaN}]],fire:1},
 {...clone(poll),cached:candle,fire:1},
 {...clone(poll),cached:{...candle,open:91},fire:1},
 {...clone(poll),pages:[[{...candle,timestamp:0},candle]],fire:0},
 {...clone(poll),replace:true,oldCleanup:true},
 {...clone(poll),effectFail:'fetch'},
 {...clone(poll),effectFail:'timer'},
 {...clone(poll),fetchSyncFail:true,effectFail:'timer'},
];
for(const row of polling)row.sharedFirstPages=!!(row.pages[0]?.[0]&&row.pages[0][0]===row.pages[1]?.[0]);
module.exports={classes,polling};
