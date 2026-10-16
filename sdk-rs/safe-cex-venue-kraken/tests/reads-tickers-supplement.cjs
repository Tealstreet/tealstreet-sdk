globalThis.tickerSupplement = async () => {
 const keyFile=path.join(src,'utils/get-key.ts');
 const keySource=fs.readFileSync(keyFile,'utf8');
 const keyOutput=ts.transpileModule(keySource,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true},reportDiagnostics:true});
 assert.equal((keyOutput.diagnostics||[]).filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 const keyExports={};vm.runInNewContext(keyOutput.outputText,{exports:keyExports,require:require('node:module').createRequire(keyFile)},{filename:keyFile});
 const strip=method(path.join(src,'store/store.base.ts'),'DefaultStore','stripTickerInfo',{},{});
 const row={symbol:'PF_A',bid:'11.125',ask:'13.25',last:'17.5',markPrice:'19.75',indexPrice:'23.875',change24h:'-29.125',fundingRate:'31.25',vol24h:'37.5',volumeQuote:'41.75',openInterest:'43.875',tag:'row-A'};
 const second={...row,symbol:'PF_B',bid:'47.125',ask:'53.25',last:'59.5',markPrice:'61.75',indexPrice:'67.875',change24h:'71.125',fundingRate:'73.25',vol24h:'79.5',volumeQuote:'83.75',openInterest:'89.875',tag:'row-B'};
 const fallback=(tag,n)=>({id:'id-'+tag,symbol:'symbol-'+tag,bid:n+1,ask:n+2,last:n+3,mark:n+4,index:n+5,percentage:n+6,openInterest:n+7,fundingRate:n+8,fundingTime:n+9,volume:n+10,quoteVolume:n+11,info:{tag}});
 const specs=[];
 const add=(label,data={tickers:[row,second]},extra={})=>specs.push({label,raw:JSON.stringify(data),...extra});
 add('distinct-fields');add('late-markets',undefined,{lateMarkets:true});
 for(const data of [null,false,{}, {tickers:null},{tickers:false},{tickers:0},{tickers:''},{tickers:[]},{tickers:{}},{tickers:'x'},{tickers:[null]},{tickers:[false]},{tickers:[17]},{tickers:['row']},{tickers:[[]]},{tickers:[row,false]},{tickers:[{...row,symbol:'UNKNOWN'},second]}])add('shape-'+specs.length,data);
 add('undefined-data',undefined,{raw:null});
 for(const [key,val] of [['last','0'],['last','-3.25'],['last','n/a'],['last','Infinity'],['markPrice','0'],['markPrice','-5'],['indexPrice','-7'],['indexPrice','n/a'],['bid','-0'],['ask','1.25suffix'],['vol24h','Infinity'],['volumeQuote','n/a'],['fundingRate','Infinity'],['openInterest',null],['change24h',{}]])add(key+'-'+val,{tickers:[{...row,[key]:val},second]});
 add('aliases',{tickers:[{symbol:'PF_A',last:'7.5',bid:'1.25',ask:'3.75',mark_price:'111',index_price:'113',vol_24h:'127',volume_quote:'131',funding_rate:'17.25',open_interest:'19.5',change_24h:'137'}]});
 add('literal-null-stops-alias',{tickers:[{...row,fundingRate:null,funding_rate:'999',openInterest:null,open_interest:'888'}]});
 add('coercion-error',{tickers:[{...row,markPrice:{toString:7,valueOf:9}}]});
 add('coercion-replaces-fallback',{tickers:[{...row,markPrice:{toString:7,valueOf:9}}]},{replaceFallback:true});
 add('null-with-market',{tickers:[null]},{undefinedMarket:true});
 for(const mode of ['sync','async'])for(const traceFail of [false,true])for(const replaceFallback of [false,true])add('read-error-'+mode+'-'+traceFail+'-'+replaceFallback,undefined,{mode,traceFail,replaceFallback});
 add('shape-trace-error',{tickers:{}},{traceFail:true});
 const cases=[];
 for(const spec of specs){
  const initial=[fallback('old-A',101.25),fallback('old-B',211.5)].map(strip),later=[fallback('new-A',307.75),fallback('new-B',419.125)].map(strip);
  const markets=[{...market('PF_A',true),id:'market-id-A'},{...market('PF_B',true),id:'market-id-B'}];
  if(spec.undefinedMarket)markets.push({...market('undefined',true),id:'undefined-id'});
  const calls=[],traces=[];const original=new Error('ticker read failure'),traceError=new Error('trace failure');original.code='READ-TICKERS';
  let release;const pending=new Promise((resolve,reject)=>{release=()=>spec.mode==='async'?reject(original):resolve({data:value(spec.raw)});});
  // Sync failure never creates a rejected asynchronous request.
  const receiver={store:{marketsBySymbol:Object.fromEntries(markets.map(m=>[m.symbol,m])),tickers:initial},xhr:{get(endpoint){calls.push(endpoint);if(spec.mode==='sync')throw original;return pending;}}};
  const fn=method(futuresFile,'KrakenExchange','fetchTickers',{ENDPOINTS:futuresTypes.ENDPOINTS,v:keyExports.v,console:{trace(error){traces.push({name:error.name,message:error.message,sameError:error===original,code:error.code??null});if(spec.replaceFallback)receiver.store.tickers=later;if(spec.traceFail)throw traceError;}}},receiver);
  const promise=fn();const prefixCalls=calls.length;
  if(spec.lateMarkets)receiver.store.marketsBySymbol=Object.fromEntries(markets.map(m=>[m.symbol,{...m,id:'late-'+m.id}]));
  if(spec.mode!=='sync')release();
  try {const result=await promise;cases.push({...spec,markets,calls,prefixCalls,traces,result:capture(result),sameFallback:result===receiver.store.tickers,storeAfter:capture(receiver.store.tickers),fallback:capture(initial),laterFallback:capture(later)});}
  catch(error){cases.push({...spec,markets,calls,prefixCalls,traces,error:error.message,sameError:error===traceError,storeAfter:capture(receiver.store.tickers),fallback:capture(initial),laterFallback:capture(later)});}
 }
 const boundaryRaw=JSON.stringify({tickers:[{...row,symbol:'constructor'}]});
 const boundaryReceiver={store:{marketsBySymbol:{},tickers:[]},xhr:{get:async()=>({data:JSON.parse(boundaryRaw)})}};
 const boundaryFn=method(futuresFile,'KrakenExchange','fetchTickers',{ENDPOINTS:futuresTypes.ENDPOINTS,v:keyExports.v,console:{trace(){throw Error('unexpected boundary trace');}}},boundaryReceiver);
 return {futuresTickers:cases,tickerBoundary:{raw:boundaryRaw,result:capture(await boundaryFn())},tickerHeader:{getKeySha256:crypto.createHash('sha256').update(keySource).digest('hex')}};
};
