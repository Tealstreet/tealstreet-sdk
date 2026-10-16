import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript'),root=new URL('../../../',import.meta.url),[family]=process.argv.slice(2);
const all=JSON.parse(fs.readFileSync(new URL('../tests/fixtures/rest-history.json',import.meta.url))),key=family==='page'?'income':family;
const floor={mapper:8,fills:12,orders:10,page:12,income:12,range:12,public_trades:10,seed:15};assert.ok(floor[family]);assert.equal(all.cases[key].length,floor[family]);
function tree(file){return ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true)}
function decl(t,name){const n=t.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(t)===name));assert.ok(n,name);return n.getText(t)}
const exchange=tree('packages/safe-cex/src/exchanges/binance/binance.exchange.ts'),types=tree('packages/safe-cex/src/exchanges/binance/binance.types.ts'),utils=tree('packages/safe-cex/src/exchanges/binance/binance.utils.ts'),common=tree('packages/safe-cex/src/types.ts'),severity=tree('packages/logging/src/LogSeverity.ts');
const method={fills:'fetchHistoricalFills',orders:'fetchHistoricalOrders',page:'_fetchIncomeHistoryPage',income:'fetchIncomeHistory',range:'fetchIncomeHistoryRange',public_trades:'fetchPublicTrades',seed:'listenOHLCV'}[family];const cls=exchange.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BinanceExchange');
const compile=code=>ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
const norm=v=>v===undefined?{$undefined:true}:typeof v==='number'?(Number.isNaN(v)?{$nan:true}:v===Infinity?{$infinity:true}:v===-Infinity?{$negativeInfinity:true}:Object.is(v,-0)?{$negativeZero:true}:v):Array.isArray(v)?v.map(norm):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,norm(v)])):v;
const results=[];
for(const input of all.cases[key]){const f=structuredClone(input),trace=[],notices=[],exports={},context=vm.createContext({exports,require});
 const enums=[...common.statements.filter(ts.isEnumDeclaration),...severity.statements.filter(ts.isEnumDeclaration)].map(n=>n.getText(n.getSourceFile())).join('\n');
 const body=family==='mapper'?decl(utils,'mapBinanceUserTrade'):`exports.Subject=class{${cls.members.find(n=>n.name?.getText(exchange)===method).getText(exchange)}};`;
 const constants=[decl(types,'ENDPOINTS'),decl(types,'ORDER_SIDE'),decl(utils,'USER_TRADES_MAX_WINDOW_MS'),...['INCOME_HISTORY_MAX_WINDOW_MS','INCOME_HISTORY_PAGE_LIMIT','INCOME_HISTORY_CHUNK_DELAY_MS'].map(n=>decl(exchange,n))].join('\n');
 vm.runInContext(compile(`${enums}\n${constants}\n${body}\nexports.error=()=>new RangeError('history fixture');exports.realLower=String.prototype.toLocaleLowerCase;`),context);
 const error=exports.error();error.code='history-code';error.response={status:503,data:{msg:'venue detail'}};if(f.name==='empty-error')error.message='';
 const errorOut=e=>e?{original:e===error,name:e.name,message:e.message,code:norm(e.code),response:norm(e.response)}:null;
 function load(file){const ex={};vm.runInContext(`(function(exports,require){${compile(fs.readFileSync(new URL(file,root),'utf8'))}})(__helperExports,require)`,Object.assign(context,{__helperExports:ex}));return ex}
 const enumValues=exports;context.require=name=>name.endsWith('/types')?enumValues:require(name);
 const realSplit=load('packages/safe-cex/src/utils/splitTimeRange.ts').splitTimeRange,realNormalize=load('packages/safe-cex/src/utils/normalizeExecutionPositionSide.ts').normalizeExecutionPositionSide,realCategory=load('packages/safe-cex/src/utils/incomeCategoryFromType.ts').incomeCategoryFromType,realOrder=load('packages/safe-cex/src/exchanges/binance/binance.order-history.ts').mapBinanceHistoricalOrder,realOmit=load('packages/safe-cex/src/utils/omitUndefined.ts').omitUndefined;
 context.omitUndefined=realOmit;
 const entry=structuredClone(all.entry);if(f.name==='numeric-prefix')entry.price='12.3tail';if(f.name==='nullable-ids'){entry.id=null;entry.orderId=null;entry.quoteQty=null;}if(f.name==='missing')for(const key of ['id','orderId','quoteQty','realizedPnl','commission','time','positionSide','side'])delete entry[key];if(f.name==='unknown-labels'){entry.positionSide='MANUAL';entry.side='CUSTOM';}
 context.lowercase=v=>{trace.push(['lower',v]);if(f.name==='locale-fail')throw error;if(f.name==='locale-mutates'){entry.commission='0.2';entry.time=2000;}return exports.realLower.call(v)};
 vm.runInContext('String.prototype.toLocaleLowerCase=function(){return lowercase(String(this))}',context);
 if(family==='mapper'){let value,failure;try{value=exports.mapBinanceUserTrade(f.name==='null-entry'?null:entry)}catch(e){failure=e}results.push({name:f.name,trace,value:norm(value),failure:errorOut(failure)});continue;}
 const subject=new exports.Subject();let gets=0,categories=0,maps=0;let payload=['page','income'].includes(family)?structuredClone(all.income):family==='orders'?structuredClone(all.orders):family==='public_trades'?[{...structuredClone(all.entry),isBuyerMaker:true}]:[structuredClone(all.entry)];
 if(f.name==='non-array')payload={code:-1000,msg:'non array'};if(f.name==='null-row')payload=[null];if(f.name==='null-id')payload[0].id=null;if(f.name==='noncall-id')payload[0].id={toString:0};
 Object.defineProperty(subject,'isPublicDataOnly',{get(){trace.push(['public_only']);return f.name==='public-only'}});
 const log=()=>{trace.push(['capture_log']);if(f.name==='capture-log-fail')throw error;return(message,severity)=>{trace.push(['log']);notices.push({method:'log',message:norm(message),severity});if(f.name==='log-fail')throw error}};
 Object.defineProperty(subject,'log',{get:log});Object.defineProperty(subject,'emitAndLogError',{get(){trace.push(['capture_emit']);return message=>{trace.push(['emit']);notices.push({method:'emit',message:norm(message)});if(f.name==='emit-fail')throw error}}});
 context.splitTimeRange=(from,to,max)=>{trace.push(['split',norm(from),norm(to),max]);return realSplit(from,to,max)};
 context.normalizeExecutionPositionSide=v=>{trace.push(['normalize',norm(v)]);return realNormalize(v)};
 context.incomeCategoryFromType=v=>{trace.push(['category',norm(v)]);categories++;if(f.name==='category-fail')throw error;if(f.name==='category-appends'&&categories===1)payload.push({...all.income[0],tranId:'appended'});return realCategory(v)};
 context.mapBinanceHistoricalOrder=v=>{trace.push(['map_order']);maps++;if(f.name==='map-fail')throw error;return realOrder(v)};
 const get=client=>{trace.push(['capture_get',client]);return(ep,{params})=>{gets++;trace.push(['get',client,ep,norm(params)]);if(f.name==='get-sync-fail')throw error;return Promise.resolve().then(()=>{trace.push(['resolve']);if(['get-reject','log-fail','emit-fail'].includes(f.name))throw error;return {data:payload}})}};
 subject.xhr={};subject.publicXhr={};Object.defineProperty(subject.xhr,'get',{get:()=>get('private')});Object.defineProperty(subject.publicXhr,'get',{get:()=>get('public')});
 let opts={symbol:'BTCUSDT',from:0,to:1000,orderId:'order-A',limit:3.6};if(['page','income'].includes(family))opts={symbol:'BTCUSDT',incomeType:'FUNDING_FEE',startTime:0,endTime:1000,limit:3.6};
 if(f.name==='missing-symbol')delete opts.symbol;if(f.name==='null-opts')opts=null;if(f.name==='undefined-opts')opts=undefined;if(f.name==='limit-null')opts.limit=null;if(f.name==='limit-zero')opts.limit=0;if(f.name==='limit-string')opts.limit='2.5';if(f.name==='zero-fields')opts={symbol:'',incomeType:'',startTime:0,endTime:0,limit:0};if(f.name==='split-range'){opts.from=1;opts.to=604800003;}if(f.name==='string-range'){opts.from='1';opts.to=1000;}
 let rangeRecords=[{sentinel:12.3}],rangeOptions,pageArgs,pageVersion=1,pageInput,seedParams,seedOpts,callbacks=[],cleanups=0,callbackRefs=[],seedJobs=[],jobFailures=[];
 if(family==='range'){
  Object.defineProperty(subject,'_fetchIncomeHistoryPage',{get(){trace.push(['page_resolve',pageVersion]);if(f.name==='resolve-mutates')pageInput.startTime=2;const receiver=pageVersion;return opts=>{pageArgs=opts;trace.push(['page_call',receiver,norm(opts)]);return Promise.resolve().then(()=>{trace.push(['page_settle']);if(f.name==='page-reject')throw error;return []})}}});
  context.paginateIncomeRange=(fetch,options)=>{rangeOptions=options;trace.push(['paginate',norm(options)]);return Promise.resolve().then(async()=>{trace.push(['pager_settle']);if(f.name==='pager-reject')throw error;if(f.name==='current-page')pageVersion=2;pageInput=f.name==='null-page-arg'?null:{startTime:1,endTime:999,limit:7};await fetch(pageInput);if(f.name==='null-result')return null;return {records:rangeRecords,truncated:f.name!=='normal'&&f.name!=='current-page'&&f.name!=='resolve-mutates',exhaustedChunks:2}})};
  let isos=0;context.Date=function(value){trace.push(['iso',norm(value)]);isos++;if(f.name===`iso${isos}-fail`)throw error;return {toISOString:()=>new Date(value).toISOString()}};
 }
 if(family==='seed'){
  seedOpts={symbol:'BTCUSDT',interval:'1m',limit:5,nested:{value:3.6}};const candleList=f.name==='empty'?[]:f.name==='null-candles'?null:structuredClone(all.candles);
  Object.defineProperty(subject,'externalPublicDataProvider',{get(){trace.push(['external']);return f.name==='external'?{subscribeCandle(o,c){trace.push(['external_listen']);assert.equal(o,seedOpts);assert.equal(c,callback);return cleanup}}:null}});
  Object.defineProperty(subject,'isDisposed',{get(){trace.push(['owner_disposed']);return f.name==='owner-disposed'}});
  const callback=value=>{trace.push(['callback']);callbacks.push(norm(value));callbackRefs.push(value===candleList?.[candleList.length-1]);if(f.name==='callback-fail')throw error};const cleanup=()=>{trace.push(['unsubscribe']);cleanups++;if(f.name==='unsubscribe-fail')throw error};
  subject.publicWebsocket={};Object.defineProperty(subject.publicWebsocket,'listenOHLCV',{get(){trace.push(['capture_public']);return(o,c)=>{trace.push(['public_listen']);assert.equal(o,seedOpts);assert.equal(c,callback);return cleanup}}});
  Object.defineProperty(subject,'fetchOHLCV',{get(){trace.push(['capture_fetch']);return opts=>{seedParams=opts;trace.push(['seed_fetch',norm(opts)]);if(f.name==='fetch-sync-fail')throw error;const p=Promise.resolve().then(()=>{trace.push(['seed_settle']);if(['fetch-reject','empty-error','capture-log-fail','log-fail','cleanup-reject'].includes(f.name))throw error;return candleList});const originalThen=p.then.bind(p);p.then=fn=>{const next=originalThen(fn),originalCatch=next.catch.bind(next);next.catch=fn=>{const job=originalCatch(fn);job.catch(()=>{});seedJobs.push(job);return job};return next};return p}}});
  let value,failure;try{value=subject.listenOHLCV(seedOpts,callback);if(['cleanup-first','unsubscribe-fail','repeat-cleanup','cleanup-reject'].includes(f.name)){value();if(f.name==='repeat-cleanup')value();}}catch(e){failure=e}
  await Promise.all(seedJobs.map(job=>job.catch(e=>jobFailures.push(errorOut(e)))));
  results.push({name:f.name,trace,notices,callbackRefs,callbacks,cleanups,params:norm(seedParams),paramsRefs:seedParams?{root:seedParams===seedOpts,nested:seedParams.nested===seedOpts.nested}:null,cleanup:typeof value==='function',jobs:seedJobs.length,jobFailures,failure:errorOut(failure)});continue;
 }
 let value,failure;try{value=family==='range'?await subject[method](0,1000,undefined):family==='public_trades'?await subject[method]('BTCUSDT',opts.limit):await subject[method](opts)}catch(e){failure=e}
 results.push({name:f.name,trace,notices,gets,categories,maps,value:norm(value),rangeOptions:norm(rangeOptions),pageArgs:norm(pageArgs),rangeRecordsRef:family==='range'&&value!==undefined?value===rangeRecords:null,failure:errorOut(failure)});
}
process.stdout.write(JSON.stringify(results));
