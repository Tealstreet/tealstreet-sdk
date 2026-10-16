// Add a separate live case; retain every original fixture value and result.
const base=require('./portfolio_oracle.cjs');
const {BreakoutpropExchange,encode,observe}=base;
const seed=base.corpus.cases.find(c=>c.name==='distinct');
const input=JSON.parse(seed.raw);
const second=structuredClone(input.portfolios[0].orders[0]);
second.orderCode='second-order-cardinality';second.instrument='ETHUSD';
second.legs[0].instrument='ETHUSD';second.legs[0].quantity=29;
second.legs[0].filledQuantity=7;second.legs[0].remainingQuantity=22;
input.portfolios[0].orders.push(second);
const raw=encode(input),expected={};
for(const method of ['formatBalance','formatOpenPositions','formatPositions','formatOrders']){
 const ex=new BreakoutpropExchange('breakoutprop',{},JSON.parse(seed.storeRaw));
 expected[method]={ok:observe(ex[method](JSON.parse(raw))),errors:ex.errors};
}
const supplement={name:'cardinality-two-valid-orders',raw,storeRaw:seed.storeRaw,symbols:null,expected};
const loggerFailures=[];
for(const method of ['formatBalance','formatOrders']){
 const {input,error}=require('./logger_failure.cjs').fixture(seed,method);
 const ex=new BreakoutpropExchange('breakoutprop',{},JSON.parse(seed.storeRaw));let calls=0;
 ex.emitAndLogError=()=>{calls++;throw error;};
 try{ex[method](input);throw Error('logger failure must propagate');}
 catch(caught){if(caught!==error)throw caught;loggerFailures.push({method,raw:encode(input),calls,sameError:caught===error,error:caught});}
}
const corpus={...base.corpus,cases:[...base.corpus.cases,supplement],loggerFailures};
if(require.main===module)console.log(JSON.stringify(corpus));
else module.exports={...base,corpus};
