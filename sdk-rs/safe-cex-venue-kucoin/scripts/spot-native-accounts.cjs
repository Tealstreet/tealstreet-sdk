const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {load,root} = require('./tables.cjs');
const types = load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const source = fs.readFileSync(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts'),'utf8');
const code = ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.CommonJS,useDefineForClassFields:true}}).outputText;
function pack(value) {
 if(value===undefined)return ['u'];if(value===null)return ['null'];
 if(typeof value==='string')return ['s',Array.from({length:value.length},(_,i)=>value.charCodeAt(i))];
 if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return ['n',b.toString('hex')];}
 if(typeof value==='boolean')return ['b',value];
 if(Array.isArray(value))return ['a',value.map(pack)];
 return ['o',Object.keys(value).map(key=>[key,pack(value[key])])];
}
async function run(kind) {
 const emitted=[],requests=[],callbacks=[];let reads=0,balanceCalls=0;
 let envelope={code:'200000',data:{userId:'user-11',subUserId:'sub-13',referrerUserId:'affiliate-17'}};
 let resolveBalance; let fail, balance={free:2}, balanceFailure, syncFailure, catchFailure, emitFailures=0;
 const raw={marker:29};
 switch(kind){
 case 'strings':break;
 case 'native-identities':envelope.data={userId:31,subUserId:true,referrerUserId:raw};break;
 case 'falsy-uid':envelope.data={userId:0,uid:53,subUserId:0,referrerUserId:false};break;
 case 'missing-data':delete envelope.data;break;
 case 'raw-rejection':envelope={code:'no',msg:raw};break;
 case 'surrogate-id':envelope.data={userId:'\ud800',subUserId:'\udfff',referrerUserId:'a\ud800b'};break;
 case 'raw-failure':fail={response:{data:{msg:raw}},message:'transport'};break;
 case 'emit-throws':envelope={code:'no',msg:'first'};emitFailures=1;break;
 case 'emit-propagates':fail={message:'first'};emitFailures=2;break;
 case 'callee-order':break;
 case 'live-data':Object.defineProperty(envelope,'data',{get(){reads++;return reads===1?{userId:0}:reads===2?{uid:61}:reads===3?{subUserId:67}:{referrerUserId:71};}});break;
 case 'validate-alias':case 'validate-pending':break;
 case 'validate-numeric-code':envelope={code:200000,msg:raw};break;
 case 'validate-infinity':envelope={code:'no',msg:Infinity};break;
 case 'validate-raw-catch':fail={response:{data:{msg:raw}},message:'transport'};break;
 case 'balance-rejection':balanceFailure={message:'balance'};break;
 case 'balance-sync-throw':syncFailure={response:{data:{msg:raw}}};break;
 case 'catch-getter':catchFailure={response:{data:{msg:raw}}};break;
 case 'null-response':break;
 default:throw Error(`unknown case ${kind}`);
 }
 const endpoints={...types.KUCOIN_SPOT_ENDPOINTS};
 const xhr={};Object.defineProperty(xhr,'get',{get(){if(kind==='callee-order')endpoints.ACCOUNT_SUMMARY='/probe-summary';return function(...args){assert.equal(this,xhr);requests.push(args);return fail?Promise.reject(fail):Promise.resolve(kind==='null-response'?null:{data:envelope});};}});
 class BaseExchange{createXhrWithSecondaryKey(){return xhr;}}
 const module={exports:{}};
 new Function('require','module','exports',code)(name=>{
  switch(name){
   case 'axios-rate-limit':return {default:value=>value};
   case 'dayjs':return {default:()=>{throw Error('unused dayjs');}};
   case '../base':return {BaseExchange};
   case '../clientOrderId':return {patternClientOrderIdRule:()=>({})};
   case './kucoinspot.api':return {createKucoinSpotAPI:()=>xhr};
   case './kucoinspot.types':return {...types,KUCOIN_SPOT_ENDPOINTS:endpoints};
   case './kucoinspot.ws-public':return {KucoinSpotPublicWebsocket:class{}};
   case './kucoinspot.ws-private':return {KucoinSpotPrivateWebsocket:class{}};
   case '../../types':case '../../utils/loop':case '../../utils/omitUndefined':case '../../utils/safe-math':case '../../utils/uuid':return {};
   default:throw Error(`unexpected import ${name}`);
  }
 },module,module.exports);
 const r=new module.exports.KucoinSpotExchange('kucoinspot',{},{});
 r.emitAndLogError=function(value){assert.equal(this,r);emitted.push(value);if(emitFailures-->0)throw {response:{data:{msg:raw}},message:'reporter'};};
 r.fetchBalance=function(){assert.equal(this,r);balanceCalls++;if(syncFailure)throw syncFailure;
  const pending=kind==='validate-pending'?new Promise(resolve=>{resolveBalance=resolve;}):balanceFailure?Promise.reject(balanceFailure):Promise.resolve(balance);
  const original=pending.catch;Object.defineProperty(pending,'catch',{get(){if(catchFailure)throw catchFailure;return function(handler){assert.equal(this,pending);assert.equal(handler.name,'');assert.equal(handler.length,0);callbacks.push([handler.name,handler.length]);return original.call(this,handler);};}});return pending;};
 const validate=kind.startsWith('validate-')||['balance-rejection','balance-sync-throw','catch-getter'].includes(kind);
 let result,thrown;try{const running=r[validate?'validateAccount':'getAccount']();if(kind==='validate-pending'){for(let i=0;i<8;i++)await Promise.resolve();assert.equal(balanceCalls,1);assert.equal(emitted.length,0);let settled=false;running.then(()=>{settled=true;});await Promise.resolve();assert.equal(settled,false);balance.free=77;resolveBalance(balance);}result=await running;}catch(e){thrown=e;}
 if(kind==='native-identities')assert.equal(result.affiliateId,raw);
 if(kind==='raw-rejection'||kind==='raw-failure')assert.equal(emitted[0],raw);
 if(kind==='validate-alias'){assert.equal(result.balance,balance);balance.free=77;}
 if(kind==='validate-raw-catch'){assert.equal(result.error,raw);assert.equal(emitted[0],raw);raw.marker=97;}
 if(kind==='balance-rejection'){assert(Object.hasOwn(result,'balance'));assert.equal(result.balance,undefined);}
 if(kind==='emit-propagates')assert.equal(thrown.response.data.msg,raw);
 return {result:pack(result),thrown:pack(thrown),emitted:emitted.map(pack),requests:requests.map(pack),callbacks,reads,balanceCalls};
}
if(require.main===module)run(process.argv[2]).then(value=>process.stdout.write(JSON.stringify(value))).catch(error=>{console.error(error);process.exitCode=1;});
module.exports={run};
