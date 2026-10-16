const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const {root,load}=require('./tables.cjs'),types=load(path.join(root,'packages/safe-cex/src/types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=source.statements.find(ts.isClassDeclaration);
function methods(context,SetType=Set){
 for(const name of ['getUsdPrice','fetchPositions','fetchSymbolMaxLeverage','setLeverage','changeMarginType','changePositionMode']){
  const member=cls.members.find(member=>member.name?.getText(source)===name);if(!member)throw Error(name);
  const code=ts.transpileModule('module.exports='+member.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.CommonJS}}).outputText;
  const module={exports:{}};new Function('module','Set',...Object.keys(types),code).call(context,module,SetType,...Object.values(types));context[name]=module.exports;
 }
}
function tag(value){if(value===undefined)return{undefined:true};if(Array.isArray(value))return value.map(tag);if(value&&typeof value==='object')return Object.fromEntries(Object.keys(value).map(key=>[key,tag(value[key])]));return value;}
async function run(kind){
 let reads=0,setCalls=0;const trace=[],predicates=[];let result,extra={};
 const context={store:{}};
 const SetType=class extends Set{constructor(values){super(values);assert.equal(new.target,SetType);setCalls++;}};methods(context,SetType);
 if(kind==='typed-leverage-alias'){
  const {DefaultStore}=require('./spot-store-notifications.cjs');context.store=new DefaultStore();
  const original={old:{buyLeverage:11,sellLeverage:13},'BTC-USDT':{buyLeverage:17,sellLeverage:19}};
  context.store.update({fetchedLeverage:original});const prior=context.store.fetchedLeverage;
  await context.setLeverage('BTC-USDT',97,'short');const current=context.store.fetchedLeverage;
  assert.notEqual(prior,current);assert.equal(prior.old,current.old);assert.deepEqual(Object.keys(current),['old','BTC-USDT']);
  prior.old.buyLeverage=77;assert.equal(current.old.buyLeverage,77);assert.equal(current.old.sellLeverage,13);
  assert.equal(current['BTC-USDT'].buyLeverage,1);assert.equal(current['BTC-USDT'].sellLeverage,1);
  return {priorBuy:prior.old.buyLeverage,currentBuy:current.old.buyLeverage,currentSell:current.old.sellLeverage,selectedBuy:current['BTC-USDT'].buyLeverage,selectedSell:current['BTC-USDT'].sellLeverage,shared:prior.old===current.old};
 }else if(kind.startsWith('price-')){
  const currency=kind.slice(6),raw={marker:71};let table={};
  if(currency==='BTC')table={'BTC-USDT':{last:11},'BTC-USDC':{last:97}};
  if(currency==='ETH')table={'ETH-USDC':{last:13}};
  if(currency==='SOL')table={'SOL-USD':{last:17}};
  if(currency==='ZERO')table={'ZERO-USDT':{last:0},'ZERO-USDC':{last:19}};
  if(currency==='RAW')table={'RAW-USDT':{last:raw}};
  Object.defineProperty(context.store,'tickersBySymbol',{get(){reads++;return currency==='LIVE'?(reads===1?{}:{'LIVE-USDC':{last:23}}):table;}});
  result=context.getUsdPrice(currency);if(currency==='RAW'){assert.equal(result,raw);raw.marker=101;}
 }else if(kind.startsWith('positions-')){
  const a={symbol:'A',active:true};const markets=[a,{symbol:'B',active:false},{symbol:'C',active:true},a,{symbol:'D',active:0}];
  if(kind==='positions-live-symbol'){
   const row={active:true};Object.defineProperty(row,'symbol',{get(){reads++;return reads===1?'ID1':'VALUE2';}});markets.splice(0,5,row);
  }
  const originalFilter=markets.filter;
  Object.defineProperty(markets,'filter',{value:function(callback){assert.equal(callback.name,'');assert.equal(callback.length,1);return originalFilter.call(this,(...args)=>{const result=callback(...args);predicates.push(result);return result;});}});
  context.store.markets=markets;
  const symbols=kind==='positions-empty'?[]:kind==='positions-C'?['C']:kind==='positions-A-duplicates'?['A','A']:undefined;
  result=await context.fetchPositions(symbols);
  if(kind==='positions-all'||kind==='positions-A-duplicates'){assert.notEqual(result[0],result.at(-1));assert.notEqual(result[0].info,result.at(-1).info);}
  extra={predicates:tag(predicates)};
 }else if(kind.startsWith('leverage-')){
  const old={buyLeverage:11,sellLeverage:13},original={old,'BTC-USDT':{buyLeverage:17,sellLeverage:19}};
  const error=Error('subscriber');let updates=[];
  let update=function(patch){assert.equal(this,context.store);trace.push('call-original');updates.push(patch);if(kind==='leverage-error')throw error;};
  Object.defineProperty(context.store,'update',{get(){trace.push('get-update');return update;}});
  Object.defineProperty(context.store,'fetchedLeverage',{get(){trace.push('get-fetched');if(kind==='leverage-live-update')update=()=>{throw Error('late replacement');};return original;}});
  try{result=await context.setLeverage(kind==='leverage-proto'?'__proto__':'BTC-USDT',97,'short');}catch(e){assert.equal(e,error);result='subscriber';}
  assert.equal(updates.length,1);assert.notEqual(updates[0].fetchedLeverage,original);assert.equal(updates[0].fetchedLeverage.old,old);
  if(kind==='leverage-proto')assert(Object.hasOwn(updates[0].fetchedLeverage,'__proto__'));
  extra={updates:tag(updates),old:tag(original)};
 }else if(kind==='ignored-arguments'){
  const poison=new Proxy({},{get(){throw Error('argument inspected');}});
  result=[await context.fetchSymbolMaxLeverage(poison),await context.changeMarginType(poison,poison),await context.changePositionMode(poison,poison)];
 }else throw Error(`unknown case ${kind}`);
 return {value:tag(result),reads,setCalls,trace,...extra};
}
run(process.argv[2]).then(value=>process.stdout.write(JSON.stringify(value))).catch(error=>{console.error(error);process.exitCode=1;});
