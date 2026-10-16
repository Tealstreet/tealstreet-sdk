import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root=new URL('../../../',import.meta.url);
const read=file=>ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);
const run=(text,ctx={})=>{const exports={};vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,...ctx});return exports;};
const shared=read('packages/safe-cex/src/types.ts');
const enums=run(shared.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(shared)).join('\n'));
const math=read('packages/safe-cex/src/utils/safe-math.ts');
const actualMath=run(math.statements.filter(n=>!ts.isImportDeclaration(n)).map(n=>n.getText(math)).join('\n'));
const inverse=read('packages/safe-cex/src/utils/inverseObj.ts');
const {inverseObj}=run(inverse.text);
const results=[];
const encode=n=>n===undefined?'undefined':Object.is(n,-0)?'-0':String(n);
for(const variant of ['binance','binancepm']) {
 const ast=read(`packages/safe-cex/src/exchanges/binance/${variant}.exchange.ts`);
 const klass=ast.statements.find(ts.isClassDeclaration);
 const table=read(`packages/safe-cex/src/exchanges/binance/${variant}.types.ts`);
 const tables=run(table.text,{require:id=>{assert.equal(id,'../../types');return enums;}});
 let minted=0;
 const context={...enums,...tables,...actualMath,inverseObj,uuid:()=>{minted++;return 'uuid-test';}};
 const fn=name=>{const member=klass.members.find(n=>n.name?.getText(ast)===name);assert.ok(member);return run(`exports.fn=function(opts) {return (${member.initializer.getText(ast)})(opts);}`,context).fn;};
 const format=fn('formatCreateTrailingStopLossOrder'),getSide=fn('getOrderPositionSide');
 const trials=[];
 for(const price of [-99.95,-99.85,-99.75,99.95,99.85]) trials.push({last:-100,price,step:0.01,contracts:3,side:'buy',type:enums.OrderType.TrailingStopLoss,hedged:false,positions:'matching',market:true,ticker:true});
 const numbers=[undefined,NaN,-Infinity,-100,-1.5,-1,-0.5,-0,0,0.5,1,1.5,100,Infinity,1e-7,1e21];
 for(const last of numbers.slice(1))for(const price of numbers)for(const step of [0.01,1,-1,0,NaN,Infinity]) trials.push({last,price,step,contracts:3,side:'buy',type:enums.OrderType.TrailingStopLoss,hedged:false,positions:'matching',market:true,ticker:true});
 for(const side of ['buy','sell'])for(const hedged of [false,true])for(const reduceOnly of [undefined,false,true])for(const type of Object.values(enums.OrderType))for(const contracts of [-Infinity,-0,0,1.5,Infinity,NaN])trials.push({last:100,price:101.5,step:0.1,contracts,side,type,hedged,reduceOnly,positions:'matching',market:true,ticker:true});
 for(const market of [false,true])for(const ticker of [false,true])for(const positions of ['empty','wrong-symbol','wrong-side','matching','duplicates'])trials.push({last:100,price:99,step:0.01,contracts:7,side:'buy',type:enums.OrderType.TrailingStopLoss,hedged:true,positions,market,ticker});
 for(const c of trials) {
  const input={symbol:'BTCUSDT',type:c.type,side:c.side,amount:999,price:c.price,reduceOnly:c.reduceOnly};
  const matching={symbol:input.symbol,side:c.side==='buy'?'short':'long',contracts:c.contracts};
  const positions=c.positions==='empty'?[]:c.positions==='wrong-symbol'?[{...matching,symbol:'OTHER'}]:c.positions==='wrong-side'?[{...matching,side:c.side==='buy'?'long':'short'}]:c.positions==='duplicates'?[matching,{...matching,contracts:999}]:[matching];
  const owner={store:{marketsBySymbol:c.market?{BTCUSDT:{precision:{price:c.step}}}:{},tickersBySymbol:c.ticker?{BTCUSDT:{last:c.last}}:{},positions,options:{isHedged:c.hedged}},getOrderPositionSide:getSide};
  minted=0;let expected;try{expected={value:format.call(owner,input)};}catch(e){expected={error:e.message};}
  results.push({variant,...c,last:encode(c.last),price:encode(c.price),step:encode(c.step),contracts:encode(c.contracts),expected,minted});
 }
}
process.stdout.write(JSON.stringify(results));
