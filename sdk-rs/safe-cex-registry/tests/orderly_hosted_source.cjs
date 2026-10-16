// Actual Orderly API/common adapter, account body and private WS; normalized API inputs.
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
 vm.runInNewContext(compiled.outputText,{module:m,exports:m.exports,Buffer,TextEncoder,global:{},require(name){assert.ok(Object.hasOwn(imports,name),`unreviewed ${file}: ${name}`);return imports[name];},...globals},{filename});
 return m.exports;
}
const axios=actualRequire('axios'),retry=actualRequire('axios-retry'),createHmac=actualRequire('create-hmac');
const diagnostics={markRestSigned(){},markRestRequestStart(){},noteRestFailure(){},noteRestSuccess(){}};
const remote=load('remoteSigningAdapter.ts',{axios,nanoid:{nanoid:()=> 'orderly-source-id'},'./utils/tryParseJson':load('utils/tryParseJson.ts'),'./utils/venueRestDiagnostics':diagnostics});
const factory=load('safeCexAxiosCreate.ts',{axios,'./remoteSigningAdapter':remote,'./transport/registry':{getTransport:()=>({restAdapter:()=>undefined,rest:({config})=>({baseURL:config.baseURL,proxied:false})})},'./utils/axiosErrorEnhancer':{enhanceAxiosError:e=>e},'./utils/isDevelopment':{isDevelopment:false},'./utils/signingCapture':{isSigningCaptureRequest:()=>false},'./utils/venueRestDiagnostics':diagnostics,'./utils/runtime':{isNodeOrBunRuntime:()=>true}});
const types=load('types.ts',{'@tealstreet/logging/src/LogSeverity':load('../../logging/src/LogSeverity.ts')});
const constants=load('exchanges/orderly/orderly.types.ts',{'../../types':types});
const ed=actualRequire('@noble/ed25519');
const bs58=actualRequire('bs58').default;
const encryption=load('exchanges/orderly/orderly.encryption.ts',{'@noble/ed25519':ed,'@noble/hashes/sha2.js':actualRequire('@noble/hashes/sha2.js'),bs58});
const retryReads=load('utils/retryReadsOnly.ts',{'axios-retry':retry});
const api=load('exchanges/orderly/orderly.api.ts',{axios,'axios-retry':retry,qs:actualRequire('qs'),'../../safeCexAxiosCreate':factory,'../../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1700000000123}},'../../utils/retryReadsOnly':retryReads,'./orderly.encryption':encryption,'./orderly.types':constants});
const unused=()=>{throw Error('unexercised source dependency');};
class BaseSocket {constructor(parent){this.parent=parent;this.isDisposed=false;this.dials=[];}createWebsocket(url){this.dials.push(url);return {addEventListener(){}};}sendWithLog(text){this.frames.push(JSON.parse(text));}trackLastReceived(){}emitWsDiagnosticReceive(){}}
const Private=load('exchanges/orderly/orderly.ws-private.ts',{'@tealstreet/json-parse/src/jsonParse':load('../../json-parse/src/jsonParse.ts'),'../../config/config':{ENABLE_RECONNECTING_WEBSOCKET:false},'../../types':types,'../../utils/get-key':{v:unused},'../../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1700000000123}},'../base.ws':{BaseWebSocket:BaseSocket},'./orderly.encryption':encryption,'./orderly.types':constants,'./orderly.utils':{normalizeSymbol:unused}},{performance:{now:()=>123}}).OrderlyPrivateWebSocket;
const filename=path.join(root,'packages/safe-cex/src/exchanges/orderly/orderly.exchange.ts');
const tree=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const owner=tree.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='OrderlyExchange');
const members=owner.members.filter(n=>n.name?.getText(tree)==='getAccount');assert.equal(members.length,1);
const scope={ENDPOINTS:constants.ENDPOINTS};
vm.runInNewContext(ts.transpileModule(`class Slice {${members[0].getText(tree)}} globalThis.Slice=Slice;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,scope);
(async()=>{
 const cases=[],seed=Buffer.alloc(32,17),publicKey=await ed.getPublicKeyAsync(seed);
 for(const name of ['kodiakfi','mode','salsadex','tealstreet-dex','what','woofi'])for(const local of [true,false]) {
  const calls=[],wire=[];let reads=0;
  const signer={accountName:`${name}-hosted-account`,async sign(request){assert.equal(request.type,'http','Orderly WS never calls hosted signer');calls.push(request);return {id:request.id,config:{...request.config,headers:{...request.config.headers,'X-Hosted-Orderly':name}}};}};
  // This probe enters at the API/socket boundary: valid credentials already carry
  // the source constructor's prefix; its falsy empty secret remains unchanged.
  const options={key:'ed25519:hosted-public-key',secret:local?`ed25519:${bs58.encode(seed)}`:'',applicationId:'hosted-application'};
  Object.defineProperty(options,'remoteSigner',{enumerable:true,get(){reads++;return signer;}});
  const exchange=new scope.Slice();Object.assign(exchange,{name,options,grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit(){}},log(){},fetchAndSetPrivateSnapshot:unused});
  exchange.xhr=api.createAPI(options,exchange);
  factory.setInnerAdapterForRemoteSigning(exchange.xhr,async config=>{wire.push({url:config.url,method:config.method,header:config.headers.get('X-Hosted-Orderly'),headers:config.headers.toJSON()});return {status:200,data:config.url===constants.ENDPOINTS.ACCOUNT?{success:true,data:{account_id:'orderly-hosted'}}:{success:true,data:{rows:[]}},headers:{},config};});
  let account,error;
  try {account=await exchange.getAccount();}catch(e){error={name:e.name,message:e.message};}
  if(local){assert.equal(error,undefined,JSON.stringify(error));assert.equal(account.userId,'orderly-hosted');assert.equal(account.affiliateId,'orderly-hosted');}else assert.deepEqual(error,{name:'TypeError',message:'Expected String'});
  await exchange.xhr.get(constants.ENDPOINTS.MARKETS);
  assert.equal(calls.length,local?2:1,'private requires local signing before hosted adapter; public still hosted');
  assert.equal(wire.length,local?2:1);assert.ok(wire.every(r=>r.header===name));
  if(local){const headers=calls[0].config.headers;assert.equal(headers['orderly-textsign'],'1700000000123GET/v1/client/info');assert.equal(wire[0].headers['orderly-textsign'],headers['orderly-textsign']);assert.equal(wire[0].headers['orderly-signature'],headers['orderly-signature']);assert.ok(await ed.verifyAsync(Buffer.from(headers['orderly-signature'],'base64url'),Buffer.from(headers['orderly-textsign']),publicKey));}
  const before=reads,socket=new Private(exchange);socket.frames=[];let wsError;
  socket.connectAndSubscribe();assert.equal(socket.dials.length,1);
  try {socket.onOpen();}catch(e){wsError={name:e.name,message:e.message};}
  assert.equal(reads,before,'private WS never reads remoteSigner');assert.equal(socket.frames.length,local?1:0);
  if(local){const auth=socket.frames[0];assert.equal(auth.event,'auth');assert.equal(auth.params.orderly_key,options.key);assert.equal(auth.params.timestamp,1700000000123);assert.ok(await ed.verifyAsync(Buffer.from(auth.params.sign,'base64url'),Buffer.from('1700000000123'),publicKey));socket.onMessage({data:JSON.stringify({event:'auth',success:true})});assert.equal(socket.frames.length,5);}
  else assert.deepEqual(wsError,{name:'TypeError',message:'Expected String'});
  cases.push({name,local,account,error,calls:calls.map(({id,...rest})=>rest),wire,frames:socket.frames,dials:socket.dials,wsError,wsRemoteReads:reads-before});
 }
 assert.equal(cases.length,12,'six names times valid/empty source secrets');
 process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,v8:process.versions.v8,encodedSeed:bs58.encode(seed),cases}));
})().catch(error=>{console.error(error);process.exitCode=1;});
