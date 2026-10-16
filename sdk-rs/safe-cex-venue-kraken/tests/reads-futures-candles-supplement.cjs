globalThis.futuresCandlesSupplement=async()=>{
 const opts={symbol:'PF_XBTUSD',interval:'5m',limit:3,from:0,to:1700000000123};
 const a={time:29000,open:'11.125',high:'23.25',low:'-7.5',close:'19.75',volume:'37.625'};
 const b={time:17000,open:'43.125',high:'59.25',low:'-13.5',close:'47.75',volume:'61.625'};
 const response={candles:[a,b,{...a,open:'71.25'}]};
 const specs=[];
 const add=(label,options=opts,data=response,extra={})=>specs.push({label,optsRaw:JSON.stringify(options),raw:data===undefined?null:JSON.stringify(data),...extra});
 for(const interval of ['1m','5m','15m','30m','1h','4h','12h','1d','1w','unknown','constructor','__proto__'])add('interval-'+interval,{...opts,interval});
 for(const limit of [-1,0,1,5001,0.25])add('limit-'+limit,{...opts,limit});
 for(const x of [{from:-1250.5},{from:1250.5,to:0},{limit:null,to:null},{from:1,to:-500.25},{symbol:'PF A/#',to:0}])add('options-'+specs.length,{...opts,...x});
 for(const data of [undefined,null,{},false,{candles:null},{candles:false},{candles:0},{candles:''},{candles:[]},{candles:{}},{candles:[null]},{candles:[7]}])add('shape-'+specs.length,opts,data,data===undefined?{raw:null}:{});
 add('coercions',opts,{candles:[{time:'1250.5',open:'-0',high:'Infinity',low:null,close:['7.25'],volume:{}},{time:null,open:'NaN',high:' 1.2px',low:true,close:[],volume:'-Infinity'}]});
 add('coercion-error',opts,{candles:[{...a,open:{toString:7,valueOf:9}}]});
 add('nonfinite',opts,response,{raw:'{"candles":[{"time":1e999,"open":"1","high":"2","low":"3","close":"4","volume":"5"},{"time":-1e999,"open":"6","high":"7","low":"8","close":"9","volume":"10"}]}'});
 for(const mode of ['sync','async'])for(const failure of ['none','report','emit'])add('error-'+mode+'-'+failure,opts,response,{mode,failure,message:mode==='sync'?'':'read failure'});
 add('clock-failure',{...opts,to:0},response,{clockFail:true});
 for(const failure of ['missing','emit'])add('missing-'+failure,opts,{}, {failure});
 add('testnet',opts,response,{testnet:true});
 add('mixed-NaN-timestamps',opts,{candles:[3000,2000,'NaN',1000].map(time=>({...a,time}))});
 const cases=[];
 for(const spec of specs){
  const calls=[],reports=[],emits=[];let clocks=0;
  const original=new Error(spec.message??'read failure'),reportError=new Error('report failure'),emitError=new Error('emit failure'),clockError=new Error('clock failure');
  const identity=e=>e===original?'read':e===reportError?'report':e===emitError?'emit':e===clockError?'clock':'mapped';
  const receiver={options:{testnet:spec.testnet??false},xhr:{get(url,{params}){calls.push({url,params:capture(params)});if(spec.mode==='sync')throw original;if(spec.mode==='async')return Promise.reject(original);return Promise.resolve({data:value(spec.raw)});}},emitAndLogError(message){emits.push(message);if(spec.failure==='emit')throw emitError;}};
  const fn=method(futuresFile,'KrakenExchange','fetchOHLCV',{Date:{now(){clocks++;if(spec.clockFail)throw clockError;return 1800000000321;}},console:{error(label,...args){if(label.includes('No candles')){reports.push({label,data:capture(args[0])});if(spec.failure==='missing')throw reportError;}else{reports.push({label,message:args[0],name:args[1].name,identity:identity(args[1])});if(spec.failure==='report')throw reportError;}}}},receiver);
  try{cases.push({...spec,calls,reports,emits,result:capture(await fn(JSON.parse(spec.optsRaw))),clocks});}
  catch(error){cases.push({...spec,calls,reports,emits,error:error.message,identity:identity(error),clocks});}
 }
 return {futuresCandles:cases.slice(0,47),futuresCandleBoundaries:cases.slice(47)};
};
