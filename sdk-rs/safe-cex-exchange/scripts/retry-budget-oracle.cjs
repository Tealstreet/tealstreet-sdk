const path = require('node:path');
const isolated = process.env.SAFE_CEX_TYPESCRIPT_PATH;
if (!isolated) throw Error('isolated Axios dependencies required');
const root = path.dirname(isolated);
const axios = require(path.join(root,'axios'));
const retryModule = require(path.join(root,'axios-retry'));
const retry = retryModule.default;
const predicates=[];
for(const method of ['', 'get', 'GET', 'head', 'options', 'put', 'delete', 'post'])
for(const code of [null,'','ECONNABORTED','ERR_CANCELED','ENOTFOUND','CERT_HAS_EXPIRED','ECONNRESET','CUSTOM'])
for(const status of [null,200,400,429,499,500,503,504,599,600]) {
 const error={config:{method},code:code??undefined,response:status===null?undefined:{status}};
 predicates.push({method,code,status,expected:retryModule.isNetworkOrIdempotentRequestError(error)});
}
(async()=>{
 const nativeNow=Date.now,nativeTimeout=global.setTimeout,nativeClear=global.clearTimeout;
 const budgets=[];
 try {
  for(const scenario of [
   {timeout:20000,elapsed:8000,delay:0,start:1000},
   {timeout:20000,elapsed:21000,delay:0,start:1000},
   {timeout:20000,elapsed:5000,delay:1000,start:1000},
   {timeout:1000,elapsed:0,delay:1000,start:1000},
   {timeout:0,elapsed:8000,delay:1000,start:1000},
   {timeout:20000,elapsed:8000,delay:0,start:0},
  ]) {
   let now=scenario.start;const timeouts=[],waits=[];
   Date.now=()=>now;
   global.setTimeout=(callback,ms)=>{waits.push(ms);now+=ms;queueMicrotask(callback);return 1;};
   global.clearTimeout=()=>{};
   const client=axios.create();
   retry(client,{retries:3,retryCondition:()=>true,retryDelay:n=>n*scenario.delay});
   let message;
   try {await client.get('/fixture',{timeout:scenario.timeout,adapter:async config=>{
    timeouts.push(config.timeout);now+=scenario.elapsed;
    throw new axios.AxiosError('attempt-'+timeouts.length,'CUSTOM',config);
   }});} catch(error){message=error.message;}
   budgets.push({...scenario,timeouts,waits,message});
  }
 } finally {Date.now=nativeNow;global.setTimeout=nativeTimeout;global.clearTimeout=nativeClear;}
 console.log(JSON.stringify({predicates,budgets}));
})().catch(error=>{console.error(error);process.exitCode=1;});
