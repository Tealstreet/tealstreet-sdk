const {BreakoutpropExchange,store,encode,observe}=require('./portfolio_oracle.cjs');
const instrument=(symbol,n)=>({symbol,type:'FOREX',version:17,description:'instrument',priceIncrement:n===0?.01:.25,pipSize:.0001,currency:'USD',lotSize:n===0?13.5:29.25,multiplier:7,firstCurrency:'BASE',extra:{zero:-0,huge:Infinity}});
const detail=(symbol,n)=>({symbol,account:'account',type:'FOREX',tradingStatus:n===0?'FULL':'CLOSE_ONLY',minOrderSize:n===0?.125:.375,maxOrderSize:{value:n===0?101.5:223.25,limitType:'ABS'},minOrderSizeIncrement:n===0?.1:1,marginRate:{rateType:'RATE',tiers:[{endVolume:'INF',value:n===0?.2:.125},{endVolume:'SECOND',value:.75}]}});
const marketData=()=>['USDABCUSD','ETHUSD'].map((symbol,n)=>({instrument:instrument(symbol,n),instrumentDetails:detail(symbol,n)}));
const tickerData=()=>({events:[{symbol:'A',type:'Quote',bid:13.125,ask:17.375,time:'2026-01-01',extra:{zero:-0,huge:Infinity}},{symbol:'B',type:'Quote',bid:29.25,ask:31.5,time:'2026-01-02'}]});
const marketSpecs=['distinct','empty','null','false','zero','negative-zero','empty-string','missing-tier','zero-margin','negative-zero-margin','nonfinite','missing-unused','reject','logger','surrogate'];
const tickerSpecs=['distinct','empty','missing-events','null-events','null-root','session','account','http','empty-msg','object-msg','logger','nonfinite','signed-zero','missing-unused','surrogate'];
function fixtureStore(){const s=store();s.markets.push({...s.markets[0]});return s;}
(async()=>{
 const cases=[];
 for(const name of marketSpecs){
  let input=marketData();
  if(name==='empty')input=[];
  if(name==='null')input=null;if(name==='false')input=false;if(name==='zero')input=0;if(name==='negative-zero')input=-0;if(name==='empty-string')input='';
  if(name==='missing-tier'||name==='logger')input[1].instrumentDetails.marginRate.tiers=[];
  if(name==='zero-margin')input[0].instrumentDetails.marginRate.tiers[0].value=0;
  if(name==='negative-zero-margin')input[0].instrumentDetails.marginRate.tiers[0].value=-0;
  if(name==='nonfinite'){input[0].instrument.priceIncrement=Infinity;input[1].instrumentDetails.minOrderSizeIncrement=-Infinity;}
  if(name==='missing-unused')delete input[0].instrument.description;
  if(name==='surrogate')input[0].instrument.symbol='\ud800';
  const s=fixtureStore(),ex=new BreakoutpropExchange('breakoutprop',{},s),trace=[],errors=[];
  const original=Object.assign(new Error('original-rejection'),{store_subscriber_ids:[19,31]}),logger=Object.assign(new Error('logger-rejection'),{store_subscriber_ids:[47,59]});
  ex.fetchCombinedInstruments=()=>{trace.push('combined');return name==='reject'?Promise.reject(original):Promise.resolve(input);};
  ex.emitAndLogError=e=>{errors.push(e);if(name==='logger')throw logger;};
  let expected;try{expected={ok:observe(await ex.fetchMarkets()),errors};}catch(e){expected={error:e.message,sameError:e===(name==='logger'?logger:original),errors};}
  if(name==='surrogate')expected={codeUnit:expected.ok[0].symbol.charCodeAt(0)};
  cases.push({method:'fetchMarkets',name,raw:encode(input),storeRaw:encode(s),expected,trace});
 }
 for(const method of ['fetchTickersForSymbols','fetchTickers'])for(const name of tickerSpecs){
  let input=tickerData();if(name==='empty')input.events=[];if(name==='missing-events')delete input.events;if(name==='null-events')input.events=null;if(name==='null-root')input=null;
  if(name==='nonfinite'){input.events[0].bid=Infinity;input.events[0].ask=-Infinity;}if(name==='signed-zero'){input.events[0].bid=-0;input.events[0].ask=0;}
  if(name==='missing-unused')delete input.events[0].time;if(name==='surrogate')input.events[0].symbol='\ud800';
  const s=fixtureStore(),ex=new BreakoutpropExchange('breakoutprop',{},s),trace=[],errors=[];
  const original=Object.assign(new Error('original-rejection'),{response:{data:{msg:name==='empty-msg'?'':name==='object-msg'?{detail:'object'}:'venue-message'}}});
  const logger=Object.assign(new Error('logger-rejection'),{store_subscriber_ids:[47,59]});
  ex.getSessionToken=()=>{trace.push('session');return name==='session'?Promise.reject(original):Promise.resolve('token');};
  ex.getAccountCode=()=>{trace.push('account');return name==='account'?Promise.reject(original):Promise.resolve('account-A');};
  ex.xhr={post:async(url,body)=>{trace.push({method:'POST',url,body});if(['http','empty-msg','object-msg','logger'].includes(name))throw original;return {data:input};}};
  ex.emitAndLogError=e=>{errors.push(e);if(name==='logger')throw logger;};
  let expected;try{expected={ok:observe(await ex[method](['A','A','B'])),errors};}catch(e){expected={error:e.message,sameError:e===logger,errors};}
  if(name==='surrogate')expected={codeUnit:expected.ok[0].symbol.charCodeAt(0)};
  cases.push({method,name,raw:encode(input),storeRaw:encode(s),expected,trace});
 }
 const aliases=[];
 for(const method of ['fetchMarkets','fetchTickersForSymbols','fetchTickers']){
  const s=fixtureStore(),input=method==='fetchMarkets'?marketData():tickerData(),ex=new BreakoutpropExchange('breakoutprop',{},s);
  ex.fetchCombinedInstruments=async()=>input;ex.getSessionToken=async()=>'';ex.getAccountCode=async()=>'';ex.xhr={post:async()=>({data:input})};
  const output=await ex[method]([]);if(method==='fetchMarkets'){output[0].info.instrument.description='changed';}else{output[0].info.symbol='changed';}
  aliases.push({method,changed:method==='fetchMarkets'?input[0].instrument.description:input.events[0].symbol});
 }
 const abandoned=[];
 for(const method of ['fetchMarkets','fetchTickersForSymbols','fetchTickers']){
  const ex=new BreakoutpropExchange('breakoutprop',{},fixtureStore());let settle,calls=0;
  ex.fetchCombinedInstruments=()=>new Promise(resolve=>{settle=()=>resolve(marketData());});
  ex.getSessionToken=()=>new Promise(resolve=>{settle=()=>resolve('');});ex.getAccountCode=async()=>{calls++;return '';};ex.xhr={post:async()=>{calls++;return {data:tickerData()};}};
  void ex[method]([]).then(()=>{calls++;});settle();await new Promise(setImmediate);abandoned.push({method,calls});
 }
 const inputEx=new BreakoutpropExchange('breakoutprop',{},fixtureStore());let settleInput,posted;
 inputEx.getSessionToken=()=>new Promise(resolve=>{settleInput=resolve;});inputEx.getAccountCode=async()=>'';inputEx.xhr={post:async(_,body)=>{posted=body.symbols;return {data:{events:[]}};}};
 const symbols=['ORIGINAL'],inputPromise=inputEx.fetchTickersForSymbols(symbols);symbols.push('LATE');settleInput('');await inputPromise;
 const inputAlias={posted};
 const fallbacks=[];
 for(const method of ['fetchMarkets','fetchTickersForSymbols','fetchTickers']){
  const s=fixtureStore(),ex=new BreakoutpropExchange('breakoutprop',{},s);let settle;
  ex.fetchCombinedInstruments=()=>new Promise(resolve=>{settle=()=>resolve([]);});
  ex.getSessionToken=()=>new Promise((_,reject)=>{settle=()=>reject(Error('fallback'));});
  const promise=ex[method]([]);s.markets=s.markets.slice(0,1);s.tickers=s.tickers.slice(0,1);settle();const result=await promise;
  const expected=observe(result);result.push(result[0]);fallbacks.push({method,expected,aliasedLength:method==='fetchMarkets'?s.markets.length:s.tickers.length});
 }
 const precedenceInput=marketData();precedenceInput[0].instrumentDetails.marginRate.tiers=[];delete precedenceInput[1].instrument.description;
 const precedenceStore=fixtureStore(),precedenceEx=new BreakoutpropExchange('breakoutprop',{},precedenceStore);precedenceEx.fetchCombinedInstruments=async()=>precedenceInput;
 const precedence={raw:encode(precedenceInput),storeRaw:encode(precedenceStore),expected:{ok:observe(await precedenceEx.fetchMarkets()),errors:precedenceEx.errors}};
 process.stdout.write(JSON.stringify({runtime:{node:process.version,nodeMajor:+process.versions.node.split('.')[0],arch:process.arch,v8:process.versions.v8},cases,aliases,abandoned,inputAlias,fallbacks,precedence}));
})().catch(e=>{console.error(e);process.exitCode=1;});
