const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function ast(p){return ts.createSourceFile(p,fs.readFileSync(path.join(root,p),'utf8'),99,true)}
function decl(a,names){const all=a.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations));return names.map(n=>{const d=all.filter(d=>d.name.getText(a)===n);assert.equal(d.length,1,n);return 'const '+d[0].getText(a)+';'}).join('\n')}
const ea=ast('packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts'),ia=ast('packages/safe-cex/src/exchanges/bybit/bybit.income.ts'),ta=ast('packages/safe-cex/src/exchanges/bybit/bybit.types.ts'),la=ast('packages/logging/src/LogSeverity.ts');
const names=['fetchClosedPositionRows','fetchHistoricalPositions','fetchClosedPnl','fetchHistoricalOrders','fetchIncomeHistory','fetchIncomeHistoryRange','incomeContext'];
const cls=ea.statements.find(s=>ts.isClassDeclaration(s)&&s.name.text==='BybitExchange');assert(cls);
const body=names.map(n=>{const hits=cls.members.filter(m=>m.name?.getText(ea)===n);assert.equal(hits.length,1,n);return hits[0].getText(ea)}).join('\n');
const ctx=ia.statements.filter(s=>ts.isInterfaceDeclaration(s)&&s.name.text==='BybitIncomeContext');assert.equal(ctx.length,1);assert.deepEqual(ctx[0].members.map(m=>m.name.getText(ia)),['xhr','log','category','accountType']);
const severity=la.statements.find(s=>ts.isEnumDeclaration(s)&&s.name.text==='LogSeverity').getText(la).replace(/^export\s+/,'');
const split=decl(ast('packages/safe-cex/src/utils/splitTimeRange.ts'),['splitTimeRange']);
const code=ts.transpileModule(`${severity}\n${decl(ta,['ENDPOINTS','ORDER_HISTORY_MAX_WINDOW_MS'])}\n${decl(ea,['MAX_CLOSED_PNL_PAGES'])}\n${split}\n${decl(ia,['fetchBybitIncomeHistory','fetchBybitIncomeHistoryRange'])}\nclass Subject{${body}}globalThis.api=new Subject();globalThis.history={fetchBybitIncomeHistory,fetchBybitIncomeHistoryRange}`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));const bits=n=>{const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'nan':b.toString('hex')};
function atom(v,refs,ignore=false){if(v===undefined)return{u:true};if(v===null)return null;if(typeof v==='number')return{n:bits(v)};if(typeof v==='string')return{s:units(v)};if(typeof v==='boolean')return v;if(!ignore&&refs.has(v))return{ref:refs.get(v)};if(v instanceof Error||Object.prototype.toString.call(v)==='[object Error]')return{error:true,name:units(v.name),message:units(v.message)};if(typeof v==='function')return{fn:v.ledgerCode??'unknown'};if(Array.isArray(v))return{a:Array.from(v,x=>atom(x,refs,ignore))};return{o:Object.entries(v).map(([k,v])=>[units(k),atom(v,refs,ignore)])}}
const from=v=>v&&v.fixtureUndefined?undefined:v&&v.fixtureNan?NaN:v&&v.fixtureNegZero?-0:Array.isArray(v)?v.map(from):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,from(v)])):v;
async function run(input){const events=[],refs=new WeakMap(),started=[];let api,index=0;
 const ref=(v,name)=>{if(v&&(typeof v==='object'||typeof v==='function'))refs.set(v,name);return v};
 const primary=ref(new Error('primary-\ud800'),'primary');function rec(name,args=[]){events.push({name,args:atom(args,refs)});if(input.fail===name)throw primary}
 const opts=from(Object.hasOwn(input,'opts')?input.opts:{symbol:'BTC',from:10,to:25,account:'acct',accountName:'name'});ref(opts,'opts');
 if(input.observeOpts&&opts&&typeof opts==='object')for(const key of ['symbol','from','to','limit','incomeType','startTime','endTime','account','accountName']){let v=opts[key];Object.defineProperty(opts,key,{get(){rec('opts:'+key);return v},enumerable:true})}
 const closed=ref(from(input.closedRows??[{orderId:'O1',symbol:'BTC',closedTime:21.5},{orderId:'O2',symbol:'ETH',closedTime:13.25}]),'closed');closed.forEach((r,i)=>ref(r,'closed'+i));
 const orders=ref(from(input.orderRows??[{id:'O1',positionSide:'long'},{id:'O2',positionSide:'short'}]),'orders');
 const log=(...args)=>rec('log',args);log.ledgerCode='log';ref(log,'log');
 const get=(endpoint,{params})=>{const n=index++;rec('get'+n,[endpoint,params]);const p=(async()=>{for(let j=0;j<(input.delay??1);j++)await Promise.resolve();rec('resolved:get'+n);if(input.failAsync==='get'+n)throw primary;
  const pages=input.pages??[{retMsg:'OK',retCode:0,result:{list:[{id:'A',symbol:'BTC',closedTime:21.5,createdTime:17.25,timestamp:13.125,incomes:[5.5,-2.25]}],nextPageCursor:''}}];let data=from(pages[Math.min(n,pages.length-1)]);if(Array.isArray(data?.result?.list))data.result.list.forEach((r,i)=>ref(r,'row'+n+'.'+i));return{data};})();started.push(p);p.catch(()=>{});return p};get.ledgerCode='get';
 const xhr=ref({get},'xhr'),xhr2=ref({get},'xhr2');
 const ctx=ref({xhr,log,category:input.category??'linear',accountType:input.accountType??'UNIFIED'},'ctx');
 if(input.observeCtx)for(const key of ['accountType','category','xhr','log']){let v=ctx[key];Object.defineProperty(ctx,key,{get(){rec('ctx:'+key);return v},enumerable:true})}
 const globals={virtualClock:{getCurrentTimeMs(){rec('clock');return input.time??1000}},omitUndefined:obj=>Object.fromEntries(Object.entries(obj).filter(([,v])=>v!==undefined)),sleep:ms=>{rec('sleep',[ms]);const p=(async()=>{await Promise.resolve();rec('resolved:sleep');if(input.failAsync==='sleep')throw primary})();started.push(p);p.catch(()=>{});return p},Error:class Created extends Error{constructor(message){rec('error',[message]);super(message)}},
 mapBybitClosedPnl:(row,account)=>{rec('mapClosed',[row,account]);if(row.skip)return undefined;return{closedTime:row.closedTime??0,account,info:row}},
 mapBybitHistoricalOrder:row=>{rec('mapOrder',[row]);if(row.skip)return undefined;return{id:row.id,createdTime:row.createdTime??0,positionSide:row.positionSide,info:row}},
 mapBybitTransactionToIncomeRecords:row=>{rec('mapIncome',[row]);return(row.incomes??[]).map((income,i)=>({timestamp:row.timestamp??0,income,term:i,info:row}))},
 aggregateBybitClosedPnl:(rows,options)=>{options.resolveSettleCurrency.ledgerCode='history_quote';if(options.resolveLeg)options.resolveLeg.ledgerCode='history_leg';rec('aggregate',[rows,options]);return rows.map(row=>({account:options.account,symbol:row.symbol,quote:options.resolveSettleCurrency(row.symbol),leg:options.resolveLeg?.(row.orderId),info:row}))}};
 vm.runInNewContext(code,globals);api=globals.api;
 const originals=Object.fromEntries(names.filter(n=>n!=='incomeContext').map(n=>[n,api[n]]));
 Object.assign(api,{isPublicDataOnly:!!input.publicOnly,DEFAULT_EXECUTIONS_LOOKBACK_MS:100,xhr,log,store:{marketsBySymbol:{BTC:{quote:'USDC'},ETH:{quote:''}}},emitAndLogError:(...a)=>rec('emit',a),getCategoryForSymbol:s=>{rec('category',[s]);return 'category.'+s},getExecutionQueryWindows:(f,t)=>{rec('windows',[f,t]);return from(input.windows??[{startTime:f,endTime:t}])}});
 Object.defineProperty(api,'xhr',{get(){rec('xhrGet');return input.swapXhrOnLog&&api.changed?xhr2:xhr},set(v){},enumerable:true});
 Object.defineProperty(api,'log',{get(){rec('logGet');if(input.swapXhrOnLog)api.changed=true;return log},enumerable:true});
 api.fetchClosedPositionRows=(...args)=>{rec('closedDependency',args);const p=(async()=>{await Promise.resolve();rec('resolved:closedDependency');if(input.failAsync==='closedDependency')throw primary;return closed})();started.push(p);p.catch(()=>{});return p};
 api.fetchHistoricalOrders=(...args)=>{rec('ordersDependency',args);const p=(async()=>{await Promise.resolve();rec('resolved:ordersDependency');if(input.failAsync==='ordersDependency')throw primary;if(input.swapMarkets)api.store.marketsBySymbol.BTC.quote='DAI';return orders})();started.push(p);p.catch(()=>{});return p};
 // Preserve original selected arrows before installing dependencies for the leg-join body.

 let returned,result,error,before;try{
  if(input.op==='raw')returned=originals.fetchClosedPositionRows.call(api,opts);
  else if(input.op==='positions')returned=originals.fetchHistoricalPositions.call(api,opts);
  else if(input.op==='closed')returned=originals.fetchClosedPnl.call(api,opts);
  else if(input.op==='orders')returned=originals.fetchHistoricalOrders.call(api,opts);
  else if(input.op==='context')returned=Object.getOwnPropertyDescriptor(api.constructor.prototype,'incomeContext').get.call(api);
  else if(input.op==='wrapperIncome')returned=originals.fetchIncomeHistory.call(api,opts);
  else if(input.op==='wrapperRange')returned=originals.fetchIncomeHistoryRange.call(api,from(input.start??0),from(input.end??20),from(input.incomeType));
  else if(input.op==='income')returned=globals.history.fetchBybitIncomeHistory(ctx,opts);
  else if(input.op==='range')returned=globals.history.fetchBybitIncomeHistoryRange(ctx,from(input.start??0),from(input.end??20),from(input.incomeType));
  before=events.slice();result=await returned;
 }catch(e){error=e}
 for(let i=0;i<128;i++)await Promise.resolve();await Promise.allSettled(started);
 return{input,before:before??events.slice(),events,result:atom(result,refs,true),resultRef:atom(result,refs),error:error?atom(error,refs,true):null,primary:error===primary};
}
const page=(list,cursor='')=>({retMsg:'OK',retCode:0,result:{list,nextPageCursor:cursor}});
const row={id:'A',symbol:'BTC',closedTime:21.5,createdTime:17.25,timestamp:13.125,incomes:[5.5,-2.25]};
const groups={
 raw:[{}, {publicOnly:true},{opts:{symbol:'ETH'}},{pages:[page([row],'NEXT'),page([{...row,id:'B'}])]},{windows:[{startTime:0,endTime:5},{startTime:6,endTime:10}]},{pages:[{retMsg:'bad',retCode:'1'}]},{pages:[{retMsg:'bad',retCode:'0',result:{list:[row]}}]},{pages:[{retMsg:'OK',retCode:1,result:{list:[]}}]},{pages:[page([row],'LOOP')]},{failAsync:'get0'},{pages:[null]},{observeOpts:true}],
 positions:[{}, {opts:{account:'x'}},{orderRows:[]},{orderRows:[null,{id:'O1',positionSide:'long'},{id:'O1',positionSide:'short'},{id:'',positionSide:'long'}]},{failAsync:'closedDependency'},{failAsync:'ordersDependency'},{fail:'aggregate'},{fail:'ordersDependency'},{swapMarkets:true},{observeOpts:true}],
 closed:[{}, {publicOnly:true},{pages:[page([{...row,skip:true},{...row,closedTime:3},{...row,closedTime:99}])]},{pages:[page([row],'N'),{retMsg:'bad',retCode:1}]},{pages:[page([row],'LOOP')]},{windows:[{startTime:0,endTime:5},{startTime:6,endTime:10}]},{failAsync:'get0'},{fail:'mapClosed'},{fail:'emit',failAsync:'get0'},{fail:'log',pages:[{retMsg:'bad',retCode:1}]},{pages:[null]},{observeOpts:true},{opts:null}],
 orders:[{}, {publicOnly:true},{opts:{fixtureUndefined:true}},{opts:{from:0,to:604800005,limit:1}},{pages:[page([row],'N'),page([{...row,createdTime:99},{...row,id:'B',createdTime:2}])]},{pages:[page([{...row,skip:true}])]},{pages:[page([], 'N')]},{pages:[{retMsg:'bad',retCode:'1'}]},{pages:[{retMsg:'bad',retCode:'0',result:{list:[row]}}]},{pages:[page([row],'LOOP')]},{failAsync:'get0'},{fail:'mapOrder'},{fail:'emit',failAsync:'get0'},{observeOpts:true}],
 context:[{}, {fail:'xhrGet'},{fail:'logGet'},{swapXhrOnLog:true}],
 wrapperIncome:[{}, {opts:{limit:1,incomeType:'COMMISSION'}},{failAsync:'get0'},{fail:'xhrGet'}],
 wrapperRange:[{}, {start:0,end:604800005},{failAsync:'sleep'},{fail:'logGet'}],
 income:[{}, {opts:{limit:1,incomeType:'FUNDING_FEE'}},{opts:{limit:100,incomeType:'UNKNOWN',startTime:0,endTime:604800005}},{opts:{limit:0}},{opts:{limit:{fixtureNan:true}}},{pages:[page([row],'LOOP')]},{pages:[page([], 'N')]},{pages:[{retMsg:'bad',retCode:1}]},{pages:[{retMsg:'bad',retCode:'0',result:{list:[row]}}]},{failAsync:'get0'},{fail:'mapIncome'},{fail:'log',failAsync:'get0'},{pages:[null]},{observeCtx:true,observeOpts:true},{opts:{limit:'bad',startTime:0,endTime:604800005},pages:[page([row],'LOOP')]},{opts:{incomeType:'toString'}}],
 range:[{}, {start:0,end:0},{start:10,end:5},{start:0,end:604800005},{start:0,end:1209600002},{start:{fixtureNan:true},end:20},{failAsync:'sleep'},{failAsync:'get0'},{fail:'log',failAsync:'get0'},{incomeType:'COMMISSION'}],
};
assert.equal(Object.values(groups).reduce((n,g)=>n+g.length,0),87,'LEDGER independent scenario floor');
(async()=>{const rows=[];for(const[op,inputs]of Object.entries(groups))for(const input of inputs)rows.push(await run({op,...input}));process.stdout.write(JSON.stringify(rows))})().catch(e=>{console.error(e);process.exitCode=1});
