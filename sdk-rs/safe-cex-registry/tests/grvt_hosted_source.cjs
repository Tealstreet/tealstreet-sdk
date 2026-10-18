// Execute actual GRVT clients, the common factory and remote adapter with Axios.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),vm=require('node:vm');
const root=path.resolve(__dirname,'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const axios=require('axios');
const retry=require('axios-retry');
function load(file,imports={}) {
 const text=fs.readFileSync(path.join(root,'packages/safe-cex/src',file),'utf8');
 const code=ts.transpileModule(text,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
 const m={exports:{}};new Function('module','exports','require',code)(m,m.exports,name=>{assert.ok(Object.hasOwn(imports,name),`unreviewed dependency ${file}: ${name}`);return imports[name];});return m.exports;
}
const diagnostics={markRestSigned:()=>{},markRestRequestStart:()=>{},noteRestFailure:()=>{},noteRestSuccess:()=>{}};
const remote=load('remoteSigningAdapter.ts',{axios,nanoid:{nanoid:()=> 'source-id'},'./utils/tryParseJson':load('utils/tryParseJson.ts'),'./utils/venueRestDiagnostics':diagnostics});
const factory=load('safeCexAxiosCreate.ts',{axios,'./remoteSigningAdapter':remote,'./transport/registry':{getTransport:()=>({rest:({config})=>({baseURL:config.baseURL,proxied:false})})},'./utils/axiosErrorEnhancer':{enhanceAxiosError:e=>e},'./utils/isDevelopment':{isDevelopment:false},'./utils/signingCapture':{isSigningCaptureRequest:()=>false},'./utils/venueRestDiagnostics':diagnostics,'./utils/runtime':{isNodeOrBunRuntime:()=>true}});
const types=load('exchanges/grvt/grvt.types.ts');
const api=load('exchanges/grvt/grvt.api.ts',{'axios-retry':retry,'../../safeCexAxiosCreate':factory,'../../types':{LogSeverity:{}},'./grvtRetry':load('exchanges/grvt/grvtRetry.ts',{'./grvt.types':types}),'../../utils/retryReadsOnly':load('utils/retryReadsOnly.ts',{'axios-retry':retry}),'./grvt.types':types});
const file=path.join(root,'packages/safe-cex/src/exchanges/grvt/grvt.exchange.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='GrvtExchange');
const members=['getAccount','placeOrder'].map(name=>{const n=klass.members.find(n=>n.name?.getText(tree)===name);assert.ok(n,name);return n.getText(tree);});
const scope={ENDPOINTS:types.ENDPOINTS,LogSeverity:{Error:'error'},GrvtBuilderUnauthorizedError:class extends Error {}};
vm.runInNewContext(ts.transpileModule(`class Slice { ${members.join('\n')} } globalThis.Slice=Slice;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,scope);
(async()=>{
 const calls=[],wire=[];
 const signer={accountName:'grvt-hosted-account',sign:async request=>{calls.push(request);return {id:request.id,config:{...request.config,headers:{...request.config.headers,'X-Hosted-GRVT':'callback-proof'}}};}};
 const options={key:'hosted-api-key',secret:'',remoteSigner:signer};
 const exchange=new scope.Slice();Object.assign(exchange,{name:'grvt',grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit:()=>{}},log:()=>{},resolveCallerClientOrderId:()=>undefined,emitAndLogError:()=>{}});
 for(const name of ['createAPI','createEdgeAPI','createTradingAPI']) {
  const xhr=api[name](options,exchange);
  factory.setInnerAdapterForRemoteSigning(xhr,async config=>{wire.push({url:config.url,header:config.headers.get('X-Hosted-GRVT')});return {status:200,data:{r:{}},headers:{},config};});
  if(name==='createTradingAPI') { exchange.tradingXhr=xhr;assert.equal((await exchange.getAccount()).userId,''); }
  else await xhr.post('/source-hosted-probe',{});
 }
 await assert.rejects(exchange.placeOrder({}),{message:'Private key required for order placement - wallet not initialized'});
 assert.equal(calls.length,3,'all three GRVT clients install the provided signer');
 assert.equal(wire.length,3,'wallet guard must not send HTTP');
 assert.ok(wire.every(r=>r.header==='callback-proof'));
 process.stdout.write(JSON.stringify({calls:calls.map(r=>({exchangeName:r.exchangeName,accountName:r.accountName,config:r.config})),wire,walletError:'Private key required for order placement - wallet not initialized'}));
})().catch(e=>{console.error(e);process.exitCode=1;});
