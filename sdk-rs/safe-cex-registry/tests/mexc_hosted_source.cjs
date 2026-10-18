// Actual MEXC API, common remote HTTP adapter and actual private WS login bodies.
'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const {createRequire}=require('node:module');
const root=path.resolve(__dirname,'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const actualRequire=createRequire(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH));
function load(file,imports={},globals={}) {
 const filename=path.join(root,'packages/safe-cex/src',file);
 const compiled=ts.transpileModule(fs.readFileSync(filename,'utf8'),{fileName:filename,reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}});
 assert.equal(compiled.diagnostics.length,0,file);
 const m={exports:{}};
 vm.runInNewContext(compiled.outputText,{module:m,exports:m.exports,Buffer,TextEncoder,require(name){assert.ok(Object.hasOwn(imports,name),`unreviewed ${file}: ${name}`);return imports[name];},...globals},{filename});
 return m.exports;
}
const axios=actualRequire('axios'),retry=actualRequire('axios-retry'),createHmac=actualRequire('create-hmac');
const diagnostics={markRestSigned(){},markRestRequestStart(){},noteRestFailure(){},noteRestSuccess(){}};
const remote=load('remoteSigningAdapter.ts',{axios,nanoid:{nanoid:()=> 'mexc-source-id'},'./utils/tryParseJson':load('utils/tryParseJson.ts'),'./utils/venueRestDiagnostics':diagnostics});
const factory=load('safeCexAxiosCreate.ts',{axios,'./remoteSigningAdapter':remote,'./transport/registry':{getTransport:()=>({restAdapter:()=>undefined,rest:({config})=>({baseURL:config.baseURL,proxied:false})})},'./utils/axiosErrorEnhancer':{enhanceAxiosError:e=>e},'./utils/isDevelopment':{isDevelopment:false},'./utils/signingCapture':{isSigningCaptureRequest:()=>false},'./utils/venueRestDiagnostics':diagnostics,'./utils/runtime':{isNodeOrBunRuntime:()=>true}});
const types=load('types.ts',{'@tealstreet/logging/src/LogSeverity':load('../../logging/src/LogSeverity.ts')});
const constants=load('exchanges/mexc/mexc.types.ts',{'../../types':types});
const unused=()=>{throw Error('unexercised source dependency');};
const capture={isSigningCaptureRequest:()=>false};
const retryReads=load('utils/retryReadsOnly.ts',{'axios-retry':retry});
const rejection=load('utils/signatureRejection.ts',{'./signingCapture':capture});
const api=load('exchanges/mexc/mexc.api.ts',{axios,'axios-retry':retry,'create-hmac':createHmac,qs:actualRequire('qs'),'@tealstreet/json-parse/src/jsonParse':load('../../json-parse/src/jsonParse.ts'),'../../remoteSigningAdapter':remote,'../../safeCexAxiosCreate':factory,'../../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1700000000123}},'../../utils/retryReadsOnly':retryReads,'../../utils/signatureRejection':rejection,'./mexc.types':constants});
const results=[];
(async()=>{
 for(const mode of ['success','error','missing']) {
  const calls=[],wire=[],frames=[],errors=[];
  const signer={accountName:'mexc-hosted-account',async sign(request){calls.push(request);return request.type==='http'?{id:request.id,config:{...request.config,headers:{...request.config.headers,'X-Hosted-Mexc':'callback-proof'}}}:mode==='success'?{id:request.id,wsAuthSignature:'hosted-ws-signature'}:mode==='error'?{id:request.id,error:'signer-offline'}:{id:request.id};}};
  const options={key:' hosted-api-key ',secret:'',remoteSigner:signer};
  const exchange={name:'mexc',options,grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit(){}},log(){},emitAndLogError:message=>errors.push(String(message))};
  const xhr=api.createAPI(options,exchange);
  factory.setInnerAdapterForRemoteSigning(xhr,async config=>{wire.push({url:config.url,method:config.method,header:config.headers.get('X-Hosted-Mexc')??null});return {status:200,data:JSON.stringify({success:true,code:0,data:[]}),headers:{},config};});
  await xhr.get(constants.ENDPOINTS.ASSETS);await xhr.get(constants.ENDPOINTS.CONTRACT_DETAIL);
  class BaseSocket {constructor(parent){this.parent=parent;this.isDisposed=false;}send(text){frames.push(JSON.parse(text));}emitWsDiagnosticSend(){}}
  const ws=load('exchanges/mexc/mexc.ws-private.ts',{nanoid:{nanoid:()=> 'mexc-ws-source-id'},'@tealstreet/json-parse/src/jsonParse':load('../../json-parse/src/jsonParse.ts'),'../../types':types,'../../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1700000000123}},'../base.ws':{BaseWebSocket:BaseSocket},'./mexc.api':api,'./mexc.types':constants,'./mexc.utils':{fromMexcSymbol:unused,normalizeExecution:unused,normalizeOrder:unused,normalizePlanOrder:unused,normalizePosition:unused,normalizeStopOrder:unused,toIntegerString:unused,toNumber:unused}});
  const socket=new ws.MexcPrivateWebsocket(exchange);
  await socket.sendLogin();
  assert.equal(calls.length,2,'one private HTTP and one WS RPC');assert.equal(wire.length,2,'public HTTP bypass');
  assert.equal(wire[0].header,'callback-proof');assert.equal(wire[1].header,null);
  assert.equal(calls[1].wsAuth.toSign,'hosted-api-key1700000000123');
  assert.equal(calls[1].accountName,'mexc-hosted-account');assert.equal(calls[1].exchangeName,'mexc');
  assert.equal(frames.length,mode==='success'?1:0);assert.equal(errors.length,mode==='success'?0:1);
  if(mode==='success'){assert.deepEqual(frames[0],{method:'login',param:{apiKey:'hosted-api-key',reqTime:'1700000000123',signature:'hosted-ws-signature'}});socket.subscribePrivateFeeds();assert.equal(frames.length,2);assert.equal(frames[1].param.filters.length,7);}
  else assert.equal(errors[0],`Remote WS signing unavailable: ${mode==='error'?'signer-offline':'no signature returned'}`);
  results.push({mode,calls:calls.map(({id,...rest})=>rest),wire,frames,errors});
 }
 assert.equal(results.length,3,'source signing outcome census');
 process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,v8:process.versions.v8,cases:results}));
})().catch(error=>{console.error(error);process.exitCode=1;});
