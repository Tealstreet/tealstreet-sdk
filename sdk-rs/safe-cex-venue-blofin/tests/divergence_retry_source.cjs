// Whole maintained class/Store; only the final Axios adapter is scripted.
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const {BlofinExchange} = require(path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts'));
const {DefaultStore} = require(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
(async () => {
 const store = new DefaultStore();
 const venue = new BlofinExchange('blofin',{key:'k',secret:'s',passphrase:'p',tuning:{consoleLogging:false}},store);
 const requests = [];
 for (const xhr of new Set([venue.xhr,venue.unlimitedXHR,venue._primaryXhr,venue._secondaryXhr].filter(Boolean))) xhr.defaults.adapter = async config => {
   requests.push({path:config.url,after:config.params?.after ?? null});
   const after = config.params?.after;
   let data = [];
   if(config.url === '/api/v1/trade/orders-pending') data = null;
   if(config.url === '/api/v1/trade/orders-tpsl-pending') data = !after ? [{tpslId:'page-a'}] : after === 'page-a' ? [{tpslId:'page-b'}] : [];
   return {data:{code:'0',msg:'normal family refused',data},status:200,statusText:'OK',headers:{},config};
 };
 venue.on('error',()=>{});
 try {
   const pending = venue.fetchOrders();
   for(let i=0;i<20;i++) await new Promise(resolve=>setImmediate(resolve));
   const first = requests.filter(r=>r.path === '/api/v1/trade/orders-tpsl-pending');
   assert.deepEqual(first.map(r=>r.after),[null,'page-a','page-b']);
   const result = await pending;
   assert.deepEqual(result,[]);
   process.stdout.write(JSON.stringify({first,all:requests}));
 } finally {venue.dispose();}
})().catch(error=>{console.error(error);process.exitCode=1});
