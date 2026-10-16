const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH ? require('node:module').createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)) : require;
const {root,load:loadTables}=require('./tables.cjs');
const {context}=require('./order-helpers.cjs');
let ids=0;
function load(file) {
 if(file.endsWith('/uuid.ts'))return {uuid:()=>{ids++;return 'fixture-id';}};
 const module={exports:{}};
 const code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
 new Function('module','exports','require',code)(module,module.exports,name=>{
  if(name.startsWith('lodash/'))return runtimeRequire(name);
  const target=path.resolve(path.dirname(file),name+'.ts');
  if(target.endsWith('/types.ts')||target.endsWith('/safe-math.ts'))return loadTables(target);
  if(name.startsWith('.'))return load(target);
  throw Error(`unexpected formatter dependency ${name}`);
 });return module.exports;
}
const env={...loadTables(path.join(root,'packages/safe-cex/src/types.ts')),...loadTables(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),...load(path.join(root,'packages/safe-cex/src/utils/omitUndefined.ts')),...load(path.join(root,'packages/safe-cex/src/exchanges/clientOrderId.ts')),uuid:()=>{ids++;return 'fixture-id';}};
for(const [file,names] of [['kucoin/kucoin.exchange.ts',['formatCreateOrder','formatAlgoOrder','clientOrderIdRule']],['base.ts',['resolveCallerClientOrderId','wouldSplitOrder','assertCallerClientOrderIdUnsplit','rejectPreflight']]]){
 const filename=path.join(root,'packages/safe-cex/src/exchanges',file);
 const source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
 const cls=source.statements.find(ts.isClassDeclaration);
 for(const name of names){
  const member=cls.members.find(m=>m.name?.getText(source)===name);if(!member)throw Error(name);
  const expr=member.initializer?.getText(source)??`function(${member.parameters.map(p=>p.getText(source)).join(',')}) ${member.body.getText(source)}`;
  const code=ts.transpileModule('module.exports='+expr,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
  const module={exports:{}};new Function('module',...Object.keys(env),code).call(context,module,...Object.values(env));context[name]=module.exports;
 }
}
context.emitAndLogError=()=>{};context._exchangeName='kucoin';
// Instantiate the maintained class and admit markets through its actual update.
const assert=require('node:assert/strict');
const {DefaultStore}=require('./spot-store-notifications.cjs');
function sourceStore(markets,positions){
 const store=new DefaultStore();store.update({markets,positions});
 const prototypeMarket=markets.find(market=>market.symbol==='__proto__');
 if(prototypeMarket){
  assert.equal(store.markets.length,1,'source real Store admitted market count');
  assert.equal(store.markets[0],prototypeMarket,'source real Store original market');
  assert.equal(Object.getPrototypeOf(store.marketsBySymbol),prototypeMarket,'source real Store retains original prototype market');
  assert.equal(Object.hasOwn(store.marketsBySymbol,'__proto__'),false);
  assert.equal(Object.hasOwn(store.marketsBySymbol,'precision'),false);
  assert.equal(store.marketsBySymbol.precision,prototypeMarket.precision,'source real Store inherits original precision');
 }
 return store;
}

function projection(value){
 if(context.store.markets[0]?.symbol==='__proto__'){
  assert.deepEqual(Object.keys(value),['symbol','clientOid','side','type','reduceOnly','closeOrder']);
  assert.equal(value.symbol,undefined);assert.equal(value.clientOid,'fixture-id');assert.equal(value.side,'buy');assert.equal(value.type,'market');assert.equal(value.reduceOnly,true);assert.equal(value.closeOrder,true);assert.equal(ids,1);
 }
 return {wire:JSON.stringify(value),keys:Array.isArray(value)?value.map(Object.keys):Object.keys(value),ids};}
(async()=>{
 const cases=JSON.parse(fs.readFileSync(0,'utf8')),out=[];
 for(const c of cases){
  if(c.special)c.opts[c.special.field]=Number(c.special.value);
  context.store=sourceStore(c.markets,c.positions);
  const row={};for(const name of ['formatCreateOrder','formatAlgoOrder']){ids=0;try{row[name]={ok:projection(await context[name](c.opts))};}catch(error){row[name]={error:error.message,ids};}}out.push(row);
 }
 process.stdout.write(JSON.stringify(out));
})().catch(error=>{console.error(error);process.exitCode=1;});
