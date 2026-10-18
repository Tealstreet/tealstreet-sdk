// Actual KuCoin/Spot factories, common remote adapter, account and private WS bodies.
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
const remote=load('remoteSigningAdapter.ts',{axios,nanoid:{nanoid:()=> 'kucoin-source-id'},'./utils/tryParseJson':load('utils/tryParseJson.ts'),'./utils/venueRestDiagnostics':diagnostics});
const factory=load('safeCexAxiosCreate.ts',{axios,'./remoteSigningAdapter':remote,'./transport/registry':{getTransport:()=>({restAdapter:()=>undefined,rest:({config})=>({baseURL:config.baseURL,proxied:false})})},'./utils/axiosErrorEnhancer':{enhanceAxiosError:e=>e},'./utils/isDevelopment':{isDevelopment:false},'./utils/signingCapture':{isSigningCaptureRequest:()=>false},'./utils/venueRestDiagnostics':diagnostics,'./utils/runtime':{isNodeOrBunRuntime:()=>true}});
const types=load('types.ts',{'@tealstreet/logging/src/LogSeverity':load('../../logging/src/LogSeverity.ts')});
const constants=load('exchanges/kucoin/kucoin.types.ts',{'../../types':types});
const spotConstants=load('exchanges/kucoin/kucoinspot.types.ts',{'../../types':types});
const broker=load('exchanges/kucoin/kucoin.broker.ts',{axios,'create-hmac':createHmac});
const retryReads=load('utils/retryReadsOnly.ts',{'axios-retry':retry});
const capture={isSigningCaptureRequest:()=>false};
const rejection=load('utils/signatureRejection.ts',{'./signingCapture':capture});
const imports={axios,'axios-retry':retry,'create-hmac':createHmac,qs:actualRequire('qs'),'../../safeCexAxiosCreate':factory,'../../utils/retryReadsOnly':retryReads,'../../utils/signatureRejection':rejection,'../../utils/signingCapture':capture,'../../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1700000000123}},'./kucoin.broker':broker,'./kucoin.types':constants};
const api=load('exchanges/kucoin/kucoin.api.ts',imports);
const spotApi=load('exchanges/kucoin/kucoinspot.api.ts',{...imports,'./kucoin.api':api,'./kucoinspot.types':spotConstants});
const fixedDate=class extends Date {static now(){return 1700000000123;}};
const unused=()=>{throw Error('unexercised source dependency');};
const result=[];
(async()=>{
 for(const [name,klass,make,table,account,market,privateClass] of [
  ['kucoin','KucoinExchange',api.createAPI,constants,'/api/v1/user-info',constants.ENDPOINTS.MARKETS,'KucoinPrivateWebsocket'],
  ['kucoinspot','KucoinSpotExchange',spotApi.createKucoinSpotAPI,spotConstants,spotConstants.KUCOIN_SPOT_ENDPOINTS.ACCOUNT_SUMMARY,spotConstants.KUCOIN_SPOT_ENDPOINTS.MARKETS,'KucoinSpotPrivateWebsocket'],
 ]) {
  const calls=[],wire=[],frames=[],dials=[],errors=[];
  const accountName=`${name}-hosted-account`;
  const signer={accountName,async sign(request){calls.push(request);return {id:request.id,config:{...request.config,headers:{...request.config.headers,'X-Hosted-Kucoin':name}}};}};
  const options={key:'hosted-api-key',secret:'',passphrase:'',remoteSigner:signer};
  const file=path.join(root,`packages/safe-cex/src/exchanges/kucoin/${name}.exchange.ts`);
  const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
  const owner=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text===klass);
  const members=owner.members.filter(n=>['getAccount','cancelAllOrders'].includes(n.name?.getText(tree)));assert.equal(members.length,2);
  const scope={...table};
  vm.runInNewContext(ts.transpileModule(`class Slice {${members.map(n=>n.getText(tree)).join("\n")}} globalThis.Slice=Slice;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,scope);
  const exchange=new scope.Slice();Object.assign(exchange,{name,options,grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit(){}},log(){},emitAndLogError:message=>errors.push(String(message)),fetchAndSetPrivateSnapshot:unused});
  exchange.xhr=make(options,exchange);
  exchange.unlimitedXHR=make(options,exchange);
  assert.notEqual(exchange.xhr,exchange.unlimitedXHR);
  const wireAdapter=async config=>{
   wire.push({url:config.url,method:config.method,header:config.headers.get('X-Hosted-Kucoin')});
   const data=config.url===account?{userId:`${name}-hosted`,subUserId:'hosted-sub',referrerUserId:null}:config.url==='/api/v1/bullet-private'?{token:'hosted-private-token',instanceServers:[{endpoint:'wss://kucoin-hosted.invalid/private',pingInterval:5000}]}:[];
   return {status:200,data:{code:'200000',data},headers:{},config};
  };
  for(const xhr of [exchange.xhr,exchange.unlimitedXHR]) factory.setInnerAdapterForRemoteSigning(xhr,wireAdapter);
  assert.equal((await exchange.getAccount()).userId,`${name}-hosted`);
  await exchange.xhr.get(market);
  class BaseSocket {
   constructor(parent){this.parent=parent;this.isDisposed=false;}
   createWebsocket(url){dials.push(url);return {addEventListener(){}};}
   sendWithLog(text){frames.push(JSON.parse(text));}
   retryFailedConnect=unused;
  }
  const ws=load(`exchanges/kucoin/${name}.ws-private.ts`,{'@tealstreet/json-parse/src/jsonParse':{jsonParse:unused},'../../config/config':{ENABLE_RECONNECTING_WEBSOCKET:false},'../../types':types,'../base.ws':{BaseWebSocket:BaseSocket},[`./${name}.types`]:table},{Date:fixedDate,performance:{now:()=>123}});
  const socket=new ws[privateClass](exchange);
  await socket.connectAndSubscribe();socket.onOpen();
  await exchange.cancelAllOrders();
  assert.equal(calls.length,name==='kucoin'?5:4,`${name}: both HTTP clients and private token use hosted adapter`);
  assert.equal(wire.length,name==='kucoin'?5:4);assert.ok(wire.every(r=>r.header===name));
  assert.equal(dials.length,1);assert.equal(dials[0],'wss://kucoin-hosted.invalid/private?token=hosted-private-token&connectId=1700000000123');
  assert.equal(errors.length,0);assert.equal(frames.filter(f=>f.type==='subscribe').length,name==='kucoin'?6:2);
  assert.equal(frames.filter(f=>f.type==='ping').length,1);
  result.push({name,calls:calls.map(r=>({exchangeName:r.exchangeName,accountName:r.accountName,config:r.config})),wire,frames,dials,errors});
 }
 assert.equal(result.length,2,'completed source family census');
 process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,v8:process.versions.v8,cases:result}));
})().catch(error=>{console.error(error);process.exitCode=1;});
