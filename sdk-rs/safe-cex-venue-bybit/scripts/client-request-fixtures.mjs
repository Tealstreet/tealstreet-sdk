import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createHmac} from 'node:crypto';
import vm from 'node:vm';
import ts from 'typescript';
import axios, {AxiosHeaders} from 'axios';
import qs from 'qs';
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
const source = ts.createSourceFile('api.ts', readFileSync(new URL('exchanges/bybit/bybit.api.ts', root), 'utf8'), ts.ScriptTarget.Latest, true);
const code = source.statements.filter(ts.isVariableStatement).map(s => s.getText(source).replace(/^export /, '')).join('\n');
const cases = [];
for (const method of ['GET','POST']) for (const url of ['/v5/order/create','/v5/market/kline'])
for (const mode of ['local','no-key','reject']) for (const data of [undefined, {amount:1e-7,price:1e21}, '  {"x":1}  ', 'plain']) {
  const wire = []; const preimages = [];
  const options = {key: mode === 'no-key' ? '' : 'fixture-key', secret:'fixture-secret', extra:{recvWindow:12345}};
  const siteHeaders = {'x-site-id':'BRA_BTL'};
  const context = vm.createContext({AxiosHeaders, qs, console, nanoid:()=>'fixture-id',
    BASE_URL:{livenet:'https://api.bybit.com'}, DEMO_URL:{livenet:'https://api-demo.bybit.com'}, BROKER_ID:'Tealstreet', RECV_WINDOW:20000,
    ENDPOINTS:{KLINE:'/v5/market/kline',MARKETS:'/v5/market/instruments-info'},
    // The common factory is the boundary here; execute the actual Axios request
    // and body transform after the extracted venue installs its interceptors.
    safeCexAxiosCreate: config => axios.create({...config, timeout:20000, adapter:async request => {
      const headers = request.headers.toJSON();
      wire.push({method:request.method,url:axios.getUri({...request,baseURL:undefined}),headers,data:request.data??null});
      return {status:200,statusText:'OK',headers:{},data:'{"retCode":0}',config:request};
    }}),
    retry:()=>{}, getBybitSiteHeaders:()=>siteHeaders, markSkipRemoteSigning:()=>{},
    canSignPayload:()=>true, signPayload:async(_opts,_name,payload)=>{
      preimages.push(payload); if(mode==='reject') throw new Error('signer refused');
      return {signature:createHmac('sha256',options.secret).update(payload).digest('hex')};
    },
    virtualClock:{getCurrentTimeMs:()=>1000},
    getBybitResponseMeta:()=>({hasExplicitStatus:true,isSuccess:true,retCode:0}),
  });
  vm.runInContext(ts.transpileModule(code+'\nglobalThis.createAPI=createAPI;', {compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText, context);
  const client = context.createAPI(options, {name:'bybit',isDemo:false,recordAuthSuccess:()=>{}});
  const request = {method,url,params:{z:1,requestId:'old'},...(data===undefined?{}:{data})};
  let error = null;
  try {await client.request(request);} catch(e) {error=e.message;}
  cases.push({options,siteHeaders,mode,request,wire,preimages,error});
}
assert.equal(cases.length,48);
process.stdout.write(JSON.stringify(cases));
