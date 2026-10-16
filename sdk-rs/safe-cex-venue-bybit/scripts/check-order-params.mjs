import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import vm from 'node:vm';
import ts from 'typescript';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const source=ts.createSourceFile('params.ts',readFileSync(root+'packages/safe-cex/src/exchanges/bybit/bybit.order-params.ts','utf8'),ts.ScriptTarget.Latest,true);
const declarations=source.statements.filter(ts.isVariableStatement).map(s=>s.getText(source).replace(/^export /,''));
const context=vm.createContext({OrderType:{Market:'market',Limit:'limit'},OrderTimeInForce:{GoodTillCancel:'GoodTillCancel'}});
vm.runInContext(ts.transpileModule(declarations.join('\n')+'\nglobalThis.get=getBybitOrderExecutionParams;globalThis.single=hasBybitSingleOrderParams;',{}).outputText,context);
const base={symbol:'BTCUSDT',type:'market',side:'buy',amount:1};
const cases=[];
const add=opts=>{for(const category of ['linear','inverse','spot'])cases.push({opts:{...base,...opts},category});};
add({});add({type:'limit'});
for(const v of [null,0,0.001,0.009999999,0.01,0.02,0.1,1,1.001,1.234,10,10.00000001,11,-1])add({maxSlippagePct:v});
for(let i=1;i<=1000;i++) {add({maxSlippagePct:i/100});add({maxSlippagePct:i/100+1e-14});}
for(const ticks of [null,'1',{},[],false,0,-1,0.5,1,2,9999,10000,10001,1e30])add({bybitSlippageToleranceTicks:ticks});
for(const bbo of [null,false,1,'x',[],{}, {sideType:'Queue'}, {level:1}, ...['Queue','Counterparty','bad'].flatMap(sideType=>[null,'1',0,1,2,3,4,5,6,1.5].map(level=>({sideType,level})))])add({type:'limit',bybitBbo:bbo});
for(const extension of [{bybitBbo:{sideType:'Queue',level:3}},{bybitSlippageToleranceTicks:1}]) {
 for(const type of ['market','limit','conditional','stop_market']) {
  add({...extension,type});
  for(const key of ['price','triggerPrice','stopLoss','takeProfit','maxSlippagePct']) for(const value of [0,1])add({...extension,type,[key]:value});
  for(const timeInForce of ['GoodTillCancel','PostOnly','FillOrKill','ImmediateOrCancel'])add({...extension,type,timeInForce});
 }
}
for(const bbo of [null,{sideType:'Queue',level:1}]) for(const ticks of [null,1])add({type:'limit',bybitBbo:bbo,bybitSlippageToleranceTicks:ticks});
for(const reduceOnly of [false,true]) for(const close of [false,true])
 add({type:'limit',symbol:'BTCUSDT',clientOrderId:'identity-client',amount:2,price:3,triggerPrice:5,stopLoss:7,takeProfit:11,maxSlippagePct:0.25,reduceOnly,close,triggerType:'index_price',slTriggerType:'last_price',tpTriggerType:'mark_price'});
const expected=cases.map(({opts,category})=>{try{return {accepted:true,params:context.get(opts,category),single:context.single(opts),roundTrip:opts,plainHasExtension:false};}catch{return {accepted:false};}});
assert.equal(cases.length,6588,'ORDER-PARAMS-CARDINALITY source cases');
const run=spawnSync('cargo',['run','--quiet','--locked','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--example','order-params'],{cwd:root,encoding:'utf8',input:JSON.stringify(cases),maxBuffer:32*1024*1024});
assert.equal(run.status,0,run.error?.message??run.stderr);
assert.deepEqual(JSON.parse(run.stdout),JSON.parse(JSON.stringify(expected)));
console.log(`Bybit execution controls: ${cases.length} live-source cases; accepted params, single-route decision and complete option round-trip agree.`);
