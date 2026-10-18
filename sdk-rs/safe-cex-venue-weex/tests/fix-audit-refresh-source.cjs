// Real private socket callbacks, parent REST methods, API and Store. Timer and
// terminal HTTP hosts are deterministic; neither replaces a venue algorithm.
const assert=require('node:assert/strict'),path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const {WeexExchange}=require(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.exchange.ts'));
const {DefaultStore}=require(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const market={id:'BTCUSDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:.001,price:.1},limits:{amount:{min:.001,max:50},leverage:{min:1,max:80}}};
const turn=()=>new Promise(setImmediate);
(async()=>{
 const observations=[];
 for(const kind of ['orders','positions','account']) for(const disposed of [false,true]){
  const store=new DefaultStore();store.update({markets:[market]});
  const venue=new WeexExchange('weex',{key:'synthetic-key',secret:'synthetic-secret',passphrase:'synthetic-passphrase',tuning:{consoleLogging:false}},store);
  let holding=false;const releases=[],requests=[];
  venue.xhr.defaults.adapter=async config=>{
   const path=config.url;let data;
   if(path==='/capi/v3/openOrders') data=[{orderId:'native-9',symbol:'BTCUSDT',side:'BUY',type:'LIMIT',status:'NEW',origQty:'2',price:holding?'999':'101.5',executedQty:'.25',timeInForce:'GTC',createdTime:1700000000000}];
   else if(path==='/capi/v3/openAlgoOrders') data=[];
   else if(path==='/capi/v3/account/position/allPosition') data=[{symbol:'BTCUSDT',positionSide:'LONG',size:holding?'7':'2',openValue:holding?'714':'200',leverage:'14',marginType:'CROSS',updatedTime:1700000000000}];
   else if(path==='/capi/v3/account/balance') data=[{asset:'USDT',balance:holding?'285':'150',availableBalance:'120',unrealizePnl:'3'}];
   else assert.fail(`unscripted ${path}`);
   requests.push(path);
   if(holding) await new Promise(resolve=>releases.push(resolve));
   return {data,status:200,statusText:'OK',headers:{},config};
  };
  const [orders,positions,balance]=await Promise.all([venue.fetchOrders(),venue.fetchPositions(['BTCUSDT']),venue.fetchBalance()]);
  store.update({orders,positions,balance});
  const state=()=>kind==='orders'?(store.orders.find(v=>v.id==='native-9')?.price??null):kind==='positions'?(store.positions.find(v=>v.contracts>0)?.contracts??null):store.balance.total;
  const before=state(),sent=requests.length;
  const originalSet=global.setTimeout,originalClear=global.clearTimeout;
  const timers=new Map();let next=1;
  global.setTimeout=(callback,ms)=>{const id=next++;timers.set(id,{callback,ms});return id;};
  global.clearTimeout=id=>timers.delete(id);
  holding=true;
  try {
   const event={data:JSON.stringify({e:kind,d:[]})};
   venue.privateWebsocket.onMessage(event);
   const first=[...timers].filter(([,t])=>t.ms===250).map(([id])=>id);
   venue.privateWebsocket.onMessage(event);
   const refresh=[...timers].filter(([,t])=>t.ms===250);
   assert.equal(refresh.length,1);assert.ok(first.every(id=>!timers.has(id)),'replaced debounce callback');
   assert.equal(requests.length,sent,'no fetch before timer');
   const [id,{callback}]=refresh[0];timers.delete(id);
   const waiter=callback(); // Calling the maintained async timer starts the fetch.
   for(let i=0;i<20 && releases.length<(kind==='orders'?2:1);i++)await turn();
   assert.equal(releases.length,kind==='orders'?2:1);
   assert.equal(state(),before,'held response leaves Store untouched');
   if(disposed)venue.dispose();
   for(const release of releases)release();
   await waiter;
   const after=state();
   assert.equal(after,disposed?(kind==='account'?0:null):kind==='orders'?999:kind==='positions'?7:285);
   observations.push({kind,disposed,before,after,requests:requests.length-sent});
  }finally{venue.dispose();global.setTimeout=originalSet;global.clearTimeout=originalClear;}
 }
 assert.equal(observations.length,6);
 process.stdout.write(JSON.stringify(observations));
})().catch(e=>{console.error(e);process.exitCode=1;});
