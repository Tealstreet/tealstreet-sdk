// Execute exact maintained method AST initializers with genuine enums and DefaultStore.
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'compiler_required');
const {root,load,ts,dependencies}=require('../../safe-cex-types/scripts/accounts-source.cjs');
const enums=load(path.join(root,'packages/safe-cex/src/types.ts'));
const {DefaultStore}=load(path.join(root,'packages/safe-cex/src/store/store.base.ts'));
const {createPositionId}=load(path.join(root,'packages/safe-cex/src/utils/createPositionId.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/blofin/blofin.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BlofinExchange');
const methods={};
for(const name of ['applyConfirmedPositionMode','afterMethodCalledInWorker','getPositionSide']) {
 const member=klass.members.find(n=>n.name?.text===name);
 assert(member&&ts.isPropertyDeclaration(member)&&ts.isArrowFunction(member.initializer),'source_shape:'+name);
 const text=`return function(){return ${member.initializer.getText(ast)}};`;
 const factory=new Function('PositionMode','MarginType','OrderSide','OrderType','uniqBy','createPositionId',ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText)(enums.PositionMode,enums.MarginType,enums.OrderSide,enums.OrderType,dependencies('lodash/uniqBy'),createPositionId);
 methods[name]=factory;
}
function host(store=new DefaultStore()) {
 const state={store,_accountPositionMode:enums.PositionMode.Hedged,_accountMarginType:enums.MarginType.Cross};
 for(const [name,factory] of Object.entries(methods))state[name]=factory.call(state);
 return state;
}
const position=(symbol,side,n)=>({id:`${symbol}:${side}`,symbol,side,entryPrice:101+n,notional:202+n,leverage:3+n,marginType:'cross',positionMode:'hedged',unrealizedPnl:4+n,realizedPnl:5+n,contracts:6+n,liquidationPrice:7+n,marginSize:8+n,info:{tag:n},extra:{contractsNative:9+n},lastUpdate:10+n});
const positions=[position('BTCUSDT','long',1),position('BTCUSDT','short',2),position('ETHUSDT','long',3)];
const leverage={BTCUSDT:{buyLeverage:11,sellLeverage:17,marginType:'isolated',positionMode:'hedged',isFetching:true},ETHUSDT:{buyLeverage:7,sellLeverage:9,marginType:'cross'}};
(async()=>{
 const sideCases=[];
 for(const mode of ['oneway','hedged',null])for(const side of ['buy','sell'])for(const type of ['limit','stop_market','take_profit_market','trailing_stop_market'])for(const reduceOnly of [false,true]){
  const state=host(),trace=[];state.getOrFetchPositionMode=async symbol=>{trace.push(symbol);return mode??undefined};
  const opts={symbol:'BTCUSDT',side,type,reduceOnly};const value=await state.getPositionSide(opts);
  assert.equal(trace.length,1);assert.equal(trace[0],'BTCUSDT');sideCases.push({mode,opts,value,trace});
 }
 assert.equal(sideCases.length,48,'independent_side_case_floor');
 const failure=host();let failureCalls=0;failure.getOrFetchPositionMode=async()=>{failureCalls++;throw Error('mode fetch probe')};await assert.rejects(failure.getPositionSide({symbol:'BTCUSDT',side:'buy',type:'limit'}),/mode fetch probe/);assert.equal(failureCalls,1);
 const applyCases=[];
 for(const mode of ['oneway','hedged'])for(const data of [{}, {fetchedLeverage:leverage}, {fetchedLeverage:leverage,positions}]){
  const store=new DefaultStore();store.update(structuredClone(data));const state=host(store);let updates=0;const observed=[];
  store.subscribe(()=>{updates++;observed.push(state._accountPositionMode)});state.applyConfirmedPositionMode(mode);
  assert.equal(state._accountPositionMode,mode);assert.equal(updates,Object.keys(data).length?1:0);if(updates)assert.equal(observed[0],mode);
  if(data.positions){assert.equal(store.positions.length,mode==='oneway'?2:3);assert.equal(store.positions[0].entryPrice,102);assert.equal(store.positions[0].id,mode==='oneway'?'BTCUSDT':'BTCUSDT:long')}
  applyCases.push({mode,input:data,modeAfter:state._accountPositionMode,updates,positions:store.positions,leverage:store.fetchedLeverage});
 }
 assert.equal(applyCases.length,6,'independent_apply_case_floor');
 const workerInputs=[['changePositionMode',{positionMode:'hedged'},false],['changePositionMode',{positionMode:'oneway'},true],['changePositionMode',null,true],['changePositionMode',{},false],['changePositionMode',{positionMode:''},true],['changePositionMode',{positionMode:null},false],['changePositionMode',0,true],['changePositionMode','hedged',false],['changePositionMode',[],true],['changeMarginType',null,false],['changeMarginType',{positionMode:'oneway'},[]],['unrelated',{positionMode:'oneway'},true]];
 const workerCases=workerInputs.map(([method,result,arg])=>{const state=host();state.afterMethodCalledInWorker(result,method,arg);return {method,result,arg,mode:state._accountPositionMode,margin:state._accountMarginType}});
 assert.equal(workerCases.length,12,'independent_worker_case_floor');assert.equal(workerCases[1].mode,'oneway');assert.equal(workerCases[10].margin,'isolated');
 // Source admits truthy unsupported values; the native PositionMode enum refuses them.
 const unsupported=host();unsupported.afterMethodCalledInWorker({positionMode:'bogus'},'changePositionMode',false);assert.equal(unsupported._accountPositionMode,'bogus','source unsupported mode admitted');
 const throwingStore=new DefaultStore();throwingStore.update({positions});const throwing=host(throwingStore),error=Error('subscriber probe');throwingStore.subscribe(()=>{throw error});assert.throws(()=>throwing.applyConfirmedPositionMode('oneway'),e=>e===error);assert.equal(throwing._accountPositionMode,'oneway');assert.equal(throwingStore.positions.length,2);
 console.log(JSON.stringify({header:{architecture:process.arch,nodeVersion:process.version},sideCases,applyCases,workerCases,stoppedAfter:{side:48,apply:6,worker:12}}));
})().catch(error=>{console.error(error);process.exitCode=1});
