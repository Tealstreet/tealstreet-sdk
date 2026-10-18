// Complete maintained class and Store; this no-symbol path must never request I/O.
const assert=require('node:assert/strict'),path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const {WeexExchange}=require(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.exchange.ts'));
const {DefaultStore}=require(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
(async()=>{
 const store=new DefaultStore();
 const venue=new WeexExchange('weex',{key:'synthetic-key',secret:'synthetic-secret',passphrase:'synthetic-passphrase',tuning:{consoleLogging:false}},store);
 let requests=0;
 venue.xhr.defaults.adapter=async()=>{requests++;throw new Error('empty market mode change must not perform HTTP');};
 try {
  const pending=venue.changePositionMode(true);
  const prefix=store.options.isHedged;
  assert.equal(prefix,true);
  await pending;
  assert.equal(requests,0);
  const orderRequests=[];
  venue.xhr.defaults.adapter=async config=>{
   const query=new URLSearchParams(config.params).toString();
   orderRequests.push([config.method,config.url+(query?'?'+query:'')]);
   assert.ok(['/capi/v3/openOrders','/capi/v3/openAlgoOrders'].includes(config.url));
   return {data:[],status:200,statusText:'OK',headers:{},config};
  };
  // A caller discards the Promise; the source operation still owns its I/O.
  void venue.fetchOrders();
  for(let turn=0;turn<8 && orderRequests.length<2;turn++) await new Promise(setImmediate);
  assert.deepEqual(orderRequests,[['get','/capi/v3/openOrders?limit=100&page=0'],['get','/capi/v3/openAlgoOrders?limit=100&page=1']]);
  process.stdout.write(JSON.stringify({prefix,settled:store.options.isHedged,requests,orderRequests}));
 }finally{venue.dispose();}
})().catch(error=>{console.error(error);process.exitCode=1;});
