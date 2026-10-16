const assert=require('node:assert/strict');
const {BreakoutpropExchange,encode,observe,corpus}=require('./portfolio_oracle.cjs');
const methods=['fetchInstrumentsRaw','fetchInstrumentDetailsRaw','fetchPortfolioRaw'];
async function capture(method,scenario){
 const trace=[], errors=[];
 const ex=new BreakoutpropExchange('breakoutprop',{},{});
 const error={message:'fallback-message',response:{data:{msg:scenario==='empty-msg'?'':scenario==='numeric-msg'?19:scenario==='object-msg'?{detail:'object'}:'venue-message'}}};
 const loggerError={message:'logger-failure',marker:317};
 const step=name=>{trace.push(name);if(scenario===name)throw error;};
 ex.emitAndLogError=e=>{errors.push(observe(e));if(scenario==='logger-failure')throw loggerError;};
 ex.getSessionToken=async()=>{step('session');return 'token';};
 ex.getAccountCode=async()=>{step('account');return 'account-A';};
 let data={
  instruments:[{symbol:'FIRST',marker:-0},{symbol:'XBTUSD',marker:22},{symbol:'LAST',marker:Infinity}],
  instrumentDetails:[{symbol:'LAST',marker:44},{symbol:'XBTUSD',marker:55},{symbol:'FIRST',marker:-Infinity}],
  portfolios:[{marker:'raw-portfolio',negativeZero:-0,overflow:Infinity}],
  ignoredTop:'preserved only by portfolio',
 };
 const field=method==='fetchInstrumentsRaw'?'instruments':'instrumentDetails';
 if(scenario==='null-root')data=null;
 if(scenario==='missing-field')delete data[field];
 if(scenario==='null-field')data[field]=null;
 if(scenario==='scalar-field')data[field]=42;
 if(scenario==='null-element')data[field]=[{symbol:'FIRST'},null];
 if(scenario==='primitive-elements')data[field]=[false,42,'XBTUSD',[],{symbol:'XBTUSD'},{symbol:12}];
 if(scenario==='multiple-portfolios')data.portfolios.push({marker:'second-portfolio',negativeZero:19,overflow:27});
 const raw=encode(data);
 ex.xhr={get:async(url,config)=>{trace.push({method:'GET',url,config});step('http');if(['empty-msg','numeric-msg','object-msg','logger-failure'].includes(scenario))throw error;return {data:JSON.parse(raw)};}};
 try{return {method,scenario,raw,trace,errors,ok:observe(await ex[method]())};}
 catch(e){return {method,scenario,raw,trace,errors,rejected:e.message,sameLoggerError:e===loggerError};}
}
(async()=>{
 const cases=[];
 for(const method of methods)for(const scenario of ['none','session','account','http','empty-msg','numeric-msg','object-msg','logger-failure','null-root','missing-field','null-field','scalar-field','null-element','primitive-elements','multiple-portfolios'])cases.push(await capture(method,scenario));
 assert.equal(cases.length,45);
 for(const c of cases){
  assert.equal(c.trace[0],'session','source_session_first');
  if(c.scenario==='session')assert.equal(c.trace.length,1,'source_session_failure_stops');
  if(c.scenario==='account')assert.deepEqual(c.trace,['session','account'],'source_account_failure_stops');
 }
 const abandoned=[],aliases=[],utf8=[];
 for(const method of methods){
  const trace=[],ex=new BreakoutpropExchange('breakoutprop',{},{});let settle;
  const data={instruments:[{symbol:'ONE',marker:11}],instrumentDetails:[{symbol:'ONE',marker:13}],portfolios:[{symbol:'ONE',marker:17}]};
  const field=method==='fetchInstrumentsRaw'?'instruments':method==='fetchInstrumentDetailsRaw'?'instrumentDetails':'portfolios';
  ex.getSessionToken=()=>{trace.push('session');return new Promise(r=>{settle=r;});};
  ex.getAccountCode=async()=>{trace.push('account');return 'account-A';};
  ex.xhr={get:async()=>{trace.push('request');return {data};}};
  void ex[method]();const before=[...trace];settle('token');await new Promise(setImmediate);
  abandoned.push({method,before,after:[...trace],raw:encode(data)});
  ex.getSessionToken=async()=> 'token';
  data[field][0].marker=19;const aliasRaw=encode(data);
  const result=await ex[method]();result[field][0].marker=777;
  aliases.push({method,field,changed:data[field][0].marker,raw:aliasRaw});
  data[field][0].symbol='\ud800';const surrogateRaw=encode(data);const surrogateResult=await ex[method]();
  utf8.push({method,raw:surrogateRaw,codeUnit:surrogateResult[field][0].symbol.charCodeAt(0)});
 }
 console.log(JSON.stringify({runtime:corpus.runtime,cases,abandoned,aliases,utf8}));
})().catch(e=>{console.error(e);process.exitCode=1;});
