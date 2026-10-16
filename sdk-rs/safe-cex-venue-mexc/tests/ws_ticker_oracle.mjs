import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer;
const find=node=>{
  if(ts.isPropertyDeclaration(node) && node.name.getText(ast)==='handleTicker') initializer=node.initializer;
  ts.forEachChild(node,find);
};
find(ast);
assert(initializer,'MexcPublicWebsocket#handleTicker was not found');
let method=initializer.getText(ast);
if(process.env.MEXC_WS_TS_CONTROL==='ticker-volume') {
  assert(method.includes('quoteVolume: next.quoteVolume'),'missing ticker mutation anchor');
  method=method.replace('quoteVolume: next.quoteVolume','quoteVolume: next.volume');
}
const js=ts.transpileModule(`class Receiver { constructor(store){ this.store=store; } handleTicker=${method}; } exports.create=store=>new Receiver(store);`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const module={exports:{}};
const fromMexcSymbol=utils.fromMexcSymbol, normalizeTicker=utils.normalizeTicker;
vm.runInNewContext(`(function(exports,fromMexcSymbol,normalizeTicker){${js}\n})(module.exports,fromMexcSymbol,normalizeTicker)`,{
  module,fromMexcSymbol,normalizeTicker,
});
const market={id:'BTC_USDT',symbol:'BTCUSDT',contractSize:0.0001,limits:{amount:{min:1}},precision:{price:2}};
const ticker={id:market.id,symbol:market.symbol,bid:1,ask:2,last:1.5,mark:1.75,index:1.25,percentage:0.1,
  openInterest:3,fundingRate:0.02,fundingTime:1700000000000,volume:5,quoteVolume:7,info:{retained:true}};
const store={marketsBySymbol:{BTCUSDT:market},tickersBySymbol:{BTCUSDT:ticker},updateTicker:(row,patch)=>Object.assign(row,patch)};
module.exports.create(store).handleTicker({symbol:'BTC_USDT',data:{lastPrice:'12',bid1:'11',ask1:'13',fairPrice:'12.5',indexPrice:'12.25',riseFallRate:'0.1',holdVol:'2',fundingRate:'0.001',fundingTime:1700000000000,volume24:'3',amount24:'36'}});
const bits=value=>{if(Number.isNaN(value))return '7ff8000000000000';const b=new ArrayBuffer(8),v=new DataView(b);v.setFloat64(0,value,false);return v.getBigUint64(0,false).toString(16).padStart(16,'0')};
const fields=Object.fromEntries(['bid','ask','last','mark','index','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'].map(key=>[key,bits(ticker[key])]));
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),fields,info:ticker.info}));
