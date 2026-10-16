import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';

const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer;
const find=node=>{
  if(ts.isPropertyDeclaration(node) && node.name.getText(ast)==='handleKline') initializer=node.initializer;
  ts.forEachChild(node,find);
};
find(ast);
assert(initializer,'MexcPublicWebsocket#handleKline was not found');
let method=initializer.getText(ast);
if(process.env.MEXC_WS_KLINE_TS_CONTROL==='nullish') {
  const before="String(json.data?.interval || json.data?.i || '')";
  assert(method.includes(before),'missing kline interval mutation anchor');
  method=method.replace(before,"String(json.data?.interval ?? json.data?.i ?? '')");
}
const js=ts.transpileModule(`class Receiver {
  constructor(){this.routedKey=null;}
  handleRoutedMessage(key){this.routedKey=key;}
  handleKline=${method};
} exports.create=()=>new Receiver();`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const module={exports:{}};
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
vm.runInNewContext(`(function(exports,fromMexcSymbol){${js}\n})(module.exports,fromMexcSymbol)`,{
  module,fromMexcSymbol:utils.fromMexcSymbol,
});
const frames=[
  {symbol:'BTC_USDT',data:{interval:'Min1'}},
  {data:{symbol:'BTC_USDT',interval:'',i:'Min5'}},
  {symbol:null,data:{symbol:'BTC_USDT',i:'Min15'}},
  {symbol:'ETH_USDT',data:{interval:0,i:false}},
  {symbol:'BTC_USDT',data:{interval:{x:1},i:'Min5'}},
  {symbol:'BTC_USDT'},
];
process.stdout.write(JSON.stringify(frames.map(frame=>{
  const receiver=module.exports.create();
  receiver.handleKline(frame);
  return {frame,key:receiver.routedKey};
})));
