// Live bit capture: validated Node 26.7.0 on darwin/arm64; CI captures on its own host.
import {captureBits, captureArchitecture, captureNodeMajor} from './capture_bits.mjs';
import fs from 'node:fs';
import vm from 'node:vm';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const entry=require.resolve(process.env.SAFE_CEX_WORKER_LODASH_PATH?process.env.SAFE_CEX_WORKER_LODASH_PATH+'/lodash.js':'lodash');
assert.equal(crypto.createHash('sha256').update(fs.readFileSync(entry)).digest('hex'),'f5465f55566bf544aad0a31c6135889ca1ed81eea8f53ec61c6cbe86926f07cf');
const lodash=require(entry);assert.equal(lodash.VERSION,'4.18.1');
const root=new URL('../../../packages/safe-cex/src/',import.meta.url);
function body(path){const ast=ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true);return ast.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(ast)).join('\n').replace(/\bexport /g,'');}
const ast=ts.createSourceFile('types',fs.readFileSync(new URL('types.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const names=['OrderSide','OrderType','OrderStatus','PositionSide','PositionMode'];
const enums=ast.statements.filter(n=>ts.isEnumDeclaration(n)&&names.includes(n.name.text));
if(enums.length!==names.length)throw Error('risk enum inventory changed');
let events=[],observedData,failReport=false;
const context=vm.createContext({structuredClone,uniqBy:lodash.uniqBy,Date:{now:()=>1000},console:{trace(){events.push(['trace',Object.keys(observedData.ordersRisk)]);},error(e){events.push(['error',e.name,e.message,Object.keys(observedData.ordersRisk)]);if(failReport)throw new TypeError('report failed');}}});
const clockAst=ts.createSourceFile('clock',fs.readFileSync(new URL('utils/virtualClock.ts',root),'utf8'),ts.ScriptTarget.Latest,true);
const clock=clockAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='VirtualClock');
if(!clock)throw Error('missing actual VirtualClock');
const source=clock.getText(clockAst)+'\nconst virtualClock=new VirtualClock();\n'+enums.map(n=>n.getText(ast).replace('export ','')).join('\n')+'\n'+['utils/clone.ts','utils/safe-math.ts','store/store.base.ts','utils/risk.ts','webworker/postProcessors/BasePostprocessor.ts','webworker/postProcessors/OrdersRiskProfilePostprocessor.ts'].map(body).join('\n');
vm.runInContext(ts.transpileModule(source+'\nglobalThis.Store=DefaultStore;globalThis.Processor=OrdersRiskProfilePostprocessor;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const market={id:'BTC',symbol:'BTC',base:'BTC',quote:'USD',active:true,precision:{amount:.01,price:.01},limits:{amount:{min:0,max:100},leverage:{min:1,max:100}},makerFee:.001,takerFee:.002};
const ticker={id:'BTC',symbol:'BTC',bid:99,ask:101,last:100,mark:100,index:100,percentage:0,openInterest:0,fundingRate:0,fundingTime:0,volume:0,quoteVolume:0};
const position={id:'p',symbol:'BTC',side:'long',entryPrice:100,contracts:2,notional:200,leverage:5,unrealizedPnl:10,liquidationPrice:0};
const order=(id,price)=>({id,symbol:'BTC',status:'open',type:'limit',side:'sell',price,amount:1,filled:0,remaining:1,reduceOnly:true,lastUpdate:0});
const seed={markets:[market],tickers:[ticker],positions:[position],orders:[order('prior',105)]};
function setup(){const store=new context.Store();let data;store.subscribe(d=>data=d);store.update(structuredClone(seed),true);const processor=new context.Processor('a','bybit',{});observedData=data;events=[];failReport=false;processor.process(data);return {store,data,processor};}
function capture(value){
 if(typeof value==='number')return {$bits:captureBits(value)};
 if(Array.isArray(value))return value.map(capture);
 if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,capture(v)]));
 return value;
}
function run(input,fail=false){const {store,data,processor}=setup();const prior=structuredClone(data.ordersRisk);store.update(structuredClone(input),true);observedData=data;events=[];failReport=fail;let thrown=null;try{processor.process(data);}catch(e){thrown={name:e.name,message:e.message};}return {input,fail,prior:capture(prior),events,thrown,risk:capture(data.ordersRisk),keys:Object.keys(data.ordersRisk)};}
const rows=[];
for(const key of Object.getOwnPropertyNames(Object.prototype))for(const field of ['symbol','id'])for(const price of [90,110])for(const fail of [false,true])rows.push(run({...seed,orders:[...seed.orders,{...order('bad',price),[field]:key}]},fail));
const controls=[];
for(const key of Object.getOwnPropertyNames(Object.prototype))for(const field of ['symbol','id'])controls.push(run({...seed,orders:[...seed.orders,{...order('bad',110),[field]:key,status:'closed'}]}));
controls.push(run({...seed,positions:[],orders:[]}));
const ordering=[['z','a','10','2'],['2','10','a','z'],['a','z','01','1']].map(ids=>run({...seed,orders:ids.map((id,i)=>order(id,101+i))}));
const {data,processor}=setup();const changed=structuredClone(seed);changed.markets[0].symbol='ETH';changed.tickers[0].symbol='ETH';changed.positions[0].symbol='ETH';changed.positions[0].id='later-position';changed.orders[0].symbol='ETH';changed.orders[0].id='later';
data.markets.push(...changed.markets);data.tickers.push(...changed.tickers);data.positions.push(...changed.positions);data.orders.push(...changed.orders);observedData=data;events=[];processor.process(data);
const cached=structuredClone(data.ordersRisk);new context.Processor('a','bybit',{}).process(data);
process.stdout.write(JSON.stringify({captureArchitecture,captureNodeMajor,seed,rows,controls,ordering,cache:{input:{markets:data.markets,tickers:data.tickers,positions:data.positions,orders:data.orders},cached:capture(cached),fresh:capture(data.ordersRisk)}}));
