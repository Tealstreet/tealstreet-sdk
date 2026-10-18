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
  process.stdout.write(JSON.stringify({prefix,settled:store.options.isHedged,requests}));
 }finally{venue.dispose();}
})().catch(error=>{console.error(error);process.exitCode=1;});
