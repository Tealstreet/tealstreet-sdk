// Unchanged source field initializers; collaborators delimit transport/Store hosts.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const {ts,root,load}=require('./configuration.cjs');
const {ENDPOINTS}=load(path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.types.ts'));
const {PositionSide,PositionMode,MarginType}=load(path.join(root,'packages/safe-cex/src/types.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bitrue/bitrue.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitrueExchange');
const names=['fetchAndSyncSymbolMaxLeverage','fetchSymbolMaxLeverage','updateSymbolMetadata','fetchBalanceAndPositionsRaw','fetchBalance','fetchPositionMetadata','isSuccessCode','getPositionAccounts','getAccountPositionSnapshots','parseBalance'];
const bind={};
for(const name of names){const members=klass.members.filter(m=>m.name?.getText(source)===name);assert.equal(members.length,1,'ENTRY-SOURCE-MEMBER:'+name);assert(ts.isArrowFunction(members[0].initializer));const code=ts.transpileModule('return ('+members[0].initializer.getText(source)+');',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;bind[name]=ctx=>new Function('ENDPOINTS','PositionSide','PositionMode','MarginType',code).call(ctx,ENDPOINTS,PositionSide,PositionMode,MarginType);}
function context(){const ctx={isPublicDataOnly:false,isDisposed:false,positionMetadataBySymbol:new Map(),store:{balance:{tag:'retained'},marketsBySymbol:{}}};for(const name of names)ctx[name]=bind[name](ctx);return ctx;}
(async()=>{
 const rows=[];
 for(const [name,method] of [['sync','fetchAndSyncSymbolMaxLeverage'],['leverage','fetchSymbolMaxLeverage'],['update','updateSymbolMetadata'],['raw','fetchBalanceAndPositionsRaw'],['balance','fetchBalance'],['position','fetchPositionMetadata']]){
  const ctx=context();let calls=0,release,emits=0;const request=()=>{calls++;return new Promise(resolve=>{release=resolve;});};ctx.xhr={get:request};ctx.emitAndLogError=()=>{emits++;};
  if(name==='balance'||name==='position')ctx.fetchBalanceAndPositionsRaw=request;
  const promise=ctx[method](name==='position'?['BTC']:'BTC');
  assert.equal(calls,1,'ENTRY-SOURCE-START:'+name);assert.equal(emits,0);const callsAtReturn=calls;
  release(name==='raw'?{data:{code:0,tag:17}}:(name==='balance'||name==='position')?undefined:{data:undefined});
  const value=await promise;
  let result=value===undefined?null:value;
  if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);result=b.toString('hex');}
  if(name==='balance'){assert.equal(value,ctx.store.balance,'ENTRY-SOURCE-BALANCE-IDENTITY');result='retained';}
  rows.push({name,callsAtReturn,result});
 }
 const ctx=context();let fetches=0;ctx.fetchBalanceAndPositionsRaw=()=>{fetches++;throw Error('unexpected fetch');};
 const supplied=[{symbol:'BTC',side:PositionSide.Long,leverageLevel:17,positionType:2}];
 const promise=ctx.fetchPositionMetadata(['BTC'],supplied);const beforeReturn=ctx.positionMetadataBySymbol.get('BTC').leverage;assert.equal(beforeReturn,17,'ENTRY-SOURCE-CACHE');ctx.positionMetadataBySymbol.clear();const selectedAfterClear=(await promise)[0].leverage;assert.equal(selectedAfterClear,17);
 ctx.isDisposed=true;const disposed=ctx.fetchPositionMetadata(['BTC'],supplied);const disposedRebuilt=ctx.positionMetadataBySymbol.size===1;assert(disposedRebuilt);assert.deepEqual(await disposed,[]);ctx.isDisposed=false;
 const empty=ctx.fetchPositionMetadata(['BTC'],[]);const emptyCleared=ctx.positionMetadataBySymbol.size===0;assert(emptyCleared);assert.deepEqual(await empty,[]);assert.equal(fetches,0);
 const beginErrors=[];
 for(const emitFails of [false,true]){const ctx=context();let emits=0,message;ctx.xhr={get(){const e=new Error('start rejected');e.response={data:{msg:'source-detail'}};throw e;}};ctx.emitAndLogError=m=>{emits++;message=m;if(emitFails){const e=new Error('emit rejected');e.name='SubscriberError';e.ids=[9,3];throw e;}};
  const promise=ctx.fetchBalanceAndPositionsRaw();assert.equal(emits,1,'ENTRY-SOURCE-SYNC-ERROR');assert.equal(message,'source-detail');let outcome;try{assert.equal(await promise,undefined);outcome='undefined';}catch(e){assert.equal(e.name,'SubscriberError');assert.deepEqual(e.ids,[9,3]);outcome=e.name;}
  beginErrors.push({emitFails,emitsAtReturn:emits,message,outcome});
 }
 assert.equal(rows.length,6);process.stdout.write(JSON.stringify({rows,cache:{beforeReturn,selectedAfterClear,emptyCleared,disposedRebuilt},beginErrors}));
})().catch(e=>{console.error(e);process.exitCode=1;});
