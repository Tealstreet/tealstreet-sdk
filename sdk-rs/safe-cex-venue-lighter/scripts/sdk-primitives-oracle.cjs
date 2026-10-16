'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const root=path.resolve(__dirname,'../../..');
function compile(file){const r=ts.transpileModule(fs.readFileSync(path.join(root,file),'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(r.diagnostics.length,0,'source transpile diagnostics');return r.outputText;}
const units=v=>Array.from({length:v.length},(_,i)=>v.charCodeAt(i));
function tag(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')}}if(typeof v==='bigint')return{kind:'bigint',decimal:v.toString()};if(typeof v==='string')return{kind:'string',units:units(v)};if(typeof v==='boolean')return{kind:'boolean',value:v};if(typeof v==='symbol')return{kind:'symbol'};if(typeof v==='function')return{kind:'function'};return{kind:'object'};}
const constants={};new Function('exports',compile('packages/lighter-sdk/src/types/constants.ts'))(constants);
let clocks=0,clockValue=1700000000987,clockThrows=false;const token=new Error('host\ud800');
const validation={};new Function('exports','require','Date',compile('packages/lighter-sdk/src/utils/validation.ts'))(validation,p=>{assert.equal(p,'../types');return constants},{now(){clocks++;if(clockThrows)throw token;return clockValue;}});
const validators=['validateAccountIndex','validateApiKeyIndex','validateNonce','validateOrderId','validateMarketId','validatePrice','validateAmount','validateOrderParams','validatePrivateKey','validatePublicKey','validateAddress','validateDeadline','validateChainId'];
assert.deepEqual(Object.keys(validation).sort(),['ValidationError',...validators].sort(),'validation runtime export floor');
const cases=[];
function run(method,args,extra={}){clocks=0;clockValue=extra.clock??1700000000987;clockThrows=!!extra.clockThrows;let output;try{validation[method](...args);output={kind:'ok'}}catch(e){output=e===token?{kind:'thrown'}:{kind:e instanceof validation.ValidationError?'validation':'builtin',name:units(e.name),message:units(e.message)}}cases.push({method,args:args.map(tag),...(Object.hasOwn(extra,'clock')?{clock:tag(extra.clock)}:{}),...(extra.clockThrows?{clockThrows:true}:{}),output,clocks});}
const wrong=[undefined,null,true,false,'','x\ud800',Symbol('s'),()=>0,{},[]];
const bigs=[-1n,0n,1n,255n,256n,4294967295n,4294967296n,18446744073709551615n,18446744073709551616n,1n<<200n,-(1n<<200n)];
for(const method of ['validateAccountIndex','validateNonce','validateOrderId','validatePrice','validateAmount'])for(const v of [...wrong,0,1,NaN,...bigs])run(method,[v]);
const numbers=[-Infinity,-1,-0,0,1,255,256,1.25,NaN,Infinity,Number.MAX_SAFE_INTEGER+1,Number.MAX_VALUE,Number.MIN_VALUE,1e21];
for(const method of ['validateApiKeyIndex','validateMarketId','validateChainId'])for(const v of [...wrong,1n,...numbers])run(method,[v]);
const notString=[undefined,null,true,false,0,-0,NaN,Infinity,1n,Symbol('s'),()=>0,{},[]];
const strings=['','0x','0X'+'a'.repeat(64),'0x'+'a'.repeat(64),'0x'+'0123456789aAbBcCdDeEfF'.repeat(4).slice(0,64),'0x'+'a'.repeat(63),'0x'+'a'.repeat(65),'0x'+'g'.repeat(64),'0x'+'a'.repeat(63)+'\ud800','0x'+'👩'.repeat(32),'0x'+'a'.repeat(40),'0x'+'a'.repeat(39)+'\n','0x'+'a'.repeat(64)+'\n',' 0x'+'a'.repeat(40)];
for(const method of ['validatePrivateKey','validatePublicKey','validateAddress'])for(const v of [...notString,...strings])run(method,[v]);
for(const v of [...wrong,0,1,NaN,0n,-1n,1700000059n,1700000060n,1n<<200n])run('validateDeadline',[v]);
for(const minFuture of [-0,0,60,60.5,NaN,Infinity,-60,1e20])run('validateDeadline',[1700000060n,minFuture]);
for(const clock of [-1500,-1,-0,Infinity,NaN])run('validateDeadline',[1n,0],{clock});
run('validateDeadline',[1700000060n],{clockThrows:true});
run('validateDeadline',[59n,60],{clock:-1});run('validateDeadline',[58n,60],{clock:-1500});
assert.equal(cases.length,310,'independent scalar case inventory'); //120+75+81+34
const orderCases=[];const base={orderId:1n,marketId:17,isBuy:true,price:31n,amount:41n};
function orderCase(fields={},sequences={},throwAt){const values={...base,...fields},trace=[],counts={};const receiver={};for(const key of ['orderId','marketId','isBuy','price','amount','postOnly','reduceOnly'])Object.defineProperty(receiver,key,{get(){trace.push(key);const n=counts[key]=(counts[key]||0)+1;if(throwAt?.field===key&&throwAt.occurrence===n)throw token;const s=sequences[key];return s?s[Math.min(n-1,s.length-1)]:values[key];}});let output;try{validation.validateOrderParams(receiver);output={kind:'ok'}}catch(e){output=e===token?{kind:'thrown'}:{kind:'validation',name:units(e.name),message:units(e.message)}}orderCases.push({fields:Object.fromEntries(Object.entries(values).map(([k,v])=>[k,tag(v)])),sequences:Object.fromEntries(Object.entries(sequences).map(([k,v])=>[k,v.map(tag)])),throwAt,trace,output});}
orderCase();orderCase({postOnly:false,reduceOnly:true});orderCase({postOnly:true,reduceOnly:true});
for(const field of ['orderId','marketId','price','amount'])for(const value of [undefined,null,false,'',NaN])orderCase({[field]:value});
for(const [field,value]of [['orderId',0n],['marketId',-0],['price',0n],['amount',0n],['isBuy',1],['postOnly',null],['reduceOnly','yes']])orderCase({[field]:value});
orderCase({}, {orderId:[0n,1n]});orderCase({}, {orderId:[0n,0n,3n]});orderCase({}, {orderId:[3n,-1n]});
orderCase({}, {postOnly:[undefined,true],reduceOnly:[undefined,true]});orderCase({}, {postOnly:[true,false,true],reduceOnly:[false,false,true]});
for(const [field,occurrence]of [['orderId',1],['orderId',2],['marketId',1],['isBuy',1],['price',2],['amount',2],['postOnly',1],['postOnly',2],['reduceOnly',1]])orderCase({postOnly:true,reduceOnly:false},{},{field,occurrence});
assert.equal(orderCases.length,44,'independent accessor case inventory');
const errorCases=['','message-A','name\ud800👩'].map(message=>{const e=new validation.ValidationError(message);return{message:units(message),name:units(e.name),nativeMessage:units(e.message),isError:e instanceof Error,isValidationError:e instanceof validation.ValidationError,hasStack:typeof e.stack==='string'};});
// Capture actual api-client helper declarations without adopting its Axios runtime.
const helpers={};new Function('exports','require',compile('packages/lighter-sdk/src/client/api-client.ts')+'\nexports.__helpers={toSafeNumber,isSafeIntegerBigInt};')(helpers,name=>name==='axios'?{default:{}}:name==='./http-types'?{}:assert.fail('unexpected import '+name));
const helperCases=[];for(const n of [0n,1n,-1n,9007199254740991n,-9007199254740991n,9007199254740992n,-9007199254740992n,1n<<200n,-(1n<<200n)]){const field='field\ud800👩';let output;try{output={kind:'ok',value:tag(helpers.__helpers.toSafeNumber(n,field))}}catch(e){output={kind:'error',name:units(e.name),message:units(e.message)}}helperCases.push({value:tag(n),field:units(field),safe:helpers.__helpers.isSafeIntegerBigInt(n),output});}assert.equal(helperCases.length,9);
const httpFile=path.join(root,'packages/lighter-sdk/src/client/http-types.ts'),tree=ts.createSourceFile(httpFile,fs.readFileSync(httpFile,'utf8'),ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
const req=(n,t)=>[n,t,false],opt=(n,t)=>[n,t,true];
const schemas={LighterApiClientConfig:[req('baseUrl','string'),opt('timeout','number'),opt('headers','Record<string, string>'),opt('debug','boolean')],ApiKey:[req('index','number'),req('publicKey','string'),req('active','boolean'),opt('createdAt','number'),opt('lastUsedAt','number')],TransferFeeInfo:[req('fee','bigint'),req('minAmount','bigint'),req('maxAmount','bigint'),opt('feePercentage','number')],ApiErrorResponse:[req('code','string'),req('message','string'),opt('details','Record<string, any>'),opt('requestId','string')],ApiResponse:[req('success','boolean'),opt('data','T'),opt('error','ApiErrorResponse'),opt('timestamp','number')],GetNonceResponse:[req('nonce','string'),req('nextNonce','string')],GetApiKeysResponse:[req('apiKeys','ApiKey[]')],SendTxResponse:[req('txHash','string'),req('status',"'pending' | 'confirmed' | 'failed'"),opt('blockNumber','number')],GetTransferFeeResponse:[req('fee','string'),req('minAmount','string'),req('maxAmount','string'),opt('feePercentage','number')]};
for(const [name,schema]of Object.entries(schemas)){const decl=tree.statements.filter(n=>ts.isInterfaceDeclaration(n)&&n.name.text===name);assert.equal(decl.length,1,`${name} unique interface`);const d=decl[0];assert.equal((d.heritageClauses||[]).length,0,`${name} no inherited fields`);assert.deepEqual((d.typeParameters||[]).map(n=>n.getText(tree)),name==='ApiResponse'?['T']:[],`${name} generic signature`);for(const n of d.members)assert.ok(ts.isPropertySignature(n),`${name} property schema`);assert.deepEqual(d.members.map(n=>[n.name.getText(tree),n.type.getText(tree),!!n.questionToken]),schema,`${name} source fields`);}
const key=i=>({index:i===1?-0:i===2?Infinity:17.25,publicKey:'public-'+i+'\ud800👩',active:i!==2,...(i?{createdAt:i===1?NaN:-Infinity,lastUsedAt:i===1?-0:9007199254740992}:{})});
const payloads={undefined:undefined,nan:NaN,big:1n<<210n,text:'opaque\ud800',symbol:Symbol('opaque'),function:()=>0};
function encode(v){if(v===undefined)return{undefined:true};if(v===null||typeof v!=='object')return tag(v);if(Array.isArray(v))return v.map(encode);return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,encode(v)]));}
const models=[];
for(let i=0;i<3;i++){
 const error={code:'code-A\ud800',message:'message-B👩',...(i?{details:payloads,requestId:'request-C\udfff'}:{})};
 const records={LighterApiClientConfig:{baseUrl:'url\ud800👩',...(i?{timeout:i===1?-0:Infinity,headers:{'2':'header-A\ud800','10':'header-B👩','key\udfff':'value-C'},debug:i===1}:{})},ApiKey:key(i),TransferFeeInfo:{fee:1n<<200n,minAmount:-(1n<<201n),maxAmount:1n<<202n,...(i?{feePercentage:i===1?NaN:-0}:{})},ApiErrorResponse:error,ApiResponse:{success:i===1,...(i?{data:payloads,error,timestamp:i===1?-0:NaN}:{})},GetNonceResponse:{nonce:'nonce-A\ud800',nextNonce:'next-B👩'},GetApiKeysResponse:{apiKeys:i?[key(2),key(1),key(2),key(0)]:[]},SendTxResponse:{txHash:'hash-A\ud800👩',status:['pending','confirmed','failed'][i],...(i?{blockNumber:i===1?9007199254740992:NaN}:{})},GetTransferFeeResponse:{fee:'fee-A\ud800',minAmount:'min-B👩',maxAmount:'max-C\udfff',...(i?{feePercentage:i===1?Infinity:-0}:{})}};
 for(const [record,value]of Object.entries(records)){const input=encode(value);if(value.headers)input.headers=Object.entries(value.headers).map(([k,v])=>({key:units(k),value:encode(v)}));models.push({record,variant:i,input});}
}
assert.equal(models.length,27,'independent HTTP model fixture floor');
const body={constants:Object.fromEntries(['MaxAccountIndex','MaxApiKeyIndex','MaxNonce','MinOrderSize','MaxOrderSize'].map(k=>[k,tag(constants[k])])),cases,orderCases,errorCases,helperCases,models};
process.stdout.write(JSON.stringify({contentHash:require('node:crypto').createHash('sha256').update(JSON.stringify(body)).digest('hex'),...body}));
