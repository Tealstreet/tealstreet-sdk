const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require,extra={}){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require,...extra},{filename:file});return exports;}
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
let xhrSetups=0,sockets=0;
class Base {createXhrWithSecondaryKey(){xhrSetups++;return unused('xhr');}}
class Socket {constructor(){sockets++;}}
let defaults,metadataCalls;
const sandbox={Math:Object.create(Math),crypto:{getRandomValues:undefined}};
const api=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts'),id=>{
 if(id==='../../exchangeMetadata')return {getExchangeMetadata(name){assert.equal(name,'backpack');metadataCalls++;return defaults;}};
 if(id==='../../types')return types;
 if(id==='./backpack.types')return constants;
 if(id==='../base')return {BaseExchange:Base};
 if(id==='./backpack.api')return {createAPI:unused('createAPI')};
 if(id==='./backpack.ws-public')return {BackpackPublicWebSocket:Socket};
 if(id==='./backpack.ws-private')return {BackpackPrivateWebSocket:Socket};
 return unused(id);
},sandbox);
const exchange=new api.BackpackExchange('backpack',{},{});
assert.equal(xhrSetups,1);assert.equal(sockets,2);

const bits=n=>{assert.equal(typeof n,'number');if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const observed=v=>v===undefined?['undefined']:v===null?['null']:['number',bits(v)];
const rng=[];
for(const value of [0,1,0x80000000,0xffffffff,5]){
 const calls=[];sandbox.crypto.getRandomValues=array=>{calls.push(['secure',array.length,Array.from(array)]);array[0]=value;return array;};
 sandbox.Math.random=()=>{throw Error('secure path used fallback');};
 rng.push({kind:'secure',input:bits(value),result:bits(exchange.nextBatchClientId()),calls});
}
for(const value of [0,-0,Number.MIN_VALUE,0.5,1-Number.EPSILON/2,1,-Number.EPSILON,-1,NaN,Infinity,-Infinity]){
 const calls=[];sandbox.crypto.getRandomValues=undefined;sandbox.Math.random=()=>{calls.push(['fallback']);return value;};
 rng.push({kind:'fallback',input:bits(value),result:bits(exchange.nextBatchClientId()),calls});
}
const rngFailures=[];
for(const kind of ['secure','fallback']){
 const failure=Object.assign(new Error('entropy failed'),{name:'EntropyError'});const calls=[];
 sandbox.crypto.getRandomValues=kind==='secure'?()=>{calls.push('secure');throw failure;}:undefined;
 sandbox.Math.random=()=>{calls.push('fallback');throw failure;};
 try{exchange.nextBatchClientId();assert.fail('entropy must throw');}catch(e){assert.equal(e,failure);rngFailures.push({kind,calls,name:e.name,message:e.message,sameFailure:true});}
}
const ids=[];
function addIds(label,count,values,throwAt){
 let calls=0;const failure=Object.assign(new Error('generator failed'),{name:'EntropyError'});
 exchange.nextBatchClientId=()=>{const i=calls++;if(i===throwAt)throw failure;return values[i%values.length];};
 let outcome;try{const result=exchange.createBatchClientIds(count);assert.ok(Array.isArray(result));outcome={ok:true,result:result.map(bits)};}
 catch(e){outcome={ok:false,name:e.name,message:e.message,sameFailure:e===failure};}
 ids.push({label,count:bits(count),values:values.map(bits),throwAt:throwAt===undefined?null:throwAt,calls,outcome});
}
addIds('zero',0,[1]);addIds('negative zero',-0,[1]);addIds('negative',-1,[1]);
addIds('nan',NaN,[1]);addIds('infinity',Infinity,[1]);addIds('fractional',2.5,[1,2,3]);
addIds('ordered',3,[9,2,9,7]);
addIds('invalid candidates',2,[0,-0,-1,NaN,Infinity,-Infinity,1.5,0x100000000,0xffffffff,1]);
addIds('duplicates exhausted',2,[1]);
addIds('attempt 64 accepted',1,[...Array(63).fill(0),5]);
addIds('attempt 65 excluded',1,[...Array(64).fill(0),5]);
addIds('full 64',64,Array.from({length:64},(_,i)=>i+1));
addIds('65 unavailable',65,Array.from({length:65},(_,i)=>i+1));
addIds('generator throws',2,[7,8],1);
const triggers=[];
for(const kind of ['tp','sl'])for(const tp of [undefined,...Object.values(types.OrderStopTriggerType)])for(const sl of [undefined,...Object.values(types.OrderStopTriggerType)])for(const explicit of [undefined,...Object.values(types.OrderStopTriggerType)]){
 defaults={defaultTPTriggerType:tp,defaultSLTriggerType:sl};metadataCalls=0;
 const result=exchange.getTriggerBy(kind,explicit);assert.equal(typeof result,'string');
 triggers.push({kind,tp:tp??null,sl:sl??null,explicit:explicit??null,result,metadataCalls});
}
const tickerScenarios=[[],[{symbol:'OTHER',last:100,mark:100}],
 [{symbol:'BTC',last:null,mark:100}],[{symbol:'BTC',mark:200}],
 [{symbol:'BTC',last:0,mark:100}],[{symbol:'BTC',last:-0,mark:100}],
 [{symbol:'BTC',last:NaN,mark:100}],[{symbol:'BTC',last:Infinity,mark:100}],
 [{symbol:'BTC',last:-Infinity,mark:100}],[{symbol:'BTC',last:100,mark:200}],
 [{symbol:'BTC',last:200,mark:100}],[{symbol:'BTC',last:100,mark:200},{symbol:'BTC',last:1000,mark:2000}]];
const classifications=[];
for(const tickers of tickerScenarios)for(const side of [types.OrderSide.Buy,types.OrderSide.Sell])for(const price of [-Infinity,0,99,100,101,Infinity,NaN]){
 exchange.store={tickers};const result=exchange.determineTriggerOrderType('BTC',side,price);assert.equal(typeof result,'string');
 classifications.push({tickers:tickers.map(t=>({symbol:t.symbol,last:observed(t.last),mark:observed(t.mark)})),side,price:bits(price),result});
}
const strings=[];
for(const symbol of ['λ🚀','BTC\ud800']){
 exchange.store={tickers:[{symbol,last:100,mark:200}]};
 strings.push({raw:JSON.stringify(symbol),result:exchange.determineTriggerOrderType(symbol,types.OrderSide.Sell,200)});
}
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},rng,rngFailures,ids,triggers,classifications,strings}));
