import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
import {load,root,ts} from './source.mjs';

const require=createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH),'../mexc-fetch.cjs'));
const filename=path.join(root,'packages/safe-cex/src/exchanges/mexc/mexc.exchange.ts');
const ast=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const selected=[];
for(const node of ast.statements){
 if(ts.isVariableStatement(node)&&node.declarationList.declarations.some(d=>['extractData','extractRows'].includes(d.name.getText(ast))))selected.push(node.getText(ast));
 if(ts.isClassDeclaration(node)&&node.name?.text==='MexcExchange')for(const member of node.members)if(member.name?.getText(ast)==='fetchHistoricalFills')selected.push(`this.fetchHistoricalFills=${member.initializer.getText(ast)};`);
}
const js=ts.transpileModule(selected.join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const types=load('packages/safe-cex/src/types.ts');
const mexcTypes=load('packages/safe-cex/src/exchanges/mexc/mexc.types.ts');
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const omitUndefined=load('packages/safe-cex/src/utils/omitUndefined.ts').omitUndefined;
const {DefaultStore}=load('packages/safe-cex/src/store/store.base.ts');
const marketRaw={symbol:'BTC_USDT',contractSize:0.1,volUnit:3,minVol:2,maxVol:100000,priceScale:5,minLeverage:4,maxLeverage:100};
const raws=[
 {tradeId:'fill-slow',symbol:'BTC_USDT',side:1,price:123.25,vol:5,createTime:1700000000000,orderId:'order-22',fee:-0.25,feeCurrency:'USDT',profit:3.5,role:2},
 {tradeId:'fill-fast',symbol:'BTC_USDT',side:2,price:55.5,vol:7,createTime:1700000001000,orderId:'order-33',fee:0.5,feeCurrency:'USDC',profit:-2.75,role:1},
];
const scenarios=[
 {name:'full-options',options:{symbol:'BTCUSDT',orderId:'order-22',from:1699999999000,to:1700000002000,limit:2.5}},
 {name:'default-limit',options:{symbol:'BTCUSDT',limit:0}},
 {name:'missing-symbol',options:{orderId:'order-22'}},
 {name:'public-only',options:{symbol:'BTCUSDT'},publicOnly:true},
 {name:'non-finite-from',options:{symbol:'BTCUSDT',from:Number.NaN}},
];
const number=value=>{if(Number.isNaN(value))return {$number:'7ff8000000000000'};const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return {$number:bytes.toString('hex')};};
const encode=value=>value===undefined?{$undefined:true}:typeof value==='number'?number(value):Array.isArray(value)?value.map(encode):value&&typeof value==='object'?Object.fromEntries(Object.entries(value).map(([key,item])=>[key,encode(item)])):value;
const output=[];
for(const scenario of scenarios){
 const store=new DefaultStore();store.update({markets:[utils.normalizeMarket(marketRaw)]});
 const requests=[];
 const context=vm.createContext({...types,...mexcTypes,...utils,omitUndefined,store,isPublicDataOnly:scenario.publicOnly??false,
  xhr:{get:async(url,config)=>{requests.push({url,params:config.params});return {data:{data:{items:raws}}};}},
 });
 vm.runInContext(js,context,{filename});
 const result=await context.fetchHistoricalFills.call(context,scenario.options);
 if(result.some(fill=>fill.id==='fill-fast')){
  const fast=result.find(fill=>fill.id==='fill-fast');
  assert.equal(fast.side,'buy','MEXC TypeScript execution side field identity');
  assert.equal(fast.maker,true,'MEXC TypeScript execution role field identity');
 }
 output.push({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),name:scenario.name,marketRaw,requests:encode(requests),fills:result});
}
if(process.env.MEXC_FETCH_FILLS_TS_CONTROL==='empty-source') output.length=0;
process.stdout.write(JSON.stringify(output));
