'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH; assert(compiler && path.isAbsolute(compiler),'absolute compiler required');
const ts=require(compiler),root=path.resolve(__dirname,'../../../packages/safe-cex/src');
function ast(file){return ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);}
const tree=ast('exchanges/grvt/grvt.exchange.ts'),cls=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='GrvtExchange');
const names=['fetchOHLCV','convertInterval','fetchOrderBook','updateMarketLeverageLimits','fetchLeverageForSymbol','setLeverage'];
const members=names.map(name=>{const n=cls.members.find(n=>n.name?.getText(tree)===name);assert(n,'actual '+name);return n.getText(tree);});
const types=ast('exchanges/grvt/grvt.types.ts'),interval=types.statements.find(n=>ts.isEnumDeclaration(n)&&n.name.text==='GrvtCandlestickInterval');
const code=[interval.getText(types),'const ENDPOINTS={KLINES:"/lite/v1/kline",ORDERBOOK:"/lite/v1/book"};','const LogSeverity={Debug:"debug",Info:"info",Warning:"warning",Error:"error"};','export class Venue {',...members,'}'].join('\n');
const compiled=ts.transpileModule(code,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(compiled.diagnostics.length,0);
const context={exports:{}};vm.runInNewContext(compiled.outputText,context);const Venue=context.exports.Venue;
function tag(value){if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {bits:b.toString('hex'),nan:Number.isNaN(value)};}if(value===undefined)return {undefined:true};if(Array.isArray(value))return value.map(tag);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,tag(v)]));return value;}
function market(id='btc_perp',symbol='BTC_USDT_PERP'){return {id,symbol,base:'BTC',quote:'USDT',active:true,precision:{amount:0.01,price:0.2},limits:{amount:{min:0.01,max:999},leverage:{min:1,max:25}},info:{ih:'0x11',bd:3,qd:6},isPerpetual:true};}
function position(id='p1'){return {id,symbol:'BTC_USDT_PERP',side:'long',entryPrice:7,notional:8,leverage:7,unrealizedPnl:0,contracts:2,liquidationPrice:1};}
function make(name,effects){
 const venue=new Venue();venue.defaultLeverage=10;venue.grvtTradingAccountId=name==='no-account'?'':'sub-72';
 const s={markets:[market()],positions:[position(),position('p2')],fetchedLeverage:{OLD:{buyLeverage:7,sellLeverage:8}}};
 Object.defineProperty(s,'marketsBySymbol',{get(){return Object.fromEntries(s.markets.map(m=>[m.symbol,m]));}});
 let updates=0,logs=0;const snapshots=[];
 s.update=patch=>{s.fetchedLeverage=patch.fetchedLeverage;effects.push(['cache']);snapshots.push(tag(s.fetchedLeverage));updates++;if(name==='cache-error'&&updates===1)throw Error('cache-fail');};
 s.updatePosition=(p,patch)=>{Object.assign(p,patch);effects.push(['position',p.id]);};
 venue.store=s;venue.log=(text,severity)=>{effects.push(['log',text,severity]);logs++;if(name==='first-log-error'&&logs===1||name==='success-log-error'&&severity==='info'||name==='catch-log-error'&&severity==='error')throw Error('log-fail');};
 venue.grvtClient=name==='no-client'?null:{authenticatedEndpoint:async()=>{effects.push(['endpoint']);if(name==='endpoint-error')throw Error('endpoint-fail');return {token:'edge-31'};},tdgClient:{}};
 return {venue,store:s,snapshots};
}
(async()=>{
 const candleRows=[],bookRows=[],limitRows=[],fetchRows=[],setRows=[];
 const publicNames=['missing-market','inherited','missing-data','empty','full','lite','numeric','defaults','non-array','null-row','request-error','first-log-error','success-log-error','catch-log-error','zero-options','nonfinite-options','nan'];
 for(const method of ['fetchOHLCV','fetchOrderBook'])for(const name of publicNames){
  const effects=[],{venue,store}=make(name,effects);const symbol=name==='missing-market'?'MISSING':name==='inherited'?'constructor':'BTC_USDT_PERP';
  let raw=method==='fetchOHLCV'?'{"r":[{"open_time":"1700000000123456789","open":"2.5","high":"7","low":"1","close":"4.25","volume_b":"13"},{"ot":"1700000060123456789","o":"3","h":"8","l":"2","c":"5","vb":"17"}]}':'{"r":{"bids":[{"price":"7","size":"10000000000000000"},{"price":"6","size":"-10000000000000000"},{"price":"5","size":"1"}],"asks":[{"price":"11","size":"2.5"},{"price":"12","size":"3.25"}]}}';
  if(name==='missing-data')raw='{}';
  if(name==='empty')raw=method==='fetchOHLCV'?'{"r":[]}':'{"r":{}}';
  if(name==='lite')raw=method==='fetchOHLCV'?'{"r":[{"ot":"2000000000","o":"2","h":"3","l":"1","c":"2.5","vb":"9"}]}':'{"r":{"bids":[{"price":"1","size":"3"}],"asks":[]}}';
  if(name==='numeric')raw=method==='fetchOHLCV'?'{"r":[{"ot":123000,"o":"2","h":"3","l":"1","c":"2","vb":"4"}]}':'{"r":{"bids":[{"price":7.5,"size":2}],"asks":false}}';
  if(name==='defaults')raw=method==='fetchOHLCV'?'{"r":[{"ot":0,"open_time":2000,"o":0,"open":"7","vb":0,"volume_b":0,"volume":"8"}]}':'{"r":{"bids":[{},0],"asks":null}}';
  if(name==='non-array'||name==='catch-log-error')raw=method==='fetchOHLCV'?'{"r":{}}':'{"r":{"bids":{}}}';
  if(name==='null-row')raw=method==='fetchOHLCV'?'{"r":[null]}':'{"r":{"bids":[null]}}';
  if(name==='nan')raw=method==='fetchOHLCV'?'{"r":[{"ot":-0,"o":"bad","h":"Infinity","l":"-0","c":"-Infinity"}]}':'{"r":{"bids":[{"price":"bad","size":"-0"}],"asks":[{"price":"Infinity","size":"1"}]}}';
  const requests=[];venue.xhr={post:async(endpoint,body)=>{requests.push({endpoint,body:tag(body)});effects.push(['post',endpoint]);if(name==='request-error')throw Error('post-fail');return {data:JSON.parse(raw)};}};
  const opts={symbol,interval:'5m',from:11,to:23,limit:17};if(name==='zero-options'){opts.from=0;opts.to=-0;opts.limit=0;}if(name==='nonfinite-options'){opts.from=Infinity;opts.to=-Infinity;opts.limit=Infinity;}
  const pending=method==='fetchOHLCV'?venue.fetchOHLCV(opts):venue.fetchOrderBook(symbol),eager=effects.slice();let result,caught;
  try{result=await pending;}catch(error){caught={name:error.name,message:error.message};}
  (method==='fetchOHLCV'?candleRows:bookRows).push({name,symbol,raw,requests,result:tag(result),error:caught??null,effects,eager});
 }
 const leverNames=['no-client','no-account','missing-market','inherited','falsy-match','missing-data','empty','full','lite','priority-empty','unmatched','non-array','null-row','null-response','endpoint-error','sdk-error','first-log-error','cache-error','nan'];
 for(const method of ['updateMarketLeverageLimits','fetchLeverageForSymbol','setLeverage'])for(const name of leverNames){
  const effects=[],{venue,store,snapshots}=make(name,effects);const symbol=name==='missing-market'?'MISSING':name==='inherited'||name==='falsy-match'?'constructor':'BTC_USDT_PERP';
  let raw=method==='setLeverage'?'{"success":true}':'{"results":[{"instrument":"btc_perp","max_leverage":"18","leverage":"5.5"},{"instrument":"btc_perp","max_leverage":"21","leverage":"6.5"}]}';
  if(name==='missing-data')raw='{}';if(name==='empty'||name==='priority-empty')raw='{"results":[],"r":[{"i":"btc_perp","l":"8"}]}';
  if(name==='lite')raw=method==='setLeverage'?'{"success":false,"s":true}':'{"r":[{"i":"btc_perp","ml1":"12.75","l":"3.25"}]}';
  if(name==='unmatched')raw=method==='setLeverage'?'{"error":{"message":"full-fail"},"e":{"m":"lite-fail"}}':'{"results":[{"instrument":"other","max_leverage":"17","leverage":"6"}]}';
  if(name==='non-array')raw='{"results":{}}';if(name==='null-row')raw='{"results":[null]}';if(name==='null-response')raw='null';
  if(name==='falsy-match')raw='{"results":[false]}';if(name==='nan')raw=method==='setLeverage'?'{"s":true}':'{"r":[{"i":"btc_perp","l":"bad","ml1":"Infinity"}]}';
  const payloads=[];const call=async(payload,endpoint)=>{payloads.push({payload:tag(payload),endpoint:tag(endpoint)});effects.push(['sdk']);if(name==='sdk-error')throw Error('sdk-fail');return JSON.parse(raw);};
  if(venue.grvtClient){venue.grvtClient.tdgClient.getAllInitialLeverage=call;venue.grvtClient.tdgClient.setInitialLeverage=call;}
  const original=store.markets;const input=name==='nan'?NaN:name==='lite'?-100:37;
  const pending=method==='updateMarketLeverageLimits'?venue.updateMarketLeverageLimits(original):method==='fetchLeverageForSymbol'?venue.fetchLeverageForSymbol(symbol):venue.setLeverage(symbol,input),eager=effects.slice();let result,caught;
  try{result=await pending;}catch(error){caught={name:error.name,message:error.message};}
  (method==='updateMarketLeverageLimits'?limitRows:method==='fetchLeverageForSymbol'?fetchRows:setRows).push({name,symbol,raw,input:tag(input),payloads,result:tag(result),error:caught??null,effects,eager,snapshots,cache:tag(store.fetchedLeverage),positions:tag(store.positions),sameArray:result===original,sameFirst:result?.[0]===original[0]});
 }
 const aliases={};
 {const events=[],{venue,store}=make('alias',events);venue.grvtClient.tdgClient.getAllInitialLeverage=async()=>({results:[{instrument:'other',max_leverage:'9'}]});const result=await venue.updateMarketLeverageLimits(store.markets);aliases.limitsSameFirst=result[0]===store.markets[0];}
 {const events=[],{venue,store}=make('alias',events);venue.log=()=>{store.markets[0].id='changed';venue.grvtTradingAccountId='sub-new';};venue.grvtClient.tdgClient.setInitialLeverage=async payload=>{aliases.setInstrument=payload.instrument;aliases.setAccount=payload.sub_account_id;return {s:true};};await venue.setLeverage('BTC_USDT_PERP',3);}
 {const events=[],{venue,store}=make('alias',events);venue.grvtClient.authenticatedEndpoint=async()=>{store.markets[0].id='changed';return {};};venue.grvtClient.tdgClient.getAllInitialLeverage=async()=>({r:[{i:'changed',l:'7.5'}]});aliases.fetchValue=tag(await venue.fetchLeverageForSymbol('BTC_USDT_PERP'));}
 {const events=[],{venue,store}=make('live',events);store.update=patch=>{store.fetchedLeverage=patch.fetchedLeverage;store.positions=[position('live')];};venue.grvtClient.tdgClient.setInitialLeverage=async()=>({s:true});await venue.setLeverage('BTC_USDT_PERP',3);aliases.postCachePosition=tag(store.positions[0]);}
 {const events=[],{venue}=make('unset',events);venue.log=()=>{venue.grvtTradingAccountId=undefined;};venue.grvtClient.tdgClient.setInitialLeverage=async payload=>{aliases.unsetAccount=tag(payload);aliases.unsetWire=JSON.stringify(payload);return {s:true};};await venue.setLeverage('BTC_USDT_PERP',3);}
 {const events=[],{venue,store}=make('cache-alias',events);venue.grvtClient.tdgClient.getAllInitialLeverage=async()=>({r:[{i:'btc_perp',l:'5.5'}]});const result=await venue.fetchLeverageForSymbol('BTC_USDT_PERP');aliases.fetchSameCache=result===store.fetchedLeverage.BTC_USDT_PERP;}
 assert.equal(candleRows.length,17);assert.equal(bookRows.length,17);assert.equal(limitRows.length,19);assert.equal(fetchRows.length,19);assert.equal(setRows.length,19);
 process.stdout.write(JSON.stringify({candleRows,bookRows,limitRows,fetchRows,setRows,aliases}));
})().catch(error=>{console.error(error);process.exitCode=1;});
