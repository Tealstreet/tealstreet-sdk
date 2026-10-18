// Construct maintained classes with real Store/Axios/interceptors; terminal HTTP only.
const assert=require('node:assert/strict');const path=require('node:path');
const {root,load}=require('./inverse-source-loader.cjs');
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const {setTelemetrySink}=load(path.join(root,'packages/safe-cex/src/telemetry.ts'));
(async()=>{
 const observations=[];
 for(const [stem,name] of [['binance','BinanceExchange'],['binancepm','BinancePMExchange'],['binancespot','BinanceSpotExchange']]){
  const Subject=load(path.join(root,`packages/safe-cex/src/exchanges/binance/${stem}.exchange.ts`))[name];
  for(const method of ['fetchPositions','fetchOrders','cancelSymbolOrders','setLeverage','changePositionMode','changeMarginType'])for(const variant of [false,true]){
   const store=new DefaultStore();
   store.update({markets:['BTC','ETH'].map(base=>({id:base+'USDT',symbol:base+'USDT',base,quote:'USDT',active:true,precision:{amount:0.1,price:0.1},limits:{amount:{min:0.1,max:100},leverage:{min:1,max:50}}}))});
   store.update({positions:['BTCUSDT','ETHUSDT'].map(symbol=>({id:symbol,symbol,side:'long',contracts:0,entryPrice:0,notional:0,leverage:1,unrealizedPnl:0,liquidationPrice:0,marginSize:0,marginType:'cross',positionMode:'oneway'}))});
   const subject=new Subject(stem,{key:'native-key',secret:'native-secret',testnet:variant,tuning:{consoleLogging:false}},store);
   const requests=[],events=[],reports=[];const symbol=variant?'ETHUSDT':'BTCUSDT';
   subject.on('error',v=>events.push(v));setTelemetrySink({captureException:v=>reports.push(v)});
   for(const client of new Set([subject.xhr,subject.papiXhr,subject._primaryXhr,subject._secondaryXhr,subject.publicXhr,subject.unlimitedXHR,subject.fapiXhr].filter(Boolean))){
    client.defaults.adapter=async config=>{
     const u=new URL(config.url,config.baseURL);const query=new URLSearchParams(config.params||{});
     const fields=[...query].filter(([k])=>!['timestamp','recvWindow','signature'].includes(k)).map(([k,v])=>k+'='+v);
     requests.push({method:config.method,base:config.baseURL,path:u.pathname,query:fields,signed:query.has('signature')});
     return {status:200,statusText:'OK',headers:{},config,data:u.pathname==='/api/v3/account'?{balances:[]}:stem==='binancepm'&&method==='fetchOrders'&&variant&&u.pathname==='/papi/v1/um/openOrders'?[{clientOrderId:'case-B',symbol:'ETHUSDT',type:'LIMIT',side:'BUY',price:'42.0',origQty:'2.5',executedQty:'0',reduceOnly:false}]:[]};
    };
   }
   if(method==='fetchPositions'&&stem!=='binancespot')store.setSetting('isHedged',variant);
   const args={fetchPositions:[[symbol]],fetchOrders:[[symbol]],cancelSymbolOrders:[symbol],setLeverage:[symbol,variant?23:17],changePositionMode:[variant,symbol],changeMarginType:[variant,symbol]}[method];
   try{
    let result,error;try{result=await subject[method](...args)}catch(e){error=e}
    const inherited=stem==='binancespot'&&['setLeverage','changePositionMode','changeMarginType'].includes(method);
    if(inherited){assert.equal(error?.message,'Not implemented');assert.equal(requests.length,0);assert.equal(events.length,0);}
    else if(stem==='binancepm'&&method==='changeMarginType'){assert.equal(error?.message,'Portfolio margin is not supported');assert.equal(requests.length,0);assert.deepEqual(events,['Exchange Error: Portfolio margin is not supported']);}
    else {assert.equal(error,undefined,`${stem}.${method}`);assert.ok(requests.length>0,`${stem}.${method}: actual HTTP`);assert.equal(events.length,0);}
    if(method==='fetchOrders'){if(stem==='binancepm'&&variant){assert.equal(result.length,1);assert.equal(result[0].id,'case-B');assert.equal(result[0].amount,2.5);}else assert.deepEqual(result,[]);}
    if(method==='fetchPositions'&&stem==='binancespot')assert.deepEqual(result,[]);
    if(method==='fetchPositions'&&stem!=='binancespot'){assert.deepEqual(result.map(r=>r.id),['BTCUSDT','ETHUSDT'].flatMap(s=>variant?[s+':long',s+':short']:[s]));assert.ok(result.every(r=>Object.is(r.contracts,0)));}
    if(method==='changePositionMode'&&!inherited)assert.equal(store.options.isHedged,variant);
    observations.push({stem,method,variant,requests,error:error?.message??null,events});
   }finally{subject.dispose();setTelemetrySink({});}
  }
 }
 assert.equal(observations.length,36);process.stdout.write(JSON.stringify(observations));
})().catch(e=>{console.error(e);process.exitCode=1});
