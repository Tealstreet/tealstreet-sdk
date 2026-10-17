const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
function compile(text,file){const r=ts.transpileModule(text,{fileName:file,reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}});assert(!r.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error));return r.outputText;}
const cache=new Map();
function load(file){if(cache.has(file))return cache.get(file);const m={exports:{}};const req=s=>s==='@tealstreet/math/src/isNotNumber'?load(path.join(root,'packages/math/src/isNotNumber.ts')):require(require.resolve(s,{paths:[path.join(root,'node_modules')]}));vm.runInThisContext(`(function(require,module,exports){${compile(fs.readFileSync(file,'utf8'),file)}\n})`,{filename:file})(req,m,m.exports);cache.set(file,m.exports);return m.exports;}
const math=load(path.join(root,'packages/safe-cex/src/utils/safe-math.ts')),keys=load(path.join(root,'packages/safe-cex/src/utils/get-key.ts'));
const file=path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.exchange.ts'),src=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const cls=src.statements.filter(s=>ts.isClassDeclaration(s)&&s.name?.text==='BingxExchange');assert.equal(cls.length,1);
const names=['validateAccount','fetchBalance','fetchMarkets','fetchTickersRaw','formatTickers','fetchTickers','fetchLeverageForSymbol','fetchMarginModeForSymbol','getOrFetchPositionMode'];
const members=[...names,'fetchPrimaryBalanceData','_parsePrecision','deriveTickerBidAsk','toBingxSymbol'].map(name=>{const found=cls[0].members.filter(m=>m.name?.getText(src)===name);assert.equal(found.length,1,name+' source census');return found[0].getText(src);});
if(process.env.SAFE_CEX_BINGX_AUDIT_SOURCE_STRING_HINT==='1') {
 const index=names.indexOf('validateAccount');
 const statement="return { error: err?.response?.data?.msg || err?.message || 'Invalid API key or secret' };";
 assert.equal(members[index].split(statement).length,2,'BINGX-AUDIT source control selector');
 members[index]=members[index].replace(statement,"return { error: String(err?.response?.data?.msg || err?.message || 'Invalid API key or secret') };");
}
const typesFile=path.join(root,'packages/safe-cex/src/types.ts'),types=ts.createSourceFile(typesFile,fs.readFileSync(typesFile,'utf8'),ts.ScriptTarget.Latest,true);
const enums=['MarginType','PositionMode'].map(name=>{const found=types.statements.filter(s=>ts.isEnumDeclaration(s)&&s.name.text===name);assert.equal(found.length,1);return found[0].getText(types).replace(/^export /,'');}).join('\n');
const endpointFile=path.join(root,'packages/safe-cex/src/exchanges/bingx/bingx.types.ts'),endpointSrc=ts.createSourceFile(endpointFile,fs.readFileSync(endpointFile,'utf8'),ts.ScriptTarget.Latest,true);
const endpoints=endpointSrc.statements.filter(ts.isVariableStatement).flatMap(s=>Array.from(s.declarationList.declarations)).filter(d=>d.name.getText(endpointSrc)==='ENDPOINTS');assert.equal(endpoints.length,1);
const Reads=vm.runInNewContext(compile(`${enums}\nconst ENDPOINTS=${endpoints[0].initializer.getText(endpointSrc)}; class Reads {${members.join('\n')}}; Reads;`,file),{v:keys.v,add:math.add,subtract:math.subtract},{filename:file});
const market={id:'BTC-USDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.25},limits:{amount:{min:0.1,max:999},leverage:{min:1,max:50}}};
const balance={total:91,free:37,used:54,upnl:8};
const rawMarket={symbol:'BTC-USDT',asset:'BTC',currency:'USDT',status:1,quantityPrecision:'2',pricePrecision:'4',tradeMinLimit:'0.125',maxLongLeverage:75};
const daily={symbol:'BTC-USDT',lastPrice:'12.25',priceChangePercent:'3.5',volume:'11.75',quoteVolume:'22.25'};
const price={symbol:'BTC-USDT',markPrice:'13.25',indexPrice:'14.75',lastFundingRate:'0.3',nextFundingTime:'12345'};
function fixtures(method,variant){
 let responses=[];let raw={dailys:[daily],prices:[price]};
 if(['validateAccount','fetchBalance'].includes(method))responses=[{code:0,data:[{asset:'USDT',balance:'13.5',availableMargin:'4.25',usedMargin:'9.25',unrealizedProfit:'-2.5'}]}];
 if(method==='fetchMarkets')responses=[{data:[rawMarket,{...rawMarket,symbol:'XAU-USD-USDT',asset:'XAUUSD',status:'1',quantityPrecision:'0',pricePrecision:'1',maxLongLeverage:0}]}];
 if(['fetchTickersRaw','fetchTickers'].includes(method))responses=[{data:raw.dailys},{data:raw.prices}];
 if(method==='fetchLeverageForSymbol')responses=[{code:0,data:{longLeverage:17,shortLeverage:23}}];
 if(method==='fetchMarginModeForSymbol')responses=[{code:0,data:{marginType:'ISOLATED'}}];
 if(method==='getOrFetchPositionMode')responses=[{code:0,data:{dualSidePosition:'true'}},{code:0,data:{dualSidePosition:true}}];
 let error=null;
 if(variant===1){
  if(['validateAccount','fetchBalance'].includes(method))responses[0].data[0]={asset:'USDT',balance:'bad',availableMargin:'bad',usedMargin:'',unrealizedProfit:'-0'};
  if(method==='fetchMarkets'){const row={...rawMarket,quantity_precision:'3',price_precision:'5'};delete row.quantityPrecision;delete row.pricePrecision;responses=[{data:[row]}];}
  if(['formatTickers','fetchTickersRaw','fetchTickers'].includes(method)){raw={dailys:[{...daily,symbol:'UNKNOWN'},daily,daily],prices:[price,{...price,markPrice:'99'}]};if(method!=='formatTickers')responses=[{data:raw.dailys},{data:raw.prices}];}
  if(method==='fetchLeverageForSymbol')responses=[{code:0,data:{longLeverage:0,shortLeverage:41}}];
  if(method==='fetchMarginModeForSymbol')responses=[{code:0,data:{marginType:'isolated'}}];
  if(method==='getOrFetchPositionMode')responses=[{code:0,data:{dualSidePosition:'false'}},{code:0,data:{dualSidePosition:'true'}}];
 }
 if(variant===2){
  if(method==='formatTickers')raw={dailys:[null],prices:[price]};
  else error={message:'network cause',response:{data:{msg:'wire denied'}}};
 }
 if(variant===3){
  if(method==='validateAccount')responses=[{code:1,msg:''}];
  if(method==='fetchBalance')responses=[{code:1,msg:'invalid'}];
  if(method==='fetchMarkets')responses=[{data:[]}];
  if(method==='formatTickers')raw={dailys:[daily],prices:[]};
  if(method==='fetchTickersRaw')responses=[{data:[daily]},null];
  if(method==='fetchTickers')responses=[{data:[daily]},{data:[price]}];
  if(method==='fetchLeverageForSymbol'||method==='fetchMarginModeForSymbol'||method==='getOrFetchPositionMode')responses=[{code:'0',msg:'strict code refusal'}];
 }
 return structuredClone({responses,raw,error,disposed:method==='fetchTickers'&&variant===3});
}
function observe(value){if(typeof value==='number'){if(Number.isNaN(value))return {number:'NaN'};const b=Buffer.alloc(8);b.writeDoubleBE(value);return {number:b.toString('hex')};}if(value===undefined)return {undefined:true};if(Array.isArray(value))return value.map(observe);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,observe(v)]));return value;}
(async()=>{const cases=[];for(const method of names)for(let variant=0;variant<4;variant++){
 const input=fixtures(method,variant),venue=new Reads(),calls=[],reports=[],updates=[];
 venue.store={balance,markets:[market],marketsById:{[market.id]:market},marketsBySymbol:{[market.symbol]:market},fetchedLeverage:{OLD:{buyLeverage:2,sellLeverage:3}},update(change){updates.push(change);Object.assign(this,change);}};
 venue.isDisposed=input.disposed;venue.emitAndLogError=(...args)=>reports.push(args);
 venue.xhr={get:async(...args)=>{calls.push(args);if(input.error){const e=new Error(input.error.message);e.response=input.error.response;throw e;}return {data:input.responses[calls.length-1]};}};
 let output;try{let result;if(method==='formatTickers')result=venue[method](input.raw);else if(method==='getOrFetchPositionMode'){const a=await venue[method](),b=await venue[method](),c=await venue[method](true);result=[a,b,c];}else if(method==='fetchLeverageForSymbol'||method==='fetchMarginModeForSymbol')result=await venue[method]('BTCUSDT');else result=await venue[method]();output={ok:observe(result)};}catch(e){output={error:{name:e.name,message:e.message}};}
 cases.push({method,variant,input,output,calls:observe(calls),reports:observe(reports),updates:observe(updates)});
 }
 const chosen=[];
 for(const method of ['fetchMarkets','fetchLeverageForSymbol']) {
  const input=fixtures(method,0),venue=new Reads();
  if(method==='fetchMarkets')input.responses[0].data[0].maxLongLeverage='75';else input.responses[0].data.longLeverage='17';
  venue.store={markets:[market],marketsBySymbol:{[market.symbol]:market},marketsById:{[market.id]:market},fetchedLeverage:{},update(change){Object.assign(this,change);}};
  venue.emitAndLogError=()=>assert.fail('source accepts dynamic numeric field');
  venue.xhr={get:async()=>({data:input.responses[0]})};
  const result=await venue[method](...(method==='fetchLeverageForSymbol'?['BTCUSDT']:[]));
  chosen.push({method,value:method==='fetchMarkets'?result[0].limits.leverage.max:result.buyLeverage});
 }
 const prefix=fixtures('fetchMarkets',0),prefixVenue=new Reads(),prefixReports=[];
 prefix.responses[0].data[0].maxLongLeverage='75';prefix.responses[0].data[1].symbol=null;
 prefixVenue.store={markets:[market]};prefixVenue.emitAndLogError=message=>prefixReports.push(message);
 prefixVenue.xhr={get:async()=>({data:prefix.responses[0]})};
 await prefixVenue.fetchMarkets();chosen.push({method:'market-source-prefix',reports:prefixReports});
 const assetRows=[];
 for(const field of ['asset','currency']) for(const value of [{asset:'wrong'},false,null,17,undefined]) {
  const bad={...rawMarket,symbol:'BAD-USDT',[field]:value};
  const venue=new Reads();
  venue.store={markets:[market]};
  venue.emitAndLogError=()=>assert.fail('source retains malformed asset');
  venue.xhr={get:async()=>({data:{data:[rawMarket,bad,{...rawMarket,symbol:'ETH-USDT',asset:'ETH'}]}})};
  const result=await venue.fetchMarkets();
  assert.equal(result.length,3,'source keeps malformed row and siblings');
  const outputField=field==='asset'?'base':'quote';
  assert.equal(result[1][outputField],value,'source retains raw asset identity');
  assetRows.push({field:outputField,type:value===null?'null':typeof value,result:observe(result)});
 }
 assert.equal(assetRows.length,10,'market asset source corpus');
 const leverageRows=[];
 for(const value of ['75',{leverage:75},[75],true]){
  const venue=new Reads();venue.store={markets:[market]};
  venue.emitAndLogError=()=>assert.fail('source retains raw leverage');
  venue.xhr={get:async()=>({data:{data:[rawMarket,{...rawMarket,symbol:'BAD-USDT',maxLongLeverage:value},{...rawMarket,symbol:'ETH-USDT',asset:'ETH'}]}})};
  const result=await venue.fetchMarkets();
  assert.equal(result.length,3,'source retains all market rows');
  assert.equal(result[1].limits.leverage.max,value,'source retains raw leverage identity');
  leverageRows.push(observe(result));
 }
 assert.equal(leverageRows.length,4,'raw market leverage source cases');
 const boundaryInput={code:0,data:[{asset:'USDT',balance:'1',userId:'a\ud800b'}]};
 const venue=new Reads();venue.store={balance};venue.xhr={get:async()=>({data:boundaryInput})};
 const boundary=(await venue.fetchBalance()).info.userId;
 const audit=[];
 for(const hint of [{reason:'denied'},['risk','limit'],17]) {
  const venue=new Reads(),reports=[];
  const input={error:{message:'network cause',response:{data:{msg:hint}}}};
  venue.emitAndLogError=(...args)=>reports.push(args);
  venue.xhr={get:async()=>{const error=new Error(input.error.message);error.response=input.error.response;throw error;}};
  const result=await venue.validateAccount();
  audit.push({input,hint,type:typeof result.error,sameHint:result.error===hint,result:observe(result.error),reports:observe(reports)});
 }
 assert.equal(audit.length,3,'BINGX-AUDIT independent source hint floor');
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},market,balance,names,cases,chosen,audit,assetRows,leverageRows,boundary:{raw:JSON.stringify(boundaryInput),units:Array.from({length:boundary.length},(_,i)=>boundary.charCodeAt(i))}}));
})().catch(e=>{console.error(e);process.exitCode=1;});
