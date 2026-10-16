const assert=require('node:assert/strict');
const {corpus,BreakoutpropExchange,store,encode,observe}=require('./portfolio_cardinality_oracle.cjs');
const methods=['fetchBalance','fetchPositions','fetchOpenPositions','fetchOrders'];
(async()=>{
 const loggerFailures=[];
 for(const method of ['fetchBalance','fetchOrders']){
  const {input,error}=require('./logger_failure.cjs').fixture(corpus.cases[0],method);
  const ex=new BreakoutpropExchange('breakoutprop',{},store());let calls=0;
  ex.fetchPortfolioRaw=async()=>input;ex.emitAndLogError=()=>{calls++;throw error;};
  try{await ex[method]();throw Error('logger failure must propagate');}
  catch(caught){if(caught!==error)throw caught;loggerFailures.push({method,raw:encode(input),calls,sameError:caught===error,error:caught});}
 }

 const cases=[];
 const specs=corpus.cases.filter(c=>!c.name.startsWith('unknown-'));
 specs.push(...['null','reject'].map(name=>({name,raw:'null',storeRaw:encode(store()),symbols:null})));
 specs.push(...['null','false','0','-0','""'].map((raw,i)=>({name:'composed-falsy-'+i,raw,storeRaw:encode(store()),symbols:null})));
 for(const c of specs){const expected={};
  for(const method of methods){
   const nativeStore=JSON.parse(c.storeRaw);if(nativeStore.nanMark)nativeStore.tickersBySymbol.BTCUSD.mark=NaN;
   const ex=new BreakoutpropExchange('breakoutprop',{},nativeStore),trace=[];
   const original={message:'raw-rejection',identity:317};
   const actualRaw=ex.fetchPortfolioRaw.bind(ex);
   ex.fetchPortfolioRaw=async()=>{trace.push('raw');if(c.name==='reject')throw original;return JSON.parse(c.raw);};
   if(c.name.startsWith('composed-falsy-')){
    ex.getSessionToken=async()=> 'token';ex.getAccountCode=async()=> 'account-A';
    ex.xhr={get:async()=>({data:JSON.parse(c.raw)})};
    ex.fetchPortfolioRaw=()=>{trace.push('raw');return actualRaw();};
   }
   try{expected[method]={ok:observe(await ex[method](c.symbols===null?undefined:c.symbols)),errors:ex.errors,trace};}
   catch(e){expected[method]={error:e.message,sameError:e===original,errors:ex.errors,trace};}
  }
  cases.push({...c,expected});
 }
 // The null fallback reads the store AFTER the raw request settles, not before.
 const abandoned=[];
 for(const method of methods){
  const input=JSON.parse(corpus.cases[0].raw),ex=new BreakoutpropExchange('breakoutprop',{},store());
  input.portfolios[0].positions[0].lastUpdateTime='2024-01-02T03:04:05';
  input.portfolios[0].orders[0].transactionTime='2024-01-02T03:04:05';
  let settle,formatted=0,rawCalls=0;
  ex.fetchPortfolioRaw=()=>{rawCalls++;return new Promise(resolve=>{settle=resolve;});};
  const formatter={fetchBalance:'formatBalance',fetchPositions:'formatPositions',fetchOpenPositions:'formatOpenPositions',fetchOrders:'formatOrders'}[method];
  const original=ex[formatter].bind(ex);ex[formatter]=(...args)=>{formatted++;return original(...args);};
  void ex[method](); // Deliberately discard the source Promise.
  const before={rawCalls,formatted};
  settle(input);await new Promise(setImmediate); // Drain cross-realm Promise assimilation.
  abandoned.push({method,raw:encode(input),before,after:{rawCalls,formatted}});
 }
 const aliases=[];
 for(const method of methods){
  const input=JSON.parse(corpus.cases[0].raw),s=store(),ex=new BreakoutpropExchange('breakoutprop',{},s);
  ex.fetchPortfolioRaw=async()=>input;
  const output=await ex[method]();
  if(method==='fetchBalance')output.info.portfolios[0].positions[0].symbol='changed';
  else if(method==='fetchOrders')output[0].info.instrument='changed';
  else output[0].info.symbol='changed';
  const observed=method==='fetchOrders'?input.portfolios[0].orders[0].instrument:input.portfolios[0].positions[0].symbol;
  ex.fetchPortfolioRaw=async()=>null;
  const fallback=await ex[method]();
  if(method==='fetchBalance')fallback.free=881;else fallback.push({marker:'changed'});
  const fallbackObserved=method==='fetchBalance'?s.balance.free:method==='fetchOrders'?s.orders.length:s.positions.length;
  aliases.push({method,observed,fallbackObserved});
 }
 const inherited=[];
 for(const witness of corpus.inherited){
  for(const method of methods.filter(m=>witness.field==='position-side'?m!=='fetchOrders':m==='fetchOrders')){
   const ex=new BreakoutpropExchange('breakoutprop',{},store());
   ex.fetchPortfolioRaw=async()=>JSON.parse(witness.raw);
   let beforeSerialization;
   const formatter=witness.field==='position-side'?'formatOpenPositions':'formatOrders';
   const original=ex[formatter].bind(ex);
   ex[formatter]=input=>{const output=original(input),value=output[0][witness.field==='position-side'?'side':witness.field];beforeSerialization={type:typeof value,truthy:!!value};return output;};
   await ex[method]();
   inherited.push({...witness,method,beforeSerialization});
  }
 }
 const utf8=[];
 for(const method of methods){const input=corpus.surrogates[0].raw,ex=new BreakoutpropExchange('breakoutprop',{},store());ex.fetchPortfolioRaw=async()=>JSON.parse(input);const result=await ex[method](['\ud800']);const text=method==='fetchBalance'?result.info.portfolios[0].positions[0].symbol:result[0].symbol;utf8.push({method,raw:input,codeUnit:text.charCodeAt(0)});}
 const deferred=[];
 for(const method of methods){
  const s=store(),ex=new BreakoutpropExchange('breakoutprop',{},s);let resolve;
  ex.fetchPortfolioRaw=()=>new Promise(r=>{resolve=r;});
  const pending=ex[method]();
  s.balance.free=673;s.positions=[{symbol:'AFTER'}];s.orders[0].id='AFTER';
  resolve(null);const result=await pending;
  deferred.push({method,value:method==='fetchBalance'?result.free:method==='fetchOrders'?result[0].id:result[0].symbol});
 }
 assert.equal(deferred.length,4);
 console.log(JSON.stringify({runtime:corpus.runtime,cases,loggerFailures,deferred,inherited,utf8,aliases,abandoned}));
})().catch(e=>{console.error(e);process.exitCode=1;});
