// Source objects are constructed in the declaration order used by the model
// codec. Presence is observed before JSON.stringify removes own undefined.
const declarations = {
  order: [
    ['id','o'], ['parentId','parent'], ['status','open'], ['symbol','BTCUSDT'],
    ['type','limit'], ['side','buy'], ['price',7], ['amount',2], ['filled',0],
    ['remaining',2], ['reduceOnly',false], ['close',false], ['info',null],
    ['clientOrderId','client'], ['timeInForce','GoodTillCancel'], ['lastUpdate',1],
    ['triggerType','index_price'], ['tpTriggerType','last_price'], ['slTriggerType','mark_price'],
    ['takeProfit',9], ['stopLoss',5], ['extra',{}],
  ],
  position: [
    ['id','p'], ['symbol','BTCUSDT'], ['side','long'], ['entryPrice',10], ['notional',20],
    ['leverage',2], ['marginType','cross'], ['positionMode','oneway'], ['unrealizedPnl',0],
    ['realizedPnl',3], ['contracts',2], ['liquidationPrice',0], ['marginSize',4],
    ['info',null], ['extra',{}], ['lastUpdate',1], ['option',{}],
  ],
};
const optional = {
  order: ['parentId','status','type','side','price','close','info','clientOrderId',
    'timeInForce','triggerType','tpTriggerType','slTriggerType','takeProfit','stopLoss','extra'],
  position: ['side','marginType','positionMode','realizedPnl','marginSize','info','extra','lastUpdate','option'],
};
const cases=[];
function capture(model,fields,states) {
 const row={};
 for(const [key,value] of declarations[model]) {
   const index=fields.indexOf(key);
   if(index>=0) {if(states[index]) row[key]=states[index]===1?undefined:value;}
   else row[key]=value;
 }
 cases.push({model,fields,states,input:JSON.stringify(row),wire:JSON.stringify(row),keys:Object.keys(row)});
}
for(const model of ['order','position']) {
 for(const key of optional[model]) for(const state of [0,1,2])capture(model,[key],[state]);
 const fields=model==='order'?['type','price','stopLoss']:['side','marginSize','positionMode'];
 for(let mask=0;mask<27;mask++){let n=mask;const states=[];for(let i=0;i<3;i++){states.push(n%3);n=Math.floor(n/3);}capture(model,fields,states);}
}
if(cases.length!==126)throw new Error(`model corpus count ${cases.length}`);
process.stdout.write(JSON.stringify(cases));
