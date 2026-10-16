const {BreakoutpropExchange,store,encode,observe}=require('./portfolio_oracle.cjs');
const intervals=['1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w'];
const specs=[...intervals,'absent','zero','negative-zero','fractional','invalid-bound','empty','invalid-date','local-date','nonfinite','missing-unused','session','account','http','logger','empty-msg','object-msg'];
const data=()=>({events:[{symbol:'BTCUSD',type:'Candle',candleType:'h',time:'2024-12-11T09:00:00.123Z',open:17.125,high:29.25,low:3.5,close:19.75,volume:41.875},{symbol:'OTHER',type:'Candle',candleType:'h',time:'2025-01-02T03:04:05.678+02:00',open:53.125,high:79.25,low:7.5,close:59.75,volume:101.875}]});
(async()=>{
 const cases=[];
 for(const name of specs){
  const input=data(),options={symbol:'BTCUSD',interval:intervals.includes(name)?name:'1h',from:1700000000123,to:1700000060789,limit:1};
  if(name==='absent'){delete options.from;delete options.to;}if(name==='zero'){options.from=0;options.to=0;}if(name==='negative-zero'){options.from=-0;options.to=-0;}
  if(name==='fractional'){options.from=-1.9;options.to=1.9;}if(name==='invalid-bound')options.from=Infinity;
  if(name==='empty')input.events=[];if(name==='invalid-date')input.events[0].time='not-a-date';if(name==='local-date')input.events[0].time='2024-12-11T09:00:00.123';
  if(name==='nonfinite'){input.events[0].open=Infinity;input.events[0].high=-Infinity;input.events[1].volume=-0;}
  if(name==='missing-unused')delete input.events[0].symbol;
  const ex=new BreakoutpropExchange('breakoutprop',{},store()),trace=[],errors=[];
  const original=Object.assign(Error('original-rejection'),{response:{data:{msg:name==='empty-msg'?'':name==='object-msg'?{detail:'venue'}:'venue-message'}}});
  const logger=Error('logger-rejection');
  ex.getSessionToken=()=>{trace.push('session');return name==='session'?Promise.reject(original):Promise.resolve('token');};
  ex.getAccountCode=()=>{trace.push('account');return name==='account'?Promise.reject(original):Promise.resolve('account-A');};
  ex.xhr={post:async(url,body)=>{trace.push({method:'POST',url,wire:JSON.stringify(body)});if(['http','logger','empty-msg','object-msg'].includes(name))throw original;return {data:input};}};
  ex.emitAndLogError=e=>{errors.push(e);if(name==='logger')throw logger;};
  let expected;try{expected={ok:observe(await ex.fetchOHLCV(options)),errors};}catch(e){expected={error:e.message,name:e.name,sameError:e===(name==='logger'?logger:original),errors};}
  const iso=[];for(const n of [options.from,options.to])if(n){try{iso.push({bits:observe(n).number,value:new Date(n).toISOString()});}catch(e){iso.push({bits:observe(n).number,error:e.message});}}
  cases.push({name,raw:encode(input),options:encode(options),iso,trace,expected});
 }
 const ex=new BreakoutpropExchange('breakoutprop',{},store());let resolve,posted,completed=0;
 ex.getSessionToken=()=>new Promise(r=>{resolve=r;});ex.getAccountCode=async()=>'';ex.xhr={post:async(_,body)=>{posted=body.symbols;completed++;return {data:{events:[]}};}};
 const options={symbol:'ORIGINAL',interval:'1m'};const promise=ex.fetchOHLCV(options);options.symbol='LATE';resolve('');await promise;
 const supplements={};
 for(const name of ['nan','surrogate','discarded']){
  const ex=new BreakoutpropExchange('breakoutprop',{},store());let settle,calls=0,wire;
  ex.getSessionToken=()=>new Promise(r=>{settle=r;});ex.getAccountCode=async()=>'';
  const input=data();if(name==='surrogate')input.events[0].symbol='\ud800';
  ex.xhr={post:async(_,body)=>{calls++;wire=JSON.stringify(body);return {data:input};}};
  const opts={symbol:'BTCUSD',interval:'1m'};if(name==='nan'){opts.from=NaN;opts.to=NaN;}
  const p=ex.fetchOHLCV(opts).then(rows=>{calls++;return rows;});settle('');
  if(name==='discarded'){void p;await new Promise(setImmediate);supplements[name]={calls};}
  else {const rows=await p;supplements[name]={wire,count:rows.length,raw:encode(input)};}
 }
 process.stdout.write(JSON.stringify({runtime:{node:process.version,nodeMajor:+process.versions.node.split('.')[0],arch:process.arch,v8:process.versions.v8},cases,owned:{posted,completed},supplements}));
})().catch(e=>{console.error(e);process.exitCode=1;});
