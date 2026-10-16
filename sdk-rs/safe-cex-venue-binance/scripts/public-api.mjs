// Executes the actual venue factory, common Axios factory, retry policy and parser.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const runtimeRequire=process.env.SAFE_CEX_TYPESCRIPT_PATH?name=>require(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..',name)):require;
const axios=runtimeRequire('axios'),retryModule=runtimeRequire(process.argv.includes('--missing-retry-import')?'axios-retry-deliberately-absent':'axios-retry'),qs=runtimeRequire('qs');
const root=new URL('../../../packages/',import.meta.url);
function source(file,deps,extra={}){
 const exports={};vm.runInNewContext(ts.transpileModule(fs.readFileSync(new URL(file,root),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText,{exports,require:name=>{assert.ok(name in deps,`unexpected import ${name}`);return deps[name];},Error,Date,console,Promise,Map,WeakMap,...extra});return exports;
}
function declaration(file,name){const ast=ts.createSourceFile(file,fs.readFileSync(new URL(file,root),'utf8'),ts.ScriptTarget.Latest,true);const stmt=ast.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(ast)===name));assert.ok(stmt,name);return stmt.getText(ast).replace(/^export /,'');}
function tag(value){if(typeof value==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(value);return {numberBits:b.toString('hex')};}if(value===undefined)return {undefined:true};if(Array.isArray(value))return value.map(tag);if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,tag(v)]));return value;}
const tryParse=source('safe-cex/src/utils/tryParseJson.ts',{}).tryParseJson;
assert.equal(typeof tryParse('123'),'string','real tryParseJson retains primitive text');
assert.equal(typeof tryParse('{"x":1}'),'object','real tryParseJson restores object text');
const vectors=JSON.parse(fs.readFileSync(0,'utf8')),results=[];
const originalAdapter=axios.defaults.adapter;
try{for(const vector of vectors){
 const trace=[],outcomes=[],diagnostics=[],transportCalls=[];let sends=0,remoteCalls=0;
 const wire=async config=>{
  sends++;trace.push({url:axios.getUri(config),method:config.method,headers:Object.entries(config.headers.toJSON(true)),timeout:config.timeout,skip:config.safeCexSkipRemoteSigning===true,body:tag(config.data)});
  if(sends<=(vector.networkFailures??0))throw new axios.AxiosError('fixture network','ERR_NETWORK',config,{});
  const response={data:vector.responseText??'{"ok":true}',status:vector.status??200,statusText:'Fixture',headers:{'x-fixture':'yes'},config};
  if(response.status>=400)throw new axios.AxiosError('fixture HTTP','ERR_BAD_RESPONSE',config,{},response);
  return response;
 };
 axios.defaults.adapter=wire;
 const diagnosticsModule={markRestRequestStart(){diagnostics.push('start');},markRestSigned(){throw Error('public request signed');},noteRestFailure(){diagnostics.push('failure');},noteRestSuccess(){diagnostics.push('success');}};
 const remote=source('safe-cex/src/remoteSigningAdapter.ts',{'axios':axios,'nanoid':{nanoid(){throw Error('public signer ID generated');}},'./utils/tryParseJson':source('safe-cex/src/utils/tryParseJson.ts',{}),'./utils/venueRestDiagnostics':diagnosticsModule});
  const parser=source('json-parse/src/jsonParse.ts',{});
 // Execute the production HOW adapter, replacing only its socket/clock I/O.
 class RecordingSocket {
  isInitialized=true;isOpen=true;isTimeSynced=true;authState='good';
  listeners=new Map();
  once(key,callback){this.listeners.set(key,callback);}
  off(key){this.listeners.delete(key);}
  send(frame){
   assert.equal(frame.type,'rpc.http');
   trace.push({tunnel:true,request:tag(frame.arg)});
   const callback=this.listeners.get('sub:'+frame.nonce);assert.ok(callback);
   void callback({status:vector.status??200,headers:{'x-fixture':'yes'},data_raw:vector.responseText??'{"ok":true}'});
  }
 }
 const tunnel=source('http-over-websocket/src/axiosHttpOverWebsocketAdapter.ts',{
  axios,nanoid:{nanoid:()=> 'fixture-nonce'},
  '@tealstreet/compression/src/decompress':{decompress(){throw Error('unexpected compressed fixture');}},
  '@tealstreet/sleep/src/sleep':{sleep(){throw Error('unexpected tunnel sleep');}},
  '@tealstreet/json-parse/src/jsonParse':parser,
  './config':{getProxyToken(){throw Error('unexpected token');},isTokenExpired(){throw Error('unexpected expiry');},shouldSimulateHttpOverWebsocket:()=>false},
  './tunnelEndpoints':{SINGAPORE_HOW_EGRESS:'fixture-sg',TOKYO_HOW_EGRESS:'fixture-ja'},
  './egressRegion':{isTokyoEgress:()=>false},'./HOWWebSocket':{HOWWebSocket:RecordingSocket},
  './types':{HOWWebSocketAuthState:{Good:'good',Bad:'bad'}}
 },{setTimeout,clearTimeout,FormData,URLSearchParams,ArrayBuffer,Uint8Array});
 const common=source('safe-cex/src/safeCexAxiosCreate.ts',{'axios':axios,'./remoteSigningAdapter':remote,'./transport/registry':{getTransport(){return {restAdapter(context){transportCalls.push({exchange:context.exchange,variant:context.variant});return vector.installAdapter===false?undefined:vector.productionTunnel?tunnel.bigIntAxiosHttpOverWebsocketAdapter:wire;}};}},'./utils/axiosErrorEnhancer':source('safe-cex/src/utils/axiosErrorEnhancer.ts',{'axios':axios}),'./utils/isDevelopment':{isDevelopment:false},'./utils/signingCapture':source('safe-cex/src/utils/signingCapture.ts',{}),'./utils/venueRestDiagnostics':diagnosticsModule,'./utils/runtime':{isNodeOrBunRuntime:()=>true}});
 const reads=source('safe-cex/src/utils/retryReadsOnly.ts',{'axios-retry':retryModule});
 const body=['BASE_URL','ENDPOINTS','RECV_WINDOW'].map(name=>declaration('safe-cex/src/exchanges/binance/binance.types.ts',name)).join('\n')+'\n'+declaration('safe-cex/src/exchanges/binance/binance.api.ts','createPublicAPI')+'\nexports.createPublicAPI=createPublicAPI;';
 const exports={};vm.runInNewContext(ts.transpileModule(body,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,...common,...parser,...reads,...remote,AxiosHeaders:axios.AxiosHeaders,retry:retryModule.default,qs});
 const options={...vector.options};if(vector.remote)options.remoteSigner={async sign(){remoteCalls++;throw Error('public remote sign');}};
 const exchange={name:'binance',store:{recordRequestOutcome(value){outcomes.push(value);}},noteHttpResponse(){diagnostics.push('response');}};
 const client=exports.createPublicAPI(options,exchange);
 let result=null,error=null;
 try{const response=await client.request(vector.request);result={status:response.status,dataType:typeof response.data,data:tag(response.data)};}catch(e){error={code:e.code??null,message:e.message,data:tag(e.response?.data)};}
 results.push({trace,outcomes,diagnostics,transportCalls,remoteCalls,stats:common.httpStats.getStats(),result,error});
}}finally{axios.defaults.adapter=originalAdapter;}
process.stdout.write(JSON.stringify(results));
