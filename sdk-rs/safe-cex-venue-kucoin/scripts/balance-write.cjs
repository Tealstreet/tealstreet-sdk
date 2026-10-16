// Reference-only witness: execute the real whole-object assignments, without
// emulating a dictionary setter in Rust or changing the production source.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {root,load}=require('./tables.cjs');
const types=load(path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/kucoin/kucoinspot.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const members=source.statements.find(ts.isClassDeclaration).members;
function bind(name,context){
 const member=members.find(m=>m.name?.getText(source)===name);
 if(!member)throw Error('missing '+name);
 const code=ts.transpileModule('module.exports='+member.initializer.getText(source),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const module={exports:{}};
 new Function('module',...Object.keys(types),code).call(context,module,...Object.values(types));
 return module.exports;
}
(async()=>{
 const results=[];
 for(const currency of JSON.parse(fs.readFileSync(0,'utf8'))){
  const row={currency,available:'2',holds:'1',balance:'3'},calls=[],errors=[];
  const context={store:{balance:{},tickersBySymbol:{'BTC-USDT':{last:7}}},xhr:{async get(url,options){calls.push({url,options});return {data:{code:'200000',data:[row]}};}},emitAndLogError(error){errors.push(error);}};
  context.getUsdPrice=bind('getUsdPrice',context);
  const balance=await bind('fetchBalance',context)();
  const capture=dict=>({ownKeys:Object.keys(dict),ownCurrency:Object.hasOwn(dict,currency),standardPrototype:Object.getPrototypeOf(dict)===Object.prototype,prototype:Object.getPrototypeOf(dict)===Object.prototype?null:Object.getPrototypeOf(dict),wire:JSON.stringify(dict)});
  results.push({currency,currencies:capture(balance.currencies),coins:capture(balance.coins),info:balance.info,calls,errors,globalFree:Object.hasOwn(Object.prototype,'free'),freshFree:'free' in {},freshBase:'base' in {}});
 }
 // DIFFERENT shape: a nested property write reaches Object.prototype. The
 // production method above instead assigns a whole object to the dictionary.
 const field='__kucoinCounterfactual';
 if(Object.hasOwn(Object.prototype,field))throw Error('counterfactual field already exists');
 let nested;
 try{const dict={};dict['__proto__'][field]=1;nested={ownKeys:Object.keys(dict),global:Object.hasOwn(Object.prototype,field),fresh:({})[field]};}
 finally{delete Object.prototype[field];}
 process.stdout.write(JSON.stringify({results,nested,cleaned:!Object.hasOwn(Object.prototype,field)}));
})().catch(e=>{console.error(e);process.exitCode=1});
