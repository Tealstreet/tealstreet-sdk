import assert from 'node:assert/strict';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
import fs from 'node:fs';

const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer;
const find=node=>{
  if(ts.isPropertyDeclaration(node) && node.name.getText(ast)==='handleDepth') initializer=node.initializer;
  ts.forEachChild(node,find);
};
find(ast);
assert(initializer,'MexcPublicWebsocket#handleDepth was not found');
let method=initializer.getText(ast);
if(process.env.MEXC_WS_DEPTH_TS_CONTROL==='skip-normalizer') {
  const before='this.orderbooks[symbol] = normalizeOrderBook(json.data || json, market);';
  assert(method.includes(before),'missing depth normalizer mutation anchor');
  method=method.replace(before,'this.orderbooks[symbol] = json.data || json;');
}
const js=ts.transpileModule(`class Receiver {
  constructor(store){this.store=store;this.orderbooks={};this.routedKey=null;}
  handleRoutedMessage(key){this.routedKey=key;}
  handleDepth=${method};
} exports.create=store=>new Receiver(store);`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
const module={exports:{}};
vm.runInNewContext(`(function(exports,fromMexcSymbol,normalizeOrderBook){${js}\n})(module.exports,fromMexcSymbol,normalizeOrderBook)`,{
  module,fromMexcSymbol:utils.fromMexcSymbol,normalizeOrderBook:utils.normalizeOrderBook,
});
const market=utils.normalizeMarket({symbol:'BTC_USDT',baseCoin:'BTC',quoteCoin:'USDT',contractSize:0.0001,volUnit:3,priceUnit:0.1,minVol:2,maxVol:100000,minLeverage:4,maxLeverage:125});
const frames=[
  {symbol:'BTC_USDT',data:{bids:[[10,2],[11,3]],asks:[[12,4],[11.5,1]]}},
  {data:{symbol:'BTC_USDT',bid:[[9,5]],ask:[[13,2]]}},
  {symbol:'BTC_USDT',data:false,bids:[[8,1]],asks:[[14,1]]},
  {symbol:null,data:{symbol:'BTC_USDT',bids:[[7,2]],asks:[[15,2]]}},
  {symbol:'MISSING_USDT',data:{bids:[[1,1]],asks:[[2,1]]}},
];
const bits=value=>{if(Number.isNaN(value))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(value);return b.toString('hex')};
const encodeBook=book=>({bids:book.bids.map(level=>[bits(level.price),bits(level.amount),bits(level.total)]),asks:book.asks.map(level=>[bits(level.price),bits(level.amount),bits(level.total)])});
const output=frames.map(frame=>{
  const receiver=module.exports.create({marketsBySymbol:{BTCUSDT:market}});
  receiver.handleDepth(frame);
  const entry=Object.entries(receiver.orderbooks)[0];
  return {captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),frame,output:entry?{symbol:entry[0],route:receiver.routedKey,book:encodeBook(entry[1])}:null};
});
process.stdout.write(JSON.stringify(output));
