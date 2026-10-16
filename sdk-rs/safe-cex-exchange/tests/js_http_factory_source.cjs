// Execute the maintained factory with installed Axios; only transport/telemetry
// hosts are injected. No replacement factory or interceptor implementation.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const axios=require('axios');
const root=path.resolve(__dirname,'../../..');
let provider;
const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/safeCexAxiosCreate.ts'),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
const imported={axios,'./remoteSigningAdapter':{createRemoteSigningAdapter:()=>{throw Error('unexpected source signer');}},'./transport/registry':{getTransport:()=>provider},'./utils/axiosErrorEnhancer':{enhanceAxiosError:e=>e},'./utils/isDevelopment':{isDevelopment:false},'./utils/signingCapture':{isSigningCaptureRequest:()=>false},'./utils/venueRestDiagnostics':{markRestRequestStart:()=>{},noteRestFailure:()=>{},noteRestSuccess:()=>{}},'./utils/runtime':{isNodeOrBunRuntime:()=>true}};
const moduleValue={exports:{}};
new Function('module','exports','require',code)(moduleValue,moduleValue.exports,name=>{assert.ok(Object.hasOwn(imported,name),'unexpected source dependency '+name);return imported[name];});
(async()=>{
 const rows=[];
 for(const proxied of [false,true])for(const failAuth of [false,true]){
  const events=[];
  provider={rest:({config})=>({baseURL:proxied?'https://proxy/'+config.baseURL:config.baseURL,proxied,authorize:async config=>{events.push('authorize');if(failAuth)throw Error('token rejected');config.headers.set('Proxy-Authorization','Bearer token');}})};
  let wire=null;
  const xhr=moduleValue.exports.safeCexAxiosCreate({baseURL:'https://venue',timeout:31,headers:{'X-Venue':'initial'},adapter:async config=>{events.push('send');wire={baseURL:config.baseURL,timeout:config.timeout,headers:config.headers.toJSON()};return {status:200,data:{ok:true},headers:{},config};}},{key:'',secret:''},{name:'polymarket',grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit:()=>{}}});
  xhr.interceptors.request.use(config=>{events.push('venue');config.headers.set('X-Venue','signed');return config;});
  let error=null;try{await xhr.get('/book');}catch(e){error=e.message;}
  rows.push({proxied,failAuth,events,wire,error});
 }
 assert.equal(rows.length,4);assert.deepEqual(rows[3].events,['venue','authorize']);
 process.stdout.write(JSON.stringify(rows));
})().catch(e=>{console.error(e);process.exitCode=1;});
