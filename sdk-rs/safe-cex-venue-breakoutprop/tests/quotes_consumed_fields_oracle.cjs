const {BreakoutpropExchange,store,encode,observe}=require('./portfolio_oracle.cjs');
(async()=>{
 const cases=[];
 for(const method of ['fetchTickersForSymbols','fetchTickers'])for(const name of ['minimal','poisoned-unused','null-row','scalar-events','null-root','null-events','missing-events','empty','valid-then-null','projection-before-null','logger']){
  let input={events:[{symbol:'A',bid:13.125,ask:17.375},{symbol:'B',bid:29.25,ask:31.5}]};
  if(name==='poisoned-unused')input.events.forEach(row=>Object.assign(row,{type:{ignored:true},time:null}));
  if(name==='null-row'||name==='logger')input.events=[null];
  if(name==='scalar-events')input.events='unused';
  if(name==='null-root')input=null;
  if(name==='null-events')input.events=null;
  if(name==='missing-events')delete input.events;
  if(name==='empty')input.events=[];
  if(name==='valid-then-null'||name==='projection-before-null')input.events.push(null);
  if(name==='projection-before-null')input.events[0].bid='13';
  const s=store(),ex=new BreakoutpropExchange('breakoutprop',{},s),errors=[];
  ex.getSessionToken=async()=>'';ex.getAccountCode=async()=>'';ex.xhr={post:async()=>({data:input})};
  const reporter=Error('logger-rejection');
  ex.emitAndLogError=e=>{errors.push(e);if(name==='logger')throw reporter;};
  let expected;try{expected={ok:observe(await ex[method](['A','B'])),errors};}catch(e){expected={error:e.message,name:e.name,errors};}
  cases.push({method,name,raw:encode(input),storeRaw:encode(s),expected});
 }
 process.stdout.write(JSON.stringify(cases));
})().catch(error=>{console.error(error);process.exitCode=1;});
