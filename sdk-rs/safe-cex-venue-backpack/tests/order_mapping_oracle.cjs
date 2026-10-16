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

const wsApi=run(path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.ws-private.ts'),id=>{
 if(id==='../../types')return types;
 if(id==='../base.ws')return {BaseWebSocket:class {constructor(parent){this.parent=parent;this.store=parent.store;}}};
 return unused(id);
});
const privateSocket=new wsApi.BackpackPrivateWebSocket({store:{tickers:[]}});
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const bits=n=>{assert.equal(typeof n,'number');if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
const optional=n=>n===undefined?null:bits(n);
const mappingInputs=['Limit','limit','LIMIT','Market','market','MARKET','mArKeT','marKet','MaRKeT','','Market ','stop','λ','\ud800','New','PartiallyFilled','Filled','Cancelled','Expired','TriggerPending','new','GTC','IOC','FOK','PO','ioc'];
const mappings=mappingInputs.map(input=>({input:units(input),type:privateSocket.mapOrderType(input),status:privateSocket.mapOrderStatus(input),tif:privateSocket.mapTimeInForce(input)}));
const tickerScenarios=[[],[{symbol:'OTHER',last:100,mark:200}],[{symbol:'BTC',mark:100}],[{symbol:'BTC',last:0,mark:100}],[{symbol:'BTC',last:-0,mark:100}],[{symbol:'BTC',last:NaN,mark:100}],[{symbol:'BTC',last:Infinity,mark:100}],[{symbol:'BTC',last:-Infinity,mark:100}],[{symbol:'BTC',last:100,mark:200}],[{symbol:'BTC',last:100,mark:200},{symbol:'BTC',last:1000,mark:2000}],[{symbol:'BTC\ud800',last:100,mark:200}]];
const classifications=[];
for(const [scenario,tickers] of tickerScenarios.entries())for(const side of [types.OrderSide.Buy,types.OrderSide.Sell])for(const trigger of [undefined,-Infinity,-0,0,99,100,101,Infinity,NaN]){
 privateSocket.store={tickers};const symbol=scenario===10?'BTC\ud800':'BTC';
 classifications.push({scenario,symbol:units(symbol),tickers:tickers.map(t=>[units(t.symbol),optional(t.last),optional(t.mark)]),side,trigger:optional(trigger),result:privateSocket.determineTriggerOrderType(symbol,side,trigger)});
}
const base={id:'order λ',symbol:'BTC',side:'Bid',orderType:'Limit',price:'105.25',quantity:'8.5',timeInForce:'GTC',selfTradePrevention:'Allow',status:'New',createdAt:1234.5,executedQuantity:'2.25'};
const specs=[
 ['base',{}],['market',{orderType:'Market'}],['trigger buy',{orderType:'Market',triggerPrice:'101',triggerQuantity:'6'}],['trigger sell',{orderType:'Market',side:'Ask',triggerPrice:'99',triggerQuantity:'6'}],
 ['trigger equality',{orderType:'Market',triggerPrice:'100'}],['trigger nan',{orderType:'Market',triggerPrice:'bad'}],['trigger zero',{orderType:'Market',triggerPrice:'0'}],['trigger minus zero',{orderType:'Market',triggerPrice:'-0'}],
 ['limit trigger',{triggerPrice:'12',triggerQuantity:'3'}],['empty trigger',{triggerPrice:''}],['empty trigger quantity',{orderType:'Market',triggerPrice:'99',triggerQuantity:''}],
 ['zero quantity',{orderType:'Market',triggerPrice:'99',quantity:'0'}],['nan quantity',{orderType:'Market',triggerPrice:'99',quantity:'bad'}],['negative zero quantity',{orderType:'Market',triggerPrice:'99',triggerQuantity:'-0'}],
 ['infinite quantity',{quantity:'Infinity'}],['missing filled',{executedQuantity:undefined}],['empty filled',{executedQuantity:''}],['bad filled',{executedQuantity:'bad'}],['negative zero filled',{executedQuantity:'-0'}],
 ['reduce false',{orderType:'Market',triggerPrice:'101',reduceOnly:false}],['reduce true',{reduceOnly:true}],['tif po',{timeInForce:'PO'}],['tif ioc',{timeInForce:'IOC'}],['tif fok',{timeInForce:'FOK'}],
 ['status partial',{status:'PartiallyFilled'}],['status filled',{status:'Filled'}],['status cancelled',{status:'Cancelled'}],['status expired',{status:'Expired'}],['status trigger pending',{status:'TriggerPending'}],
 ['price prefix',{price:' 7.25junk',quantity:'9.5tail'}],['price nan',{price:'bad'}],['quantity negative',{quantity:'-5'}],
];
const originalDetermine=exchange.determineTriggerOrderType;
const orders=[];
for(const [index,[label,patch]] of specs.entries())for(const failure of ['none','classify','log']){
 const input={...base,...patch},trace=[],error={opaque:'failure'};
 exchange.store={tickers:[{symbol:'BTC',last:100,mark:200}]};
 exchange.determineTriggerOrderType=(symbol,side,trigger)=>{trace.push(['classify',symbol,side,bits(trigger)]);if(failure==='classify')throw error;return originalDetermine(symbol,side,trigger);};
 exchange.log=(message,severity)=>{trace.push(['log',message,severity]);if(failure==='log')throw error;};
 let outcome;try{const result=exchange.mapOrder(input);outcome={ok:true,values:{...result,price:bits(result.price),amount:bits(result.amount),filled:bits(result.filled),remaining:bits(result.remaining),lastUpdate:bits(result.lastUpdate)},keys:Object.keys(result)};}
 catch(e){assert.equal(e,error);outcome={ok:false,sameFailure:true};}
 orders.push({index,label,failure,input,trace,outcome});
}
exchange.determineTriggerOrderType=originalDetermine;exchange.log=()=>assert.fail('boundary log');
const boundaryInput={...base,reduceOnly:null,triggerPrice:null,executedQuantity:null};
const boundaryOrder=exchange.mapOrder(boundaryInput);
const boundary={input:boundaryInput,reduceOnly:boundaryOrder.reduceOnly,filled:bits(boundaryOrder.filled),remaining:bits(boundaryOrder.remaining)};
process.stdout.write(JSON.stringify({mappings,classifications,orders,boundary}));
