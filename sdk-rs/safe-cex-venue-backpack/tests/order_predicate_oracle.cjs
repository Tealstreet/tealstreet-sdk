const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require},{filename:file});return exports;}
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
let xhrSetups=0,sockets=0;
class Base {createXhrWithSecondaryKey(){xhrSetups++;return unused('xhr');}}
class Socket {constructor(){sockets++;}}
const api=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts'),id=>{
 if(id==='../../types')return types;
 if(id==='./backpack.types')return constants;
 if(id==='../base')return {BaseExchange:Base};
 if(id==='./backpack.api')return {createAPI:unused('createAPI')};
 if(id==='./backpack.ws-public')return {BackpackPublicWebSocket:Socket};
 if(id==='./backpack.ws-private')return {BackpackPrivateWebSocket:Socket};
 return unused(id);
});
const exchange=new api.BackpackExchange('backpack',{},{});
assert.equal(xhrSetups,1);assert.equal(sockets,2);

const bits=n=>{assert.equal(typeof n,'number');if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const cases=[];
const numbers=[0,-0,NaN,Infinity,-Infinity,1.25];
for(const type of Object.values(types.OrderType))for(let mask=0;mask<32;mask++)for(const close of [undefined,false,true]){
 const opts={symbol:'BTC_USDC_PERP',type,side:types.OrderSide.Buy,amount:1};
 const numeric={};
 for(const [i,field] of ['triggerPrice','takeProfit','stopLoss'].entries())if(mask&(1<<i)){opts[field]=numbers[(mask+i)%numbers.length];numeric[field]=bits(opts[field]);}
 if(mask&8)opts.tpTriggerType=types.OrderStopTriggerType.MarkPrice;
 if(mask&16)opts.slTriggerType=types.OrderStopTriggerType.LastPrice;
 if(close!==undefined)opts.close=close;
 const actual=exchange.isOrdinaryLimitOrder(opts);assert.equal(typeof actual,'boolean');
 cases.push({type,mask,close:close===undefined?null:close,numeric,actual});
}
const invalid=[];
for(const field of ['triggerPrice','takeProfit','stopLoss','tpTriggerType','slTriggerType']){
 const opts={symbol:'BTC_USDC_PERP',type:types.OrderType.Limit,side:types.OrderSide.Buy,amount:1,[field]:null};
 const actual=exchange.isOrdinaryLimitOrder(opts);assert.equal(typeof actual,'boolean');
 invalid.push({field,raw:JSON.stringify(opts),actual});
}
const strings=['λ🚀','BTC\ud800'].map(symbol=>{const opts={symbol,type:types.OrderType.Limit,side:types.OrderSide.Buy,amount:1};return {raw:JSON.stringify(opts),symbolRaw:JSON.stringify(symbol),actual:exchange.isOrdinaryLimitOrder(opts)};});
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},cases,invalid,strings}));
