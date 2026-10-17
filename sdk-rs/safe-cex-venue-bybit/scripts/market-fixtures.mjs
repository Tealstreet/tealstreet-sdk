import ts from 'typescript';
import vm from 'node:vm';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const root=new URL('../../../packages/safe-cex/src/exchanges/bybit/',import.meta.url);
const base={symbol:'BTCUSDT',baseCoin:'BTC',quoteCoin:'USDT',status:'Trading',symbolType:'innovation',marketRegion:'',lotSizeFilter:{qtyStep:'0.001',minOrderQty:'0.01',maxOrderQty:'500'},priceFilter:{tickSize:'0.1'},leverageFilter:{minLeverage:'1',maxLeverage:'100'},deliveryTime:'0'};
const values=[undefined,null,false,true,0,-0,1,'','12.5tail','0x10','Infinity','-Infinity','NaN','\uFEFF12.5','\u008512.5',[],[12.5],{}, {toString:null}];
const cases=[];
const marker=n=>Number.isNaN(n)?'NaN':n===Infinity?'Infinity':n===-Infinity?'-Infinity':Object.is(n,-0)?'-0':'finite';
for(const variant of ['bybit','bybitinverse','bybitspot']) {
 const file=ts.createSourceFile(variant,readFileSync(new URL(`${variant}.exchange.ts`,root),'utf8'),ts.ScriptTarget.Latest,true);
 const cls=file.statements.find(node=>ts.isClassDeclaration(node)&&node.members.some(m=>m.name?.getText(file)==='fetchMarketsHelper'));
 const member=cls.members.find(m=>m.name?.getText(file)==='fetchMarketsHelper');
 let mapper;
 function walk(node){if(ts.isCallExpression(node)&&ts.isPropertyAccessExpression(node.expression)&&node.expression.name.text==='map'&&ts.isArrowFunction(node.arguments[0])) mapper=node.arguments[0];ts.forEachChild(node,walk);}
 walk(member);assert.ok(mapper);
 const context=vm.createContext({});
 vm.runInContext(ts.transpileModule(`globalThis.map=${mapper.getText(file)}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
 const add=(raw,fees=[],category='linear')=>{
  context.fees=fees;context.category=category;
  let result,error,markers;
  try {result=context.map(raw);markers=[result.precision.amount,result.precision.price,result.limits.amount.min,result.limits.amount.max,result.limits.leverage.min,result.limits.leverage.max,...(result.takerFee===undefined?[]:[result.takerFee,result.makerFee])].map(marker);}
  catch(e){error=e.message;}
  cases.push({variant,raw,fees,category,result,error,markers});
 };
 for(const status of ['Trading','PreLaunch','Settling','',null]) for(const category of ['linear','inverse','spot']) add({...base,status},[],category);
 for(const symbolType of ['', 'innovation','stock','ETF','commodity','forex','new-kind']) add({...base,symbolType});
 for(const [parent,key] of [['lotSizeFilter','qtyStep'],['lotSizeFilter','minOrderQty'],['lotSizeFilter','maxOrderQty'],['priceFilter','tickSize'],['leverageFilter','minLeverage'],['leverageFilter','maxLeverage']]) for(const value of values) {
  const raw=structuredClone(base);if(value===undefined)delete raw[parent][key];else raw[parent][key]=value;add(raw);
 }
 for(const value of values) {const raw={...base};if(value===undefined)delete raw.deliveryTime;else raw.deliveryTime=value;add(raw);}
 for(const value of values) add(base,[{symbol:'ignored',takerFeeRate:'99',makerFeeRate:'88'},{symbol:base.symbol,takerFeeRate:value,makerFeeRate:value},{symbol:base.symbol,takerFeeRate:'77',makerFeeRate:'66'}]);
 for(const parent of ['lotSizeFilter','priceFilter','leverageFilter']) for(const value of [undefined,null]){const raw={...base};if(value===undefined)delete raw[parent];else raw[parent]=value;add(raw);}
 add(base,[null]); add(base,[{symbol:base.symbol,takerFeeRate:'1',makerFeeRate:'2'},null]);
 // Competing failures pin source evaluation order: precision before expiry.
 add({...base,lotSizeFilter:null,deliveryTime:{toString:null}});
 for(const value of [undefined,null,false,17,{symbol:'wrong'}]){
  const raw={...base};if(value===undefined)delete raw.symbol;else raw.symbol=value;
  add(raw,[],variant==='bybitinverse'?'inverse':variant==='bybitspot'?'spot':'linear');
  const row=cases[cases.length-1];row.symbolRefusal=true;
  row.ownSymbols=['id','symbol'].map(key=>Object.hasOwn(row.result,key));
  row.symbolIdentity=[row.result.id===raw.symbol,row.result.symbol===raw.symbol];
 }
 for(const symbol of [{symbol:'wrong'},['wrong']]) {
  add({...base,symbol},[{symbol:structuredClone(symbol),takerFeeRate:{toString:null},makerFeeRate:'0.02'}]);
  const row=cases[cases.length-1];row.symbolRefusal=true;
  row.ownSymbols=['id','symbol'].map(key=>Object.hasOwn(row.result,key));
  row.symbolIdentity=[row.result.id===symbol,row.result.symbol===symbol];
  assert.equal(row.error,undefined);assert.equal(row.result.takerFee,undefined);
 }
 for(const raw of [null,{...base,symbol:17,lotSizeFilter:{...base.lotSizeFilter,qtyStep:{toString:null}}}]){
  add(raw);cases[cases.length-1].projectionError=true;
 }
 for(const field of ['baseCoin','quoteCoin']) for(const value of [undefined,null,false,17,{asset:'wrong'}]) {
  const raw={...base}; if(value===undefined)delete raw[field];else raw[field]=value;
  add(raw,[],variant==='bybitinverse'?'inverse':variant==='bybitspot'?'spot':'linear');
  const row=cases[cases.length-1];row.assetRefusal=true;
  row.ownAssets=['base','quote'].map(key=>Object.hasOwn(row.result,key));
  row.assetIdentity=[row.result.base===raw.baseCoin,row.result.quote===raw.quoteCoin];
 }
}
console.log(JSON.stringify(cases));
