import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {load,root,ts} from './source.mjs';
const filename='packages/safe-cex/src/exchanges/mexc/mexc.ws-private.ts';
const source=fs.readFileSync(`${root}/${filename}`,'utf8');
const ast=ts.createSourceFile(filename,source,ts.ScriptTarget.Latest,true);
let initializer;
const find=node=>{
  if(ts.isPropertyDeclaration(node) && node.name.getText(ast)==='handlePosition') initializer=node.initializer;
  ts.forEachChild(node,find);
};
find(ast);
assert(initializer,'MexcPrivateWebsocket#handlePosition was not found');
const js=ts.transpileModule(`class Receiver { constructor(store,parent){this.store=store;this.parent=parent;} handlePosition=${initializer.getText(ast)}; } exports.create=(store,parent)=>new Receiver(store,parent);`,{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},
}).outputText;
const utils=load('packages/safe-cex/src/exchanges/mexc/mexc.utils.ts');
if(process.env.MEXC_WS_PRIVATE_POSITION_CONTROL==='entry-price') {
  const original=utils.normalizePosition;
  utils.normalizePosition=(...args)=>{
    const value=original(...args);
    if(value) value.entryPrice += 1;
    return value;
  };
}
const module={exports:{}};
const fromMexcSymbol=utils.fromMexcSymbol,normalizePosition=utils.normalizePosition;
vm.runInNewContext(`(function(exports,fromMexcSymbol,normalizePosition){${js}\n})(module.exports,fromMexcSymbol,normalizePosition)`,{
  module,fromMexcSymbol,normalizePosition,
});
let captured=[];
const store={marketsBySymbol:{},addOrUpdatePositions:positions=>captured=positions};
const parent={};
module.exports.create(store,parent).handlePosition({data:[{
  symbol:'BTC_USDT',positionType:2,holdVol:'3',holdAvgPrice:'12.5',unrealized:'1.25',
  realised:'0.5',liquidatePrice:'8',im:'2',leverage:'10',openType:1,updateTime:1700000000000,
}]});
assert.equal(captured.length,1);
const row=captured[0];
const finiteFields={
  entryPrice:row.entryPrice,notional:row.notional,leverage:row.leverage,
  unrealizedPnl:row.unrealizedPnl,realizedPnl:row.realizedPnl,contracts:row.contracts,
  liquidationPrice:row.liquidationPrice,marginSize:row.marginSize,lastUpdate:row.lastUpdate,
};
const numericBits=Object.fromEntries(Object.entries(finiteFields).map(([key,value])=>[
  key, Number.isNaN(value)?'7ff8000000000000':new DataView(new Float64Array([value]).buffer).getBigUint64(0,true).toString(16).padStart(16,'0'),
]));
process.stdout.write(JSON.stringify({captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0]),id:row.id,symbol:row.symbol,side:row.side,numericBits}));
