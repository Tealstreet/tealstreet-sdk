import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import vm from 'node:vm';
import ts from 'typescript';
import qs from 'qs';
import axios, {AxiosHeaders} from 'axios';
import {createServer, validateHeaderValue} from 'node:http';
import {once} from 'node:events';
const root=fileURLToPath(new URL('../../../',import.meta.url));
const source=ts.createSourceFile('api.ts',readFileSync(root+'packages/safe-cex/src/exchanges/bybit/bybit.api.ts','utf8'),ts.ScriptTarget.Latest,true);
const code=source.statements.filter(ts.isVariableStatement).map(s=>s.getText(source).replace(/^export /,'')).join('\n');
let interceptor, responseInterceptor, payload;
let signerValue=' \tclean\r\n\u0000\u007fvalue\t ';
const context=vm.createContext({AxiosHeaders,qs,console,nanoid:()=> 'fixed-request-id',
  BASE_URL:{livenet:'https://api.bybit.com'},DEMO_URL:{livenet:'https://api-demo.bybit.com'},BROKER_ID:'broker',RECV_WINDOW:20000,
  ENDPOINTS:{KLINE:'/v5/market/kline',MARKETS:'/v5/market/instruments-info'},
  safeCexAxiosCreate:()=>({interceptors:{request:{use:fn=>interceptor=fn},response:{use:fn=>responseInterceptor=fn}}}),
  retry:()=>{},getBybitSiteHeaders:options=>options.siteHeaders,markSkipRemoteSigning:c=>{c.skip_remote_signing=true;},
  canSignPayload:o=>!!o.secret||!!o.payloadSigner,signPayload:async(_o,_e,p)=>{payload=p;return {signature:'signed',headers:{'X-Test':'signer','X-Signer':signerValue}};},
  virtualClock:{getCurrentTimeMs:()=>1791518966503},getBybitResponseMeta:()=>({hasExplicitStatus:false}),
});
vm.runInContext(ts.transpileModule(code+'\nglobalThis.createAPI=createAPI;',{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,context);
const cases=[];
const parameters=[{}, {z:1,a:2}, {requestId:'old',a:1}, {amount:1e-7,price:1e21}, {'10':'ten','2':'two','01':'one','4294967295':'max'}, {nested:{b:true,a:null},list:[1,'x y',null]}, {empty:[],object:{},text:'!\'()*~😃\uFEFF\u0085'}, {huge:9007199254740993}];
const parameterPairs=[...parameters.map(params=>({params,data:params})),{params:{queryOnly:17},data:{bodyOnly:29}}];
for(const method of ['get','post']) for(const url of ['/v5/order/create','/v5/market/tickers','/v5/market/kline'])
 for(const mode of ['local','remote','no-key','no-secret','payload']) for(const {params,data} of parameterPairs) for(const lowercase of [false,true]) {
 const options={key:mode==='no-key'?'':'public-key',secret:mode==='no-secret'||mode==='payload'?'':'secret',remoteSigner:mode==='remote',payloadSigner:mode==='payload',siteHeaders:{'x-site-id':' \tBRA_BTL\r\n '}};
 const config={method,url,params,data,headers:lowercase?{'x-test':'request','x-bapi-sign':'old','X-BAPI-RECV-WINDOW':'20000'}:{'X-Test':'request','X-BAPI-RECV-WINDOW':'20000'}};
 context.createAPI(options,{name:'bybit',isDemo:false}); payload=null;
 const result=await interceptor(config);
 cases.push({options,config,expected:{params:result.params,headers:AxiosHeaders.from(result.headers).toJSON(),skip:!!result.skip_remote_signing,payload,body:JSON.stringify(result.data)}});
}
// These source assertions pin exact auth punctuation and absent reason behavior independently.
const statuses=[];
context.createAPI({remoteSigner:true,siteHeaders:{}},{name:'bybit',isDemo:false,recordAuthFailure:()=>{}});
for(const status of [401,403,500]) for(const statusText of [undefined,'','Custom reason']) {
 const response=await responseInterceptor({status,statusText,data:{},config:{safeCexBybitEnvProbe:true}});
 const expected=status===401?'HTTP 401 Unauthorized — API key rejected':status===403?'HTTP 403 Forbidden — API key rejected':`HTTP 500${statusText?' '+statusText:''}`;
 assert.equal(response.data.retMsg,expected);
 statuses.push({status,statusText:statusText??null,message:response.data.retMsg});
}
// Extract the HOW adapter's actual header projection; no HTTP validation occurs here.
const how=ts.createSourceFile('how.ts',readFileSync(root+'packages/http-over-websocket/src/axiosHttpOverWebsocketAdapter.ts','utf8'),ts.ScriptTarget.Latest,true);
const headerStatements=[];
function visit(node) {
  if (ts.isVariableStatement(node) && node.declarationList.declarations.some(d=>['newHeaders','headersToIterate'].includes(d.name.getText(how)))) headerStatements.push(node.getText(how));
  if (ts.isIfStatement(node) && node.expression.getText(how)==='headersToIterate') headerStatements.push(node.getText(how));
  ts.forEachChild(node,visit);
}
visit(how);assert.equal(headerStatements.length,3);
const projection=ts.transpileModule(headerStatements.join('\n')+'\nglobalThis.result=newHeaders;',{}).outputText;
const paths=[];
const server=createServer((request,response)=>{response.setHeader('Content-Type','application/json');response.end(JSON.stringify(request.headers));});
server.listen(0,'127.0.0.1');await once(server,'listening');
try {
  const values=[[' \tclean\r\n\0\u007fvalue\t ','cleanvalue','cleanvalue'],['a\tb','a\tb','a\tb'],['\uFEFFa','\uFEFFa','a'],['\u0085a','\u0085a','\u0085a'],['😃a','😃a','a']];
  for(const [raw,shared,direct] of values) for(const kline of [false,true]) {
    let rawNodeError=null;
    try {validateHeaderValue('X-Probe',raw);} catch(error) {rawNodeError=error.code;}
    signerValue=raw;
    context.createAPI({key:'key',secret:'secret',siteHeaders:{'X-Probe':raw}},{name:'bybit',isDemo:false});
    const request=await interceptor({method:'get',url:kline?'/v5/market/kline':'/v5/order/realtime',params:{},headers:{}});
    const header=kline?'X-Probe':'X-Signer';
    assert.equal(request.headers.get(header),shared);
    const tunnelContext=vm.createContext({config:request});vm.runInContext(projection,tunnelContext);
    assert.equal(tunnelContext.result[header],shared);
    // Loopback exercises the actual Axios Node adapter and Node HTTP, with proxy disabled.
    const response=await axios.get(`http://127.0.0.1:${server.address().port}/`,{headers:request.headers,proxy:false,adapter:'http',timeout:5000});
    assert.equal(response.data[header.toLowerCase()],direct);
    paths.push({raw,shared,direct,tunnel:tunnelContext.result[header],rawNodeError,kline});
  }
} finally {server.close();await once(server,'close');}
assert.equal(paths[0].rawNodeError,'ERR_INVALID_CHAR');
assert.equal(paths[2].rawNodeError,null);
process.stdout.write(JSON.stringify({cases,statuses,paths}));
