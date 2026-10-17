const {BreakoutpropExchange, store, encode, observe} = require('./portfolio_oracle.cjs');
const row = {portfolios:[{
  balances:[{currency:'USD',value:50.25}],
  positions:[{symbol:'BTCUSD',side:'BUY',openPrice:100,quantity:2,quantityNotional:200,marginRate:0.05,lastUpdateTime:'2026-01-01T00:00:00Z'}],
  orders:[{orderCode:'source-order',status:'WORKING',instrument:'BTCUSD',type:'LIMIT',transactionTime:'2026-01-01T00:00:00Z',legs:[{side:'BUY',price:101.25,quantity:3,filledQuantity:1,remainingQuantity:2,positionEffect:'OPEN'}]}],
}]};
const cases=[];
for(const method of ['formatBalance','formatOpenPositions','formatPositions','formatOrders']) {
  for(const malformedUnread of [false,true]) {
    const input=structuredClone(row), state=store();
    if(malformedUnread) for(const field of (method==='formatOrders'?['balances','positions']:method==='formatBalance'?['orders']:['balances','orders'])) input.portfolios[0][field]=[null];
    const ex=new BreakoutpropExchange('breakoutprop',{},state);
    cases.push({method,malformedUnread,raw:encode(input),storeRaw:encode(state),expected:{ok:observe(ex[method](input)),errors:ex.errors}});
  }
}
for(const method of ['formatBalance','formatOpenPositions','formatPositions','formatOrders']) {
  const variants=[['missing-portfolios',{}],['null-portfolio',{portfolios:[null]}]];
  for(const [name,input] of [['null-position',{positions:[null]}],['null-balance',{balances:[null]}],['null-order',{orders:[null]}]]) {
    const data=structuredClone(row);Object.assign(data.portfolios[0],input);variants.push([name,data]);
  }
  for(const value of [0,1700000000123,null,true,[],{toString:null}]) {
    const data=structuredClone(row);data.portfolios[0].positions[0].lastUpdateTime=value;data.portfolios[0].orders[0].transactionTime=value;variants.push(['date-'+JSON.stringify(value),data]);
  }
  const poison=structuredClone(row);Object.assign(poison.portfolios[0],{account:null,version:{unexpected:true}});
  Object.assign(poison.portfolios[0].balances[0],{account:[],version:false});
  Object.assign(poison.portfolios[0].positions[0],{account:null,positionCode:{toString:null},openTime:[]});
  Object.assign(poison.portfolios[0].orders[0],{executions:null,cashTransactions:'unused',account:false,version:[]});
  Object.assign(poison.portfolios[0].orders[0].legs[0],{instrument:null,legRatio:'unused',averagePrice:{ignored:true}});
  variants.push(['poison-unused',poison]);
  const invalidKey=structuredClone(row);invalidKey.portfolios[0].orders[0].status={toString:null};invalidKey.portfolios[0].orders[0].legs=[];variants.push(['key-before-leg',invalidKey]);
  for(const [name,input] of variants) {
    const state=store(),ex=new BreakoutpropExchange('breakoutprop',{},state);let expected;
    try { expected={ok:observe(ex[method](input)),errors:ex.errors}; } catch(error) { expected={error:error.message,errors:ex.errors}; }
    cases.push({method,name,raw:encode(input),storeRaw:encode(state),expected});
  }
}
console.log(JSON.stringify(cases));
