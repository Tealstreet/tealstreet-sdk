const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const load = file => require(path.join(root, 'packages/safe-cex/src', file));
const { AsterDEXExchange } = load('exchanges/asterdex/asterdex.exchange.ts');
const { AsterDEXPrivateWebsocket } = load('exchanges/asterdex/asterdex.ws-private.ts');
const { asterDEXVirtualClock: clock } = load('exchanges/asterdex/asterdex.virtualClock.ts');
const { generateAsterDEXCodeMessageSignature: signature } = load('exchanges/asterdex/asterdex.api.ts');
const { DefaultStore } = load('store/store.base.ts');
const axios = require('axios');
const turn = () => new Promise(setImmediate);
const until = async predicate => { for (let i=0;i<100&&!predicate();i++) await turn(); assert(predicate()); };
const deferred = () => { let release; const promise=new Promise(resolve=>{release=resolve}); return { promise, release }; };
const fixture = () => { const store=new DefaultStore(); const venue=new AsterDEXExchange('asterdex',{key:'fixture-key',secret:'fixture-secret',tuning:{consoleLogging:false}},store); return {venue,store}; };
const ticker = {id:'BTCUSDT',symbol:'BTCUSDT',bid:0,ask:0,last:1,mark:0,index:0,percentage:0,fundingRate:0,fundingTime:0,openInterest:0,volume:0,quoteVolume:0};
const response = (config,data) => ({config,data,status:200,statusText:'OK',headers:{}});
(async()=>{
  const result={};
  const originalAdapter=axios.defaults.adapter;
  try {
    const {venue}=fixture(); const gate=deferred();
    axios.defaults.adapter=async config=>{await gate.promise;return response(config,{serverTime:Date.now()+1000});};
    const pending=clock.syncClientServerTime(false,'v1');
    assert(clock.syncInFlight); venue.dispose(); gate.release(); await pending;
    assert.equal(clock.syncInFlight,null); assert.equal(clock.getTimeOffset(),0);
    result.clock={settled:true,slot:false,offset:0};
  } finally { axios.defaults.adapter=async config=>response(config,{serverTime:Date.now()});clock.stop(); }
  {
    const {venue,store}=fixture();store.update({tickers:[{...ticker}]});const gate=deferred();let sent=0;
    venue.publicAPI.defaults.adapter=async config=>{sent++;await gate.promise;return response(config,{symbol:'BTCUSDT',openInterest:'7'});};
    venue.startOpenInterestPolling();await until(()=>sent===1);venue.dispose();store.update({tickers:[{...ticker}]});gate.release();
    await until(()=>store.tickers[0].openInterest===7);result.interest=store.tickers[0].openInterest;
  }
  {
    const {venue,store}=fixture();const socket=new AsterDEXPrivateWebsocket(venue,store);const gate=deferred();let sent=0;
    venue.primaryAPI.defaults.adapter=async config=>{sent++;await gate.promise;return response(config,[]);};
    const pending=socket.runPositionRepair();assert(socket.positionRepairInFlight);await until(()=>sent===1);
    socket.dispose();venue.dispose();assert(socket.positionRepairInFlight);gate.release();await pending;
    assert(!socket.positionRepairInFlight);assert.equal(store.positions.length,0);assert.equal(store.loaded.positions,false);
    result.repair={before:true,after:false,positions:0,loaded:false};
  }
  {
    const {venue,store}=fixture();const socket=new AsterDEXPrivateWebsocket(venue,store);const timers=new Map();let id=1;
    const originalSet=global.setTimeout,originalClear=global.clearTimeout;
    try {
      global.setTimeout=(callback,delay)=>{const handle=id++;timers.set(handle,{callback,delay});return handle};
      global.clearTimeout=handle=>timers.delete(handle);
      socket.scheduleListenKeyRefresh(1800000);socket.startListenKeyWatchdog();socket.scheduleBalanceReconcile(0);socket.scheduleOrderReconcile();
      assert.deepEqual([...timers.values()].map(t=>t.delay),[1800000,300000,1000,10000]);
      socket.dispose();assert.equal(timers.size,0);result.delays=[1800000,300000,1000,10000];
    } finally {global.setTimeout=originalSet;global.clearTimeout=originalClear;venue.dispose();}
  }
  {
    const key='0xDEAD_private_fixture_secret';let error;
    try {signature('x=1',key,false);}catch(e){error=e;}
    assert.equal(error.name,'TypeError');assert(error.message.includes(key));result.signature={name:error.name,sourceLeaksKey:true};
  }
  axios.defaults.adapter=originalAdapter;
  process.stdout.write(JSON.stringify(result));
})().catch(error=>{console.error(error);process.exitCode=1});
