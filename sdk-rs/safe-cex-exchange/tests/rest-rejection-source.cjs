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
   if(name==='woo')Object.assign(original,{isAxiosError:true,response:{status:418,headers:{},data:{message:'denied'}},extra:{number:Infinity,missing:undefined,text:'\ud800'}});
   const client=name==='asterdex'?venue.primaryAPI:venue.xhr;
   client.defaults.adapter=async config=>{if(name==='woo')config.paramsSerializer.indexes=false;original.config=config;throw original;};
   try {
    let caught;try{await(name==='asterdex'?venue.syncPositionMode():client.get('/v3/accountinfo'));}catch(error){caught=error;}
    if(name==='woo'){
      assert.equal(caught.message,'[woo] GET /v3/accountinfo failed with 418 undefined: denied');
      assert.equal(caught.context.method,'GET');assert.equal(caught.context.url,'/v3/accountinfo');assert.equal(caught.context.statusCode,418);
    }
    assert.equal(caught.originalError,caught);assert.equal(caught.originalError.message,caught.message);
    if(name==='woo'){assert.equal(caught.originalError.config.paramsSerializer.indexes,false);assert.equal(typeof caught.originalError.config.paramsSerializer.serialize,'function');assert.equal(caught.extra.number,Infinity);assert(Object.hasOwn(caught.extra,'missing'));assert.equal(caught.extra.text.charCodeAt(0),0xd800);}
    assert.equal(caught,original);assert(caught instanceof Error);assert(caught instanceof (name==='asterdex'?axios.AxiosError:TypeError));assert.equal(caught.name,original.name);
    for(const key of ['name','message','code','stack','cause','response','isAxiosError'])assert.equal(caught[key],original[key]);
   }finally{venue.dispose();}
  }
  for(const primitive of ['primitive rejection',42,null,undefined]){
   const raw=axios.create();raw.defaults.adapter=async()=>{throw primitive;};
   let rejected=false,rejection;try{await raw.get('/primitive');}catch(error){rejected=true;rejection=error;}
   assert(rejected);assert.equal(rejection,primitive);
  }
  process.stdout.write(JSON.stringify({asterdex:true,woo:true}));
 }finally{clock.stop();axios.defaults.adapter=prior;}
})().catch(error=>{console.error(error);process.exitCode=1});
