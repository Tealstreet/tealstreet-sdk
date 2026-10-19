const assert = require('node:assert/strict');
const path = require('node:path');
const { setTimeout: deadlineTimer, clearTimeout: clearDeadline } = require('node:timers');
const root = path.resolve(__dirname, '../../..');
const load = file => require(path.join(root, 'packages/safe-cex/src', file));
const { AsterDEXExchange } = load('exchanges/asterdex/asterdex.exchange.ts');
const { AsterDEXPrivateWebsocket } = load('exchanges/asterdex/asterdex.ws-private.ts');
const { asterDEXVirtualClock: clock } = load('exchanges/asterdex/asterdex.virtualClock.ts');
const { generateAsterDEXCodeMessageSignature: signature } = load('exchanges/asterdex/asterdex.api.ts');
const { DefaultStore } = load('store/store.base.ts');
const axios = require('axios');
const deferred = () => { let release; const promise=new Promise(resolve=>{release=resolve}); return { promise, release }; };
const observed = async (promise, label) => {
  let timer;
  try {
    return await Promise.race([promise, new Promise((_, reject) => {
      timer = deadlineTimer(() => reject(new Error(`ASTER-LIFECYCLE ${label} was not observed`)), 5000);
    })]);
  } finally { clearDeadline(timer); }
};
const holdRequestAdmission = () => {
  const queued = deferred();
  const originalSet = global.setTimeout;
  let dispatch;
  global.setTimeout = (callback, delay, ...args) => {
    if (delay !== 0 || dispatch) return originalSet(callback, delay, ...args);
    dispatch = () => callback(...args);
    queued.release();
  };
  return {
    queued: queued.promise,
    release: () => { global.setTimeout = originalSet; assert.equal(typeof dispatch, 'function'); dispatch(); },
    restore: () => { global.setTimeout = originalSet; },
  };
};
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
    const {venue,store}=fixture();store.update({tickers:[{...ticker}]});const gate=deferred(),started=deferred(),updated=deferred();let sent=0;
    const unsubscribe=store.subscribe(data=>{if(data.tickers[0]?.openInterest===7) updated.release();});
    try {
      venue.publicAPI.defaults.adapter=async config=>{sent++;started.release();await gate.promise;return response(config,{symbol:'BTCUSDT',openInterest:'7'});};
      venue.startOpenInterestPolling();await observed(started.promise,'open-interest request');assert.equal(sent,1);
      venue.dispose();store.update({tickers:[{...ticker}]});gate.release();
      await observed(updated.promise,'late source open-interest Store write');assert.equal(store.tickers[0].openInterest,7);result.interest=store.tickers[0].openInterest;
    } finally {unsubscribe();gate.release();venue.dispose();}
  }
  {
    const {venue,store}=fixture();const socket=new AsterDEXPrivateWebsocket(venue,store);const gate=deferred(),started=deferred();let sent=0;
    venue.primaryAPI.defaults.adapter=async config=>{sent++;started.release();await gate.promise;return response(config,[]);};
    const admission=holdRequestAdmission();
    try {
      const pending=socket.runPositionRepair();assert(socket.positionRepairInFlight);
      await observed(admission.queued,'rate-limit admission timer');assert.equal(sent,0);assert(socket.positionRepairInFlight);
      admission.release();await observed(started.promise,'signed position repair request');assert.equal(sent,1);
      await Promise.resolve();assert(socket.positionRepairInFlight);
      socket.dispose();venue.dispose();assert(socket.positionRepairInFlight);gate.release();await observed(pending,'position repair finally');
      assert(!socket.positionRepairInFlight);assert.equal(store.positions.length,0);assert.equal(store.loaded.positions,false);
      result.repair={queued:true,sent,before:true,after:false,positions:0,loaded:false};
    } finally {admission.restore();gate.release();socket.dispose();venue.dispose();}
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
