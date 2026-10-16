import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';import crypto from 'node:crypto';import {createRequire} from 'node:module';
const require=createRequire(import.meta.url),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=new URL('../../../',import.meta.url),[family]=process.argv.slice(2),all=JSON.parse(fs.readFileSync(new URL('../tests/fixtures/api-auth.json',import.meta.url)));
function tree(path){return ts.createSourceFile(path,fs.readFileSync(new URL(path,root),'utf8'),ts.ScriptTarget.Latest,true)}
function decl(t,name){const n=t.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(t)===name));assert.ok(n,name);return n.getText(t)}
const compile=code=>ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const norm=v=>v===undefined?{$undefined:true}:Array.isArray(v)?v.map(norm):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,v])=>[k,norm(v)])):v;
const families={futures_public:['binance','createPublicAPI'],futures:['binance','createAPI'],pm:['binancepm','createBinanceAPI'],papi:['binancepm','createPAPI'],fapi:['binancepm','createFAPI'],pm_public:['binancepm','createPublicAPI'],spot:['binancespot','createAPI']};
const results=[];
if(family==='auth'){
 const t=tree('packages/safe-cex/src/exchanges/binance/binancespot.ws-private.ts'),types=tree('packages/safe-cex/src/exchanges/binance/binancespot.types.ts'),cls=t.statements.find(n=>ts.isClassDeclaration(n)&&n.name.text==='BinancePrivateWebsocket'),member=cls.members.find(n=>n.name?.getText(t)==='buildUserDataSubscriptionParams');assert.equal(all.auth.length,13);
 for(const f of all.auth){const trace=[],exports={},context={exports,createHmac:crypto.createHmac};vm.createContext(context);
  vm.runInContext(compile(`${decl(types,'USER_DATA_RECV_WINDOW')}\n${decl(t,'BINANCE_SPOT_USER_DATA_REQUEST_ID')}\n${decl(t,'buildBinanceSpotUserDataSignaturePayload')}\nexports.Subject=class{${member.getText(t)}};exports.error=()=>new RangeError('api fixture');exports.isError=e=>e instanceof Error;`),context);
  const error=exports.error();error.code='api-code';error.response={status:503};const old={key:'key-A',secret:'secret-A'},fresh={key:'key-B',secret:'secret-B'},subject=new exports.Subject();let current=old;subject.parent={};Object.defineProperty(subject.parent,'options',{get(){trace.push(['options']);return current},set(v){current=v}});
  let response={wsAuthSignature:'remote-signature'};if(f.name==='remote-error')response={error:'signer denied',wsAuthSignature:'remote-signature'};if(f.name==='remote-missing')response={};if(f.name==='remote-empty')response={error:'',wsAuthSignature:''};if(f.name==='remote-null')response=null;if(f.name==='remote-number')response={wsAuthSignature:7};if(f.name==='remote-object')response={wsAuthSignature:{value:5}};
  const sign=request=>{trace.push(['remote',norm(request)]);if(f.name==='remote-sync-fail')throw error;return Promise.resolve().then(()=>{trace.push(['resolve']);if(f.name==='remote-reject')throw error;return response})};const signer={accountName:'account-A'};Object.defineProperty(signer,'sign',{get(){trace.push(['capture_remote']);return sign}});
  if(f.name.startsWith('remote'))old.remoteSigner=signer;
  context.virtualClock={getCurrentTimeMs(){trace.push(['clock']);if(f.name==='clock-fail')throw error;if(f.name==='replace-options')subject.parent.options=fresh;return 1700000000123}};
  context.createHmac=(algorithm,secret)=>{trace.push(['hmac',algorithm,secret]);if(f.name==='hmac-fail')throw error;return crypto.createHmac(algorithm,secret)};
  let value,failure;try{value=await subject.buildUserDataSubscriptionParams()}catch(e){failure=e}
  results.push({name:f.name,trace,value:norm(value),signatureRef:value?value.signature===response?.wsAuthSignature:null,failure:failure?{original:failure===error,name:failure.name,message:failure.message,code:norm(failure.code),response:norm(failure.response)}:null});
 }
}else{
 assert.ok(families[family]);assert.equal(all.factories.length,12);assert.equal(all.requests.length,10);assert.equal(all.responses.length,9);
 const [stem,method]=families[family],t=tree(`packages/safe-cex/src/exchanges/binance/${stem}.api.ts`),types=tree(`packages/safe-cex/src/exchanges/binance/${stem}.types.ts`),retry=tree('packages/safe-cex/src/utils/retryReadsOnly.ts'),signatures=tree('packages/safe-cex/src/utils/signatureRejection.ts'),remote=tree('packages/safe-cex/src/remoteSigningAdapter.ts');
 const names=stem==='binancepm'?['createBinanceAPI','createPAPI','createFAPI','createPublicAPI']:stem==='binance'?['createPublicAPI','createAPI']:['createAPI'];
 for(const f of all.factories){const trace=[],exports={},context={exports,isNetworkError:()=>true};vm.createContext(context);
  const code=[...(stem==='binancepm'?['BASE_PAPI_URL','BASE_FAPI_URL']:['BASE_URL']), 'ENDPOINTS','PUBLIC_ENDPOINTS','RECV_WINDOW'].map(n=>decl(types,n)).join('\n');
  vm.runInContext(compile(`${code}\n${['isReadOnlyMethod','onlyReads','networkErrorOnRead','readsAndPosted'].map(n=>decl(retry,n)).join('\n')}\n${['SIGNATURE_RETRY_CONFIG_KEY','isBinanceStyleSignatureRejection','hasRetriedSignature','signatureRejectedResponse','resendOnSignatureRejection'].map(n=>decl(signatures,n)).join('\n')}\n${decl(remote,'SKIP_REMOTE_SIGNING')}\n${decl(remote,'markSkipRemoteSigning').replace('export const markSkipRemoteSigning =','exports.markSkip =')}\n${names.map(n=>decl(t,n)).join('\n')}\nexports.error=()=>new RangeError('api fixture');`),context);
  const error=exports.error();error.code='api-code';error.response={status:503};const options={key:'key-A',secret:'secret-A'},exchange={name:stem},client={interceptors:{request:{use(handler){trace.push(['request_use']);if(f.name==='request-fail')throw error;client.request=handler}},response:{use(success,failure){trace.push(['response_use']);client.success=success;client.failure=failure}}}};
  if(f.name==='testnet')options.testnet=true;if(f.name==='recv-zero')options.extra={recvWindow:0};if(f.name==='recv-null')options.extra={recvWindow:null};if(f.name==='recv-string')options.extra={recvWindow:'2500'};if(['tunnel','install-fail'].includes(f.name))options.tealstreetTunnel=true;
  let config,mode,createRefs;function create(which){return(c,o,e)=>{trace.push(['create',which]);if(f.name==='create-fail')throw error;config=c;mode=which;createRefs=o===options&&e===exchange;return client}}
  context.safeCexAxiosCreate=create('proxy');context.safeCexAxiosCreateNoProxy=create('noProxy');
  context.bigIntAsStringJsonParse=()=>{};context.qs=require('qs');
  let retryReceipt;context.retry=(c,r)=>{trace.push(['retry']);const path=stem==='binance'?'/fapi/v1/listenKey':'/papi/v1/listenKey';const probes=[['GET','/private'],['head','/private'],['post',`${path}?x=1`],['put',path],['post','/private'],[undefined,path],['delete','/private']].map(([method,url])=>r.retryCondition({config:{method,url}}));retryReceipt={retries:r.retries,probes};assert.equal(c,client);if(f.name==='retry-fail')throw error};
  context.installTransportAdapter=(c,o,name,transform)=>{trace.push(['install',name,transform??'default']);assert.equal(c,client);assert.equal(o,options);if(f.name==='install-fail')throw error};
  let value,failure;try{value=exports[method](options,exchange,...(family==='pm'&&f.name==='pm-null'?[null]:[]))}catch(e){failure=e}
  const nativeConfig=config?{baseURL:config.baseURL,timeout:norm(config.timeout),headers:norm(config.headers),serializer:'repeat',query:config.paramsSerializer.serialize({symbol:['BTCUSDT','ETHUSDT'],nested:{v:'a b'},n:null}),transform:config.transformResponse===context.bigIntAsStringJsonParse?'bigInt':'default'}:null;
  const requests=[],responses=[];
  if(value){if(f.name==='live-options'){options.testnet=true;options.key='key-B';options.extra={recvWindow:9876};exchange.name='renamed'}
   for(const r of all.requests){const calls=[],nested={value:5},headers={private:'secret'},config={headers,timeout:500,nested};const ep=stem==='binancespot'?'/api/v3': '/fapi/v1';
    if(r.name==='listen')config.url=stem==='binancepm'?'/papi/v1/listenKey':`${ep}/listenKey`;else if(['public','headers-fail','skip-fail'].includes(r.name))config.url=`${ep}/exchangeInfo`;else if(r.name==='null-url')config.url=null;else if(r.name==='array-url')config.url=[`${ep}/exchangeInfo`];else if(r.name!=='missing-url')config.url='/private';
    context.AxiosHeaders={from(h){calls.push(['headers']);if(r.name==='headers-fail')throw error;return {...h}}};
    context.markSkipRemoteSigning=c=>{calls.push(['skip']);if(r.name==='skip-fail')throw error;return exports.markSkip(c)};
    let signRefs=null;context.signBinanceRequest=(c,o,name,window)=>{calls.push(['sign',name,norm(window)]);signRefs=c===config&&o===options;if(r.name==='sign-sync-fail')throw error;return Promise.resolve().then(()=>{calls.push(['resolve']);if(r.name==='sign-reject')throw error;return {...c,signed:true}})};
    let value,failure;try{value=await client.request(config)}catch(e){failure=e}
    requests.push({name:r.name,calls,value:norm(value),refs:value?{input:value===config,headers:value.headers===headers,nested:value.nested===nested}:null,signRefs,failure:failure?{original:failure===error,name:failure.name,message:failure.message,code:norm(failure.code),response:norm(failure.response)}:null});
   }
   if(client.success)for(const r of all.responses){const emitted=[],response={data:r.data},original={response},msg=r.data?.msg;
    exchange.emitAndLogError=m=>emitted.push(norm(m));for(const which of ['success','failure'].filter(which=>client[which])){let value,error;try{value=await client[which](which==='success'?response:original)}catch(e){error=e}
     responses.push({name:r.name,which,emitted:emitted.splice(0),value:norm(value),refs:value?value===response:null,error:error?{original:error===original,name:error.name??null,message:error.message??null,code:norm(error.code),responseRef:error.response===response}:null});}
   }
  }
  results.push({name:f.name,trace,config:nativeConfig,retry:retryReceipt??null,refs:!!value&&value===client&&createRefs,requests,responses,failure:failure?{original:failure===error,name:failure.name,message:failure.message,code:norm(failure.code),response:norm(failure.response)}:null});
 }
}
process.stdout.write(JSON.stringify(results));
