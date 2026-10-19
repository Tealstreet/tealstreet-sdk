// Unedited exchange members, real DefaultStore, createPositionId, enums and
// precision helper. Only configured Axios instances/time are injected.
const fs=require('node:fs'),path=require('node:path'),{ts,root}=require('./source.cjs');
const parse=file=>ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true),printer=ts.createPrinter();
const print=node=>printer.printNode(ts.EmitHint.Unspecified,node,node.getSourceFile());
const compile=code=>ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const ast=parse('packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts'),cls=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='AsterDEXExchange');
const names=['getFapiPath','getAccountEndpoint','fetchBalance','reconcileBalanceAfterPrivateUpdate','fetchPositionMode','syncPositionMode','getAccountPositionMode','fetchPositions','transformPosition','fetchOrders','transformOrder','reverseMapOrderType','reverseMapOrderStatus','fetchMarkets','fetchLeverageBrackets','transformMarket','extractFilterValue','fetchTickers','transformTicker','fetchOpenInterestMap','collectOpenInterestFromPayload','setLeverage','changePositionMode'];
const members=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(ast)===name);if(!n)throw Error('Missing source '+name);return print(n)}).join('\n');
const enums=[parse('packages/safe-cex/src/types.ts'),parse('packages/logging/src/LogSeverity.ts')].flatMap(ast=>ast.statements.filter(ts.isEnumDeclaration)).map(print).join('\n');
const idSource=parse('packages/safe-cex/src/utils/createPositionId.ts').statements.filter(n=>!ts.isImportDeclaration(n)).map(print).join('\n');
const {values}=require('./source.cjs');
const pe={};new Function('exports',compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/ccxt_helpers/precision.ts'),'utf8')))(pe);
const exported={};new Function('exports','uniqBy','getAsterDEXFapiPath','asterdexOrderTypeMap','precisionToString',compile(enums+'\n'+idSource+'\nexport class AccountSource {store:any;primaryAPI:any;publicAPI:any;apiVersion:any;isDisposed=false;balanceStateVersion=0;lastBalanceReconciliationAt=0;log:any;\n'+members+'}'))(exported,loadUniq(),values.getAsterDEXFapiPath,values.asterdexOrderTypeMap,pe.precisionToString);
function loadPackage(name){const tsPath=process.env.SAFE_CEX_TYPESCRIPT_PATH||path.join(root,'sdk-rs/target/asterdex-oracle-deps/node_modules/typescript');const dependencyRoot=path.dirname(path.resolve(path.dirname(require.resolve(tsPath)),'..'));return require(path.join(dependencyRoot,name));}
function loadUniq(){return loadPackage('lodash/uniqBy');}
const baseAst=parse('packages/safe-cex/src/exchanges/base.ts');const baseClass=baseAst.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BaseExchange');const disposeMember=baseClass.members.find(n=>n.name?.getText(baseAst)==='dispose');if(!disposeMember)throw Error('Missing BaseExchange.dispose');
const be={};new Function('exports',compile('export class BaseDispose {'+print(disposeMember)+'}'))(be);
const ce={};new Function('exports',compile(fs.readFileSync(path.join(root,'packages/safe-cex/src/utils/clone.ts'),'utf8')))(ce);
const se={};new Function('exports','clone','virtualClock','uniqBy',compile(enums+'\n'+parse('packages/safe-cex/src/store/store.base.ts').statements.filter(n=>!ts.isImportDeclaration(n)).map(print).join('\n')))(se,ce.clone,{getCurrentTimeMs:()=>1234},loadUniq());
const {schemas,sample}=require('./oracle.cjs');
const market=(symbol,patch={})=>({...sample(schemas.AsterDEXMarketResponse),symbol,baseAsset:symbol.replace('USDT',''),quoteAsset:'USDT',status:'TRADING',pricePrecision:2,quantityPrecision:3,filters:[{filterType:'LOT_SIZE',minQty:'0.001',maxQty:'100'}],...patch});
const storeMarket=(symbol,patch={})=>({id:symbol,symbol,base:symbol.replace('USDT',''),quote:'USDT',active:true,precision:{price:0.01,amount:0.001},limits:{amount:{min:0.001,max:100},leverage:{min:1,max:20}},...patch});
const position=(symbol,patch={})=>({...sample(schemas.AsterDEXPositionResponse),symbol,positionAmt:'0',entryPrice:'20',notional:'40',unRealizedProfit:'3',liquidationPrice:'1',positionSide:'BOTH',marginType:'CROSSED',leverage:'7',...patch});
const order=(symbol,patch={})=>({...sample(schemas.AsterDEXOrderResponse),symbol,orderId:21,type:'LIMIT',status:'NEW',side:'BUY',price:'10',origQty:'4',executedQty:'1',stopPrice:'0',closePosition:false,reduceOnly:false,...patch});
const ticker=(symbol,patch={})=>({...sample(schemas.AsterDEXTickerResponse),symbol,lastPrice:'10.5',priceChangePercent:'-2.5',volume:'100',quoteVolume:'200',...patch});
const balance={totalWalletBalance:'100',availableBalance:'70',totalInitialMargin:'20',totalOpenOrderInitialMargin:'10',totalUnrealizedProfit:'3'};
const cases=[
 {name:'positions-hedged-first-real',family:'reads',method:'fetchPositions',initial:{options:{isHedged:true},markets:[storeMarket('BTCUSDT'),storeMarket('ETHUSDT'),storeMarket('SPOT',{isSpot:true}),storeMarket('DEAD',{active:false})]},queue:[{data:[position('BTCUSDT',{positionAmt:'2',positionSide:'LONG'}),position('BTCUSDT',{positionAmt:'99',positionSide:'LONG',leverage:'-12'}),position('ETHUSDT',{leverage:'9'}),position('ETHUSDT',{leverage:'bad'})]}]},
 {name:'positions-oneway-short-filter',family:'reads',method:'fetchPositions',args:[['BTCUSDT']],initial:{markets:[storeMarket('BTCUSDT'),storeMarket('ETHUSDT')]},queue:[{data:[position('BTCUSDT',{positionAmt:'-2'}),position('ETHUSDT',{positionAmt:'3'})]}]},
 {name:'positions-empty-filter',family:'reads',method:'fetchPositions',args:[[]],initial:{markets:[storeMarket('BTCUSDT')]},queue:[{data:[position('BTCUSDT',{positionAmt:'2'})]}]},
 {name:'orders-filter-and-stop',family:'reads',method:'fetchOrders',args:[['BTCUSDT']],queue:[{data:[order('ETHUSDT'),order('BTCUSDT',{type:'STOP_MARKET',stopPrice:'14',side:'SELL',closePosition:true}),order('BTCUSDT',{orderId:22,status:'PARTIALLY_FILLED'})]}]},
 {name:'orders-empty-filter',family:'reads',method:'fetchOrders',args:[[]],queue:[{data:[order('BTCUSDT')]}]},
 {name:'markets-select-btc-and-brackets',family:'reads',method:'fetchMarkets',queue:[{data:{symbols:[market('ETHUSDT'),market('BTCUSDT'),market('DEAD',{status:'BREAK'})]}},{data:[{symbol:'BTCUSDT',brackets:[{initialLeverage:25}]},{symbol:'ETHUSDT',brackets:[{initialLeverage:0}]}]}]},
 {name:'markets-bracket-failure-default',family:'reads',method:'fetchMarkets',version:'v1',queue:[{data:{symbols:[market('BTCUSDT')]}},{error:'brackets down'}]},
 {name:'markets-bracket-null-tail-keeps-partial',family:'reads',method:'fetchMarkets',queue:[{data:{symbols:[market('BTCUSDT')]}},{data:[{symbol:'BTCUSDT',brackets:[{initialLeverage:35}]},null]}]},
 {name:'markets-null-body-throws-before-brackets',family:'reads',method:'fetchMarkets',queue:[{data:null}]},
 {name:'markets-null-sample-throws',family:'reads',method:'fetchMarkets',queue:[{data:{symbols:[null,market('BTCUSDT')]}}]},
 {name:'orders-null-filter-throws',family:'reads',method:'fetchOrders',args:[['BTCUSDT']],queue:[{data:[null,order('BTCUSDT')]}]},
 {name:'tickers-bulk-only',family:'reads',method:'fetchTickers',queue:[{data:[ticker('BTCUSDT'),ticker('ETHUSDT')]},{data:[{symbol:'BTCUSDT',openInterest:'13'},{symbol:'BTCUSDT',openInterest:'14'},{symbol:'ETHUSDT',openInterest:'Infinity'}]}]},
 {name:'tickers-empty-bulk-no-fallback',family:'reads',method:'fetchTickers',queue:[{data:[ticker('BTCUSDT')]},{data:[]}]},
 {name:'read-http-failure',family:'reads',method:'fetchOrders',queue:[{error:'orders down'}]},
 {name:'sync-hedge-string',family:'configuration',method:'syncPositionMode',queue:[{data:{dualSidePosition:'true'}}]},
 {name:'sync-invalid-response',family:'configuration',method:'syncPositionMode',queue:[{data:{dualSidePosition:1}}]},
 {name:'sync-axios-error-name-boundary',family:'configuration',method:'syncPositionMode',queue:[{error:'mode API down',errorType:'axios'}]},
 {name:'leverage-round-up-clamp',family:'configuration',method:'setLeverage',args:['BTCUSDT',20.5,'long'],initial:{markets:[storeMarket('BTCUSDT')]},queue:[{data:{}}]},
 {name:'leverage-negative-half-min-fallback',family:'configuration',method:'setLeverage',args:['BTCUSDT',-1.5],initial:{markets:[storeMarket('BTCUSDT',{limits:{amount:{min:0,max:1},leverage:{min:0,max:20}}})]},queue:[{data:{}}]},
 {name:'leverage-negative-half-zero',family:'configuration',method:'setLeverage',args:['BTCUSDT',-0.5],initial:{markets:[storeMarket('BTCUSDT',{limits:{amount:{min:0,max:1},leverage:{min:-5,max:0}}})]},queue:[{data:{}}]},
 {name:'leverage-in-range-half',family:'configuration',method:'setLeverage',args:['BTCUSDT',2.5],initial:{markets:[storeMarket('BTCUSDT')]},queue:[{data:{}}]},
 {name:'leverage-market-missing',family:'configuration',method:'setLeverage',args:['MISSING',2],queue:[]},
 {name:'mode-change-refresh',family:'configuration',method:'changePositionMode',args:[true,'ignored'],initial:{markets:[storeMarket('BTCUSDT')]},queue:[{data:{}},{data:[position('BTCUSDT',{positionAmt:'2',positionSide:'SHORT'})]}]},
 {name:'mode-change-refresh-fails',family:'configuration',method:'changePositionMode',args:[false],initial:{options:{isHedged:true}},queue:[{data:{}},{error:'risk down'}]},
 {name:'mode-change-post-fails',family:'configuration',method:'changePositionMode',args:[true],queue:[{error:'mode down'}]},
 {name:'balance-current-applied',family:'balance',method:'reconcileBalanceAfterPrivateUpdate',args:[3],version:'v1',balanceVersion:3,queue:[{data:balance}]},
 {name:'balance-stale-after-request',family:'balance',method:'reconcileBalanceAfterPrivateUpdate',args:[3],balanceVersion:3,queue:[{data:balance,versionAfter:4}]},
 {name:'balance-disposed-after-request',family:'balance',method:'reconcileBalanceAfterPrivateUpdate',args:[3],balanceVersion:3,queue:[{data:balance,disposeAfter:true}]},
 {name:'balance-fetch-failure-is-reconciled',family:'balance',method:'reconcileBalanceAfterPrivateUpdate',args:[3],balanceVersion:3,queue:[{error:'balance down'}]},
 {name:'balance-subscriber-failure-is-reconciled',family:'balance',method:'reconcileBalanceAfterPrivateUpdate',args:[3],balanceVersion:3,subscriberError:'subscriber down',queue:[{data:balance}]},
];
(async()=>{const out=[];const dateNow=Date.now;try{Date.now=()=>1234;for(const c of cases){
 const store=new se.DefaultStore();store.update(structuredClone(c.initial||{}));let notifications=0;store.subscribe(()=>{notifications++;});if(c.subscriberError)store.subscribe(()=>{throw Error(c.subscriberError);});
 const source=new exported.AccountSource();source.store=store;source.apiVersion=c.version||'v3';source.balanceStateVersion=c.balanceVersion||0;source.dispose=be.BaseDispose.prototype.dispose;source.unsubscribeFromStore=()=>{};source.remoteSignerAvailabilityCleanups=new Set();source.stopWatchdog=()=>{};source.getAllSockets=()=>[];source.off=()=>{};
 const queue=structuredClone(c.queue),requests=[],logs=[];source.log=(message,severity)=>logs.push({message,severity});
 const api=kind=>({get:async(url,config)=>request(kind,'GET',url,undefined,config),post:async(url,body)=>request(kind,'POST',url,body)});
 const request=(kind,method,url,body,config)=>{requests.push({kind,method,url,...(body===undefined?{}:{body}),...(config||{})});const next=queue.shift();if(!next)throw Error('Unexpected request');if('versionAfter'in next)source.balanceStateVersion=next.versionAfter;if(next.disposeAfter)source.dispose();if(next.error)throw next.errorType==='axios'?new (loadPackage('axios').AxiosError)(next.error):Error(next.error);return {data:next.data};};
 source.primaryAPI=api('primary');source.publicAPI=api('public');let output,error;try{output=await source[c.method](...(c.args||[]));}catch(e){error={name:e.name,message:e.message};if('isAxiosError'in e)error.isAxiosError=e.isAxiosError;}
 out.push({...c,args:c.args||[],initial:c.initial||{},version:c.version||'v3',output:output===undefined?null:output,error:error||null,requests,logs,notifications,unconsumed:queue.length,state:{isHedged:store.options.isHedged,balance:structuredClone(store.balance),loaded:structuredClone(store.loaded),positions:structuredClone(store.positions),version:source.balanceStateVersion,lastAt:source.lastBalanceReconciliationAt}});
 }}finally{Date.now=dateNow;}process.stdout.write(JSON.stringify(out,(_,value)=>typeof value==='number'&&Object.is(value,-0)?{number:'-0'}:value));})().catch(e=>{process.stderr.write(e.stack);process.exitCode=1;});
