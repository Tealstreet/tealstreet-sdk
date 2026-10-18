const assert=require('node:assert/strict');
const path=require('node:path');
const root=path.resolve(__dirname,'../../..');
const load=file=>require(path.join(root,'packages/safe-cex/src',file));
const {AsterDEXExchange}=load('exchanges/asterdex/asterdex.exchange.ts');
const {WOOXExchange}=load('exchanges/woo/woo.exchange.ts');
const {DefaultStore}=load('store/store.base.ts');
const {asterDEXVirtualClock:clock}=load('exchanges/asterdex/asterdex.virtualClock.ts');
const axios=require('axios');
(async()=>{
 const prior=axios.defaults.adapter;axios.defaults.adapter=async config=>({config,status:200,data:{serverTime:Date.now()},headers:{}});
 try {
  for(const name of ['asterdex','woo']){
   const venue=name==='asterdex'?new AsterDEXExchange(name,{key:'fixture',secret:'fixture',tuning:{consoleLogging:false}},new DefaultStore()):new WOOXExchange(name,{key:'fixture',secret:'fixture',applicationId:'fixture-app',tuning:{consoleLogging:false}},new DefaultStore());
   const original=name==='asterdex'?new axios.AxiosError('native Aster rejection','ERR_ASTER_REJECTED'):new TypeError('native WOO rejection');
   Object.assign(original,{code:name==='asterdex'?'ERR_ASTER_REJECTED':'ERR_WOO_REJECTED',stack:`${original.name}: terminal-host`,cause:{message:'root cause',number:Infinity,missing:undefined}});
   if(name==='woo')Object.assign(original,{isAxiosError:true,response:{status:418,headers:{},data:{message:'denied'}}});
   const client=name==='asterdex'?venue.primaryAPI:venue.xhr;
   client.defaults.adapter=async config=>{original.config=config;throw original;};
   try {
    let caught;try{await(name==='asterdex'?venue.syncPositionMode():client.get('/v3/accountinfo'));}catch(error){caught=error;}
    assert.equal(caught,original);assert(caught instanceof Error);assert.equal(caught.name,original.name);
    for(const key of ['name','message','code','stack','cause','response','isAxiosError'])assert.equal(caught[key],original[key]);
   }finally{venue.dispose();}
  }
  process.stdout.write(JSON.stringify({asterdex:true,woo:true}));
 }finally{clock.stop();axios.defaults.adapter=prior;}
})().catch(error=>{console.error(error);process.exitCode=1});
