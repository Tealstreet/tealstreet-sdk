globalThis.ohlcvSupplement = async () => {
 const first=[29.5,'11.125','23.25','-7.5','19.75','999.5','37.625',41];
 const second=[17.25,'43.125','59.25','-13.5','47.75','887.5','61.625',73];
 const duplicate=[17.25,'71.125','83.25','-17.5','79.75','773.5','97.625',101];
 const base={symbol:'BTC/USD',interval:'5m',from:1729.5};
 const response={result:{last:19,XXBTZUSD:[first,second,duplicate]}};
 const specs=[];
 const add=(label,opts=base,data=response,extra={})=>specs.push({label,optsRaw:JSON.stringify(opts),raw:data===undefined?null:JSON.stringify(data),...extra});
 for(const interval of [...Object.keys(spotTypes.INTERVAL),'unknown','constructor','__proto__'])add('interval-'+interval,{...base,interval});
 for(const from of [0,-1250.25,null])add('from-'+from,{...base,from});
 add('from-negative-zero',base,response,{optsRaw:'{"symbol":"BTC/USD","interval":"5m","from":-0}'});
 add('missing-market',{...base,symbol:'UNKNOWN'});
 for(const pair of ['',0,false,true,17.25,['ARRAY'],{id:'OBJECT'}])add('pair-'+JSON.stringify(pair),base,response,{pair});
 for(const data of [undefined,null,false,{}, {result:null},{result:false},{result:[]},{result:'x'},{result:{last:7}},{result:{'': [first],XXBTZUSD:[second]}},{result:{XXBTZUSD:[]}}, {result:{XXBTZUSD:null}}, {result:{XXBTZUSD:{}}},{result:{XXBTZUSD:[null]}}, {result:{XXBTZUSD:[{0:3.5,1:'1px',2:'Infinity',3:null,4:true,6:[]}]}}])add('shape-'+specs.length,base,data);
 add('key-order',base,{result:{'12':[first],'2':[second],last:5}});
 add('array-result',base,{result:[[first,second]]});
 add('numeric-fields',base,{result:{XXBTZUSD:[[0,'-0','Infinity','n/a',[],false,{}],[-1,'1.2rest',null,true,'  5.75',0,['7.25']]]}});
 for(const mode of ['sync','async'])for(const message of ['', 'read failure'])for(const consoleFail of [false,true])add('error-'+mode+'-'+message+'-'+consoleFail,base,response,{mode,message,consoleFail});
 add('undefined-data',base,response,{raw:null});
 add('nonfinite-timestamps',base,response,{raw:'{"result":{"XXBTZUSD":[[1e999,"1","2","3","4","5","6"],[-1e999,"7","8","9","10","11","12"],[1e999,"13","14","15","16","17","18"]]}}'});
 const cases=[];
 for(const spec of specs){
  const calls=[],reports=[]; const original=new Error(spec.message??'read failure'),consoleError=new Error('console failure');
  const receiver={store:{marketsBySymbol:{'BTC/USD':{info:{pairName:Object.hasOwn(spec,'pair')?spec.pair:'XXBTZUSD'}}}},xhr:{get(endpoint,{params}){
   const interval=typeof params.interval==='function'||(params.interval&&typeof params.interval==='object')?{inherited:String(params.interval)}:capture(params.interval);
   calls.push({endpoint,pair:capture(params.pair),interval,since:Object.hasOwn(params,'since')?capture(params.since):{undefined:true}});
   if(spec.mode==='sync')throw original;
   if(spec.mode==='async')return Promise.reject(original);
   return Promise.resolve({data:value(spec.raw)});
  }}};
  const fn=method(spotFile,'KrakenSpotExchange','fetchOHLCV',{ENDPOINTS:spotTypes.ENDPOINTS,INTERVAL:spotTypes.INTERVAL,console:{error(label,message){reports.push([label,message]);if(spec.consoleFail)throw consoleError;}}},receiver);
  try{cases.push({...spec,calls,reports,result:capture(await fn(JSON.parse(spec.optsRaw)))});}
  catch(error){cases.push({...spec,calls,reports,error:error.message,sameError:error===consoleError});}
 }
 const boundaries=[];
 for(const stamp of ['17.25',null]){
  const raw=JSON.stringify({result:{XXBTZUSD:[[stamp,'1','2','3','4','5','6']]}});
  const receiver={store:{marketsBySymbol:{}},xhr:{get:async()=>({data:JSON.parse(raw)})}};
  const fn=method(spotFile,'KrakenSpotExchange','fetchOHLCV',{ENDPOINTS:spotTypes.ENDPOINTS,INTERVAL:spotTypes.INTERVAL,console:{error(){throw Error('unexpected source boundary report');}}},receiver);
  boundaries.push({raw,optsRaw:JSON.stringify(base),result:capture(await fn(base))});
 }
 return {ohlcv:cases,ohlcvBoundaries:boundaries};
};
