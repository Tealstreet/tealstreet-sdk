// Execute the real async selector body; receiver operations record dispatch.
const fs=require('node:fs'),path=require('node:path');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
const file=path.join(root,'packages/safe-cex/src/exchanges/bitget/bitget.exchange.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=source.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BitgetExchange');
const member=klass.members.find(n=>n.name?.getText(source)==='getOrderPlanTypeAndAmount');
if(!member)throw Error('missing getOrderPlanTypeAndAmount');
const domainFile=path.join(root,'packages/safe-cex/src/types.ts');
const domain=ts.createSourceFile(domainFile,fs.readFileSync(domainFile,'utf8'),ts.ScriptTarget.Latest,true);
const code=ts.transpileModule(domain.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(domain).replace(/^export\s+/, '')).join('\n')+'\nclass Source {'+member.getText(source)+'}',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const {Source,OrderType,OrderSide}=new Function(code+';return {Source,OrderType,OrderSide};')();
const num=n=>Object.is(n,-0)?'-0':String(n);
(async()=>{
 if(process.argv.includes('--field-identity')) {
  const cases=[];
  for(const amount of [0,1]) {
   const receiver=new Source(),opts={symbol:'BTCUSDT',type:OrderType.StopLoss,side:OrderSide.Buy,amount};
   const row=(id,offset)=>({id,symbol:`symbol:${id}`,side:id==='store'?'long':'short',contracts:offset+1.25,entryPrice:offset+2.5,notional:offset+3.75,leverage:offset+4.25,unrealizedPnl:offset+5.5,liquidationPrice:offset+6.75});
   const store=[row('store',10)],resolved=[row('resolved:first',20),row('resolved:second',30)];
   receiver.store={positions:store};receiver.resolvePositionsForPlanType=async()=>resolved;
   receiver.selectPlanTypeAndAmount=(_opts,positions)=>positions.map(p=>({...p}));
   const inputs={store:structuredClone(store),resolved:structuredClone(resolved)};
   const output=await receiver.getOrderPlanTypeAndAmount(opts);
   cases.push({amount,...inputs,output});
  }
  process.stdout.write(JSON.stringify(cases)+'\n');return;
 }
 const cases=[];
 for(const shape of ['place','order'])for(const type of Object.values(OrderType))for(const side of Object.values(OrderSide))for(const amount of [0,-0,1,-1,NaN,Infinity,-Infinity])for(const failure of ['none','resolve','select']) {
  const receiver=new Source(),calls=[],opts={symbol:'BTCUSDT',type,side,amount};
  if(shape==='order')Object.assign(opts,{id:'existing-order',info:{retained:'metadata'}});
  const store=[{contracts:7}],resolved=[{contracts:13},{contracts:2}];
  const error=new Error('original '+failure);
  receiver.store={get positions(){calls.push(['store']);return store;}};
  receiver.resolvePositionsForPlanType=async(symbol,side)=>{calls.push(['resolve',symbol,side]);if(failure==='resolve')throw error;return resolved;};
  receiver.selectPlanTypeAndAmount=(seen,positions)=>{calls.push(['select',seen===opts,positions===store?'store':'resolved']);if(failure==='select')throw error;return [positions.map(p=>p.contracts),num(seen.amount),seen.id??null,seen.info??null];};
  let output;try{output={value:await receiver.getOrderPlanTypeAndAmount(opts)};}catch(e){output={error:e.message,sameError:e===error};}
  cases.push({name:`${shape}:${type}:${side}:${num(amount)}:${failure}`,shape,type,side,amount:num(amount),failure,calls,output});
 }
 const utf16=[];
 for(const symbol of ['\ud800','x\udc00']) {
  const source=new Source(),calls=[],opts={symbol,type:OrderType.StopLoss,side:OrderSide.Buy,amount:1};
  source.store={positions:[]};
  source.resolvePositionsForPlanType=async(symbol,side)=>{calls.push([symbol,side]);return [];};
  source.selectPlanTypeAndAmount=()=>['pos_loss',0];
  const result=await source.getOrderPlanTypeAndAmount(opts);
  utf16.push({input:JSON.stringify(opts),calls:JSON.stringify(calls),result});
 }
 const aliasSource=new Source(),storeRows=[{contracts:7}];
 aliasSource.store={positions:storeRows};
 aliasSource.resolvePositionsForPlanType=async()=>storeRows;
 aliasSource.selectPlanTypeAndAmount=(_,positions)=>{positions[0].contracts=19;positions.push({contracts:23});return positions;};
 const aliasResult=await aliasSource.getOrderPlanTypeAndAmount({symbol:'BTCUSDT',type:OrderType.StopLoss,side:OrderSide.Buy,amount:0});
 const afterSelector=storeRows.map(p=>p.contracts);
 storeRows[0].contracts=31;const afterStore=aliasResult.map(p=>p.contracts);
 const pendingSource=new Source(),pendingOpts={symbol:'BTCUSDT',type:OrderType.StopLoss,side:OrderSide.Buy,amount:1};
 let release;const pendingCalls=[];
 pendingSource.store={positions:[]};
 pendingSource.resolvePositionsForPlanType=(symbol,side)=>{pendingCalls.push([symbol,side]);return new Promise(resolve=>{release=resolve;});};
 pendingSource.selectPlanTypeAndAmount=seen=>({same:seen===pendingOpts,amount:seen.amount,symbol:seen.symbol,side:seen.side});
 const pending=pendingSource.getOrderPlanTypeAndAmount(pendingOpts);
 pendingOpts.amount=9;pendingOpts.symbol='ETHUSDT';pendingOpts.side=OrderSide.Sell;
 if(release)release([]);
 const afterAwait=await pending;
 process.stdout.write(JSON.stringify({cases,utf16,alias:{afterSelector,afterStore},pendingAlias:{calls:pendingCalls,afterAwait}})+'\n');
})().catch(e=>{console.error(e);process.exitCode=1;});
