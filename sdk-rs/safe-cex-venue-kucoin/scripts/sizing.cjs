const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root,load}=require('./tables.cjs');
const env={...load(path.join(root,'packages/safe-cex/src/types.ts')),...load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),...load(path.join(root,'packages/safe-cex/src/utils/symbolConverter.ts'))};
const context={store:{},options:{}};
for(const [venue,names] of [['kucoin',['toKucoinSymbol','toInternalSymbol','toApiSymbol','contractSizeFor','nativeAmountForCancelReplace','reconcileFetchedOrderRemainders','getOrderPlanTypeAndAmount','apiMarginCoin']],['kucoinspot',['getUsdPrice']]]) {
 const file=path.join(root,`packages/safe-cex/src/exchanges/kucoin/${venue}.exchange.ts`);
 const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=source.statements.find(ts.isClassDeclaration);
 for(const name of names) {
  const member=cls.members.find(m=>m.name?.getText(source)===name);if(!member)throw Error(name);
  const expr=member.initializer?.getText(source) ?? `function() ${member.body.getText(source)}`;
  const code=ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
  const module={exports:{}};new Function('module',...Object.keys(env),code).call(context,module,...Object.values(env));context[name]=module.exports;
 }
}
module.exports={context};
if(require.main===module){
const cases=JSON.parse(fs.readFileSync(0,'utf8'));
process.stdout.write(JSON.stringify(cases.map(c=>{
 context.store={...c.store,marketsBySymbol:Object.fromEntries(c.store.markets.map(m=>[m.symbol,m])),tickersBySymbol:Object.fromEntries(c.store.tickers.map(t=>[t.symbol,t]))};context.options={testnet:c.testnet};
 const result={};
 const calls={toKucoinSymbol:[c.symbol],toInternalSymbol:[c.symbol],toApiSymbol:[c.symbol],contractSizeFor:[c.symbol],nativeAmountForCancelReplace:[c.order],reconcileFetchedOrderRemainders:[c.fetched],getOrderPlanTypeAndAmount:[c.opts],apiMarginCoin:[],getUsdPrice:[c.currency]};
 for(const [name,args] of Object.entries(calls)){try{result[name]={ok:context[name](...args)}}catch(e){result[name]={error:e.message}}}
 return result;
})));
}
