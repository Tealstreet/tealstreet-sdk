const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
function unused(name){return new Proxy(function(){throw Error('unused '+name);},{get(_t,key){if(key==='__esModule')return false;throw Error('unused '+name+'.'+String(key));}});}
function run(file,require){const exports={};const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});assert.equal(compiled.diagnostics.length,0);vm.runInNewContext(compiled.outputText,{exports,require},{filename:file});return exports;}
const severity=run(path.join(root,'packages/logging/src/LogSeverity.ts'),id=>unused(id));
const types=run(path.join(root,'packages/safe-cex/src/types.ts'),id=>id==='@tealstreet/logging/src/LogSeverity'?severity:unused(id));
const constants=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.types.ts'),id=>unused(id));
let xhrSetups=0,sockets=0;
class Base {createXhrWithSecondaryKey(){xhrSetups++;return unused('xhr');}}
class Socket {constructor(){sockets++;}}
const api=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts'),id=>{
 if(id==='p-iteration')return require('p-iteration');
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
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const labels=['base','sell','post only','IOC','FOK','reduce false','reduce true','ordinary market','trigger defined','client zero','client fractional','client max','client overflow','client nan','client infinity','missing market','inactive','read only','lookup throws','amount zero','amount nan','amount infinity','below min','above max','missing price','price zero','price nan','price infinity','price precision zero','amount precision nan','tiny amount rounded zero','large amount overflow','scientific precision','half rounding','whole precision','exponent dot precision','negative zero client','both precision invalid','bound equality'];
const originalOrdinary=exchange.isOrdinaryLimitOrder;
const builders=[];
for(const [index,label] of labels.entries()){
 const opts={symbol:'BTC λ',type:types.OrderType.Limit,side:types.OrderSide.Buy,amount:3.125,price:101.25};
 const market={id:'VENUE 17',active:true,isReadOnly:false,limits:{amount:{min:0.125,max:100}},precision:{price:0.5,amount:0.125}};let clientId=17;
 switch(label){
 case 'sell':opts.side=types.OrderSide.Sell;break;case 'post only':opts.timeInForce=types.OrderTimeInForce.PostOnly;break;case 'IOC':opts.timeInForce=types.OrderTimeInForce.ImmediateOrCancel;break;case 'FOK':opts.timeInForce=types.OrderTimeInForce.FillOrKill;break;
 case 'reduce false':opts.reduceOnly=false;break;case 'reduce true':opts.reduceOnly=true;break;case 'ordinary market':opts.type=types.OrderType.Market;break;case 'trigger defined':opts.triggerPrice=0;break;
 case 'client zero':clientId=0;break;case 'client fractional':clientId=1.5;break;case 'client max':clientId=0xffffffff;break;case 'client overflow':clientId=0x100000000;break;case 'client nan':clientId=NaN;break;case 'client infinity':clientId=Infinity;break;case 'negative zero client':clientId=-0;break;
 case 'inactive':market.active=false;break;case 'read only':market.isReadOnly=true;break;case 'amount zero':opts.amount=0;break;case 'amount nan':opts.amount=NaN;break;case 'amount infinity':opts.amount=Infinity;break;case 'below min':opts.amount=0.1;break;case 'above max':opts.amount=101;break;
 case 'missing price':delete opts.price;break;case 'price zero':opts.price=0;break;case 'price nan':opts.price=NaN;break;case 'price infinity':opts.price=Infinity;break;case 'price precision zero':market.precision.price=0;break;case 'amount precision nan':market.precision.amount=NaN;break;
 case 'tiny amount rounded zero':opts.amount=0.001;market.limits.amount.min=0;break;case 'large amount overflow':opts.amount=1e308;market.limits.amount.max=Infinity;market.precision.amount=0.1;break;
 case 'scientific precision':market.precision.price=1e-7;break;case 'half rounding':opts.price=1.005;market.precision.price=0.01;break;case 'whole precision':market.precision.price=2;market.precision.amount=1;break;case 'exponent dot precision':market.precision.price=1.25e-7;break;
 case 'both precision invalid':market.precision.price=0;market.precision.amount=NaN;break;case 'bound equality':opts.amount=0.125;break;
 }
 const trace=[],failure=Object.assign(new Error('lookup failed'),{name:'LookupError'});
 exchange.isOrdinaryLimitOrder=originalOrdinary;
 exchange.store={marketsBySymbol:new Proxy({},{get(_t,key){trace.push(['market',key]);if(label==='lookup throws')throw failure;return label==='missing market'?undefined:market;}})};
 const input={symbol:opts.symbol,type:opts.type,side:opts.side,amount:bits(opts.amount),price:opts.price===undefined?null:bits(opts.price),trigger:opts.triggerPrice===undefined?null:bits(opts.triggerPrice),tif:opts.timeInForce??null,reduce:opts.reduceOnly??null,client:bits(clientId),market:{id:market.id,active:market.active,readOnly:market.isReadOnly,min:bits(market.limits.amount.min),max:bits(market.limits.amount.max),pp:bits(market.precision.price),ap:bits(market.precision.amount)}};
 let outcome;try{const r=exchange.buildBatchLimitOrderRequest(opts,clientId);outcome={ok:true,request:{...r,clientId:bits(r.clientId)},keys:Object.keys(r)};}catch(e){outcome={ok:false,name:e.name,message:e.message,sameLookup:e===failure};}
 builders.push({index,label,input,trace,outcome});
}
const modes=['empty','normal','all single','pair sync','pair async','single sync','single async','report throws','log throws','format throws','assert throws','length throws','read throws','eligibility throws','sparse','append','shrink','eligibility mutation','single append'];
async function placement(mode,deferred=false){
 const items=mode==='empty'?[]:['A','B','C'].map(id=>({id,eligible:id!=='C'&&!['all single','single append'].includes(mode)}));if(mode==='sparse')delete items[1];
 const trace=[],failure={opaque:'original',toString(){trace.push(['format']);if(mode==='format throws')throw formatError;return 'ActionError: action \ud800 failed';}},reportError={opaque:'report'},logError={opaque:'log'},formatError={opaque:'format'};let release;
 const orders=new Proxy(items,{get(target,key){if(key==='length'){trace.push(['length',target.length]);if(mode==='length throws')throw failure;}else if(typeof key==='string'&&/^\d+$/.test(key)){trace.push(['read',Number(key)]);if(mode==='read throws'&&key==='1')throw failure;}return Reflect.get(target,key);}});
 exchange.assertCallerClientOrderIds=arg=>{assert.equal(arg,orders);trace.push(['assert']);if(mode==='assert throws')throw failure;};
 exchange.canUseNativeBatch=order=>{trace.push(['eligible',order?.id??'undefined']);if(mode==='eligibility throws')throw failure;if(mode==='eligibility mutation'&&order?.id==='A')items[1]={id:'X',eligible:false};return !!order?.eligible;};
 exchange.placeBatchPair=pair=>{const ids=pair.map(o=>o?.id??'undefined');trace.push(['pair',ids]);assert.equal(pair.length,2);if(mode==='append')items.push({id:'D',eligible:true});if(mode==='shrink')items.length=1;if(mode==='pair sync')throw failure;if(['pair async','report throws'].includes(mode))return Promise.reject(failure);if(deferred)return new Promise(resolve=>{release=()=>resolve(ids.map(id=>'p '+id));});return Promise.resolve(ids.map(id=>'p '+id));};
 exchange.placeOrder=order=>{const id=order?.id??'undefined';trace.push(['single',id]);if(mode==='single append'&&id==='A')items.push({id:'D',eligible:false});if(id==='C'&&mode==='single sync')throw failure;if(id==='C'&&['single async','log throws','format throws'].includes(mode))return Promise.reject(failure);return Promise.resolve(['s '+id+(id==='C'?'\ud800':'')]);};
 exchange.emitAndLogError=(error,context)=>{assert.equal(error,failure);trace.push(['report',context]);if(mode==='report throws')throw reportError;};
 exchange.log=(message,severity)=>{trace.push(['log',units(message),severity]);if(mode==='log throws')throw logError;};
 const pending=exchange.placeOrders(orders),atConstruction=structuredClone(trace);let whilePending;
 if(deferred){await Promise.resolve();whilePending=structuredClone(trace);release();}
 let outcome;try{const ids=await pending;outcome={ok:true,ids:ids.map(units)};}catch(e){outcome={ok:false,label:e===failure?'original':e===reportError?'report':e===logError?'log':e===formatError?'format':'unexpected'};}
 return {mode,atConstruction,whilePending,trace,outcome};
}
async function main(){const placements=[];for(const mode of modes)placements.push(await placement(mode));const wait=await placement('normal',true);process.stdout.write(JSON.stringify({builders,placements,wait}));}
main().catch(e=>{console.error(e);process.exitCode=1;});
