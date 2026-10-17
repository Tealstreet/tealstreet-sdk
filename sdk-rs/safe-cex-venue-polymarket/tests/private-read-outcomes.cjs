// Execute the maintained factory with installed Axios; only transport/telemetry
// hosts are injected. No replacement factory or interceptor implementation.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||path.resolve(__dirname,'../../../node_modules/typescript'));
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
 for(const statuses of [[200],[200,200],[503]]){
  const outcomes=[];
  provider={rest:({config})=>({baseURL:config.baseURL,proxied:false})};
  const xhr=moduleValue.exports.safeCexAxiosCreate({baseURL:'https://venue',adapter:async config=>{
   const status=statuses[Number(config.url.slice(1))];
   const response={status,data:{},headers:{},config};
   if(status>=400)throw new axios.AxiosError('unavailable','ERR_BAD_RESPONSE',config,{},response);
   return response;
  }},{key:'',secret:''},{name:'polymarket',store:{recordRequestOutcome:outcome=>outcomes.push(outcome)},grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit:()=>{}}});
  for(let i=0;i<statuses.length;i++){try{await xhr.get('/'+i);}catch(error){assert.equal(error.response.status,503);}}
  rows.push(outcomes);
 }
 assert.deepEqual(rows,[['ok'],['ok','ok'],['other']]);
 process.stdout.write(JSON.stringify(rows));
})().catch(e=>{console.error(e);process.exitCode=1;});
