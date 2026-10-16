// Actual Backpack API + common remote HTTP adapter, and actual private WS/crypto.
'use strict';
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const {createRequire}=require('node:module');
const root=path.resolve(__dirname,'../../..');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const actualRequire=createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'../../packages/safe-cex/package.json'));
function load(file,imports={},globals={}) {
 const filename=path.join(root,'packages/safe-cex/src',file);
 const compiled=ts.transpileModule(fs.readFileSync(filename,'utf8'),{fileName:filename,reportDiagnostics:true,compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}});
 assert.equal(compiled.diagnostics.length,0,file);
 const m={exports:{}};
 vm.runInNewContext(compiled.outputText,{module:m,exports:m.exports,Buffer,TextEncoder,require(name){assert.ok(Object.hasOwn(imports,name),`unreviewed ${file}: ${name}`);return imports[name];},...globals},{filename});
 return m.exports;
}
const axios=require('axios'),retry=require('axios-retry');
const diagnostics={markRestSigned(){},markRestRequestStart(){},noteRestFailure(){},noteRestSuccess(){}};
const remote=load('remoteSigningAdapter.ts',{axios,nanoid:{nanoid:()=> 'backpack-source-id'},'./utils/tryParseJson':load('utils/tryParseJson.ts'),'./utils/venueRestDiagnostics':diagnostics});
const factory=load('safeCexAxiosCreate.ts',{axios,'./remoteSigningAdapter':remote,'./transport/registry':{getTransport:()=>({rest:({config})=>({baseURL:config.baseURL,proxied:false})})},'./utils/axiosErrorEnhancer':{enhanceAxiosError:e=>e},'./utils/isDevelopment':{isDevelopment:false},'./utils/signingCapture':{isSigningCaptureRequest:()=>false},'./utils/venueRestDiagnostics':diagnostics,'./utils/runtime':{isNodeOrBunRuntime:()=>true}});
const constants=load('exchanges/backpack/backpack.types.ts');
const ed=actualRequire('@noble/ed25519');
const signing=load('exchanges/backpack/backpack.signing.ts',{'@noble/ed25519':ed,'@noble/hashes/sha2.js':actualRequire('@noble/hashes/sha2.js'),'../../utils/virtualClock':{virtualClock:{getCurrentTimeMs:()=>1700000000123}}});
const api=load('exchanges/backpack/backpack.api.ts',{'axios-retry':retry,qs:require('qs'),'../../remoteSigningAdapter':remote,'../../safeCexAxiosCreate':factory,'../../utils/httpFailure':load('utils/httpFailure.ts'),'./backpack.signing':signing,'./backpack.types':constants,'../../utils/retryReadsOnly':load('utils/retryReadsOnly.ts',{'axios-retry':retry})});
const types=load('types.ts',{'@tealstreet/logging/src/LogSeverity':load('../../logging/src/LogSeverity.ts')});
class BaseSocket {constructor(parent){this.parent=parent;this.isDisposed=false;}}
const unused=()=>{throw Error('unused dependency');};
const Private=load('exchanges/backpack/backpack.ws-private.ts',{'../../types':types,'../../utils/createPositionId':{createPositionId:unused},'@tealstreet/json-parse/src/jsonParse':{jsonParse:unused},'../base.ws':{BaseWebSocket:BaseSocket},'./backpack.signing':signing,'./backpack.types':constants}).BackpackPrivateWebSocket;
const file=path.join(root,'packages/safe-cex/src/exchanges/backpack/backpack.exchange.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const klass=ast.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='BackpackExchange');
const members=klass.members.filter(n=>n.name?.getText(ast)==='getAccount');assert.equal(members.length,1);
const scope={ENDPOINTS:constants.ENDPOINTS,LogSeverity:types.LogSeverity};
vm.runInNewContext(ts.transpileModule(`class Slice {${members[0].getText(ast)}} globalThis.Slice=Slice;`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText,scope);
(async()=>{
 const calls=[],wire=[];
 const signer={accountName:'backpack-hosted-account',sign:async request=>{calls.push(request);return {id:request.id,config:{...request.config,headers:{...request.config.headers,'X-Hosted-Backpack':'callback-proof'}}};}};
 const options={key:'hosted-public-key',secret:'',remoteSigner:signer};
 const exchange=new scope.Slice();Object.assign(exchange,{name:'backpack',options,grafanaEndpointsEvery:[],grafanaEndpointsPeriodic:[],emitter:{emit(){}},log(){},emitAndLogError:unused});
 exchange.xhr=api.createAPI(options,exchange);
 factory.setInnerAdapterForRemoteSigning(exchange.xhr,async config=>{wire.push({url:config.url,header:config.headers.get('X-Hosted-Backpack')??null});return {status:200,data:config.url===constants.ENDPOINTS.ACCOUNT?{accountId:'hosted-backpack'}:[],headers:{},config};});
 assert.equal((await exchange.getAccount()).userId,'hosted-backpack');
 await exchange.xhr.get(constants.ENDPOINTS.MARKETS);
 assert.equal(calls.length,1,'private HTTP signs; public HTTP skips');assert.equal(wire.length,2);
 assert.equal(wire[0].header,'callback-proof');assert.equal(wire[1].header,null);
 const seed=Buffer.alloc(32,7),publicKey=await ed.getPublicKeyAsync(seed),key=Buffer.from(publicKey).toString('base64');
 const cases=[];
 for(const local of [false,true])for(const method of ['authenticate','subscribeToOrderUpdates','subscribeToPositionUpdates']) {
  let reads=0,signCalls=0;const errors=[],frames=[];
  const opts={key,secret:local?seed.toString('base64'):''};
  Object.defineProperty(opts,'remoteSigner',{get(){reads++;return {accountName:'backpack-ws-account',sign(){signCalls++;throw Error('WS must use source local signer');}};}});
  const socket=new Private({options:opts,emitAndLogError:message=>errors.push(message)});
  socket.send=message=>frames.push(JSON.parse(message));
  await socket[method]();
  assert.equal(reads,0);assert.equal(signCalls,0);
  assert.equal(frames.length,local?1:0);assert.equal(errors.length,local?0:1);
  if(local) {
   const signature=frames[0].signature;assert.equal(signature.length,4);
   assert.deepEqual(signature.slice(2),['1700000000123','5000']);
   assert.ok(await ed.verifyAsync(Buffer.from(signature[1],'base64'),Buffer.from('instruction=subscribe&timestamp=1700000000123&window=5000'),publicKey));
  } else assert.match(errors[0],/RangeError:.*secretKey.*32.*length=0/);
  cases.push({local,method,reads,signCalls,errors,frames});
 }
 assert.equal(cases.length,6,'completed WS source cases');
 process.stdout.write(JSON.stringify({node:process.version,major:Number(process.versions.node.split('.')[0]),arch:process.arch,v8:process.versions.v8,calls:calls.map(r=>({exchangeName:r.exchangeName,accountName:r.accountName,config:r.config})),wire,cases}));
})().catch(error=>{console.error(error);process.exitCode=1;});
