const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),root=path.resolve(__dirname,'../../..');
const dir='packages/safe-cex/src/exchanges/lighter/';
const sources=new Map();
function evaluate(file,imports={},expose=[],globals={}) {
 const source=fs.readFileSync(path.join(root,file),'utf8');sources.set(file,source);
 const c=ts.transpileModule(source+'\n'+expose.map(n=>`exports.${n}=${n};`).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
 assert.equal(c.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);
 const out={};vm.runInNewContext(c.outputText,{exports:out,Error,Map,Array,...globals,require(n){assert.ok(Object.hasOwn(imports,n),n);return imports[n]}});return out;
}
const domains=fs.readFileSync(path.join(root,'packages/safe-cex/src/types.ts'),'utf8');sources.set('packages/safe-cex/src/types.ts',domains);
const tree=ts.createSourceFile('types',domains,ts.ScriptTarget.Latest,true),enums={};
vm.runInNewContext(ts.transpileModule(tree.statements.filter(ts.isEnumDeclaration).map(n=>n.getText(tree)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:enums});
const types=evaluate(dir+'lighter.types.ts',{'../../types':enums,'../../transport/registry':{withPublicCorsProxy:url=>url}});
const tier=evaluate(dir+'lighter.account-tier.ts'),fees=evaluate(dir+'lighter.builder-fee.ts',{'./lighter.types':types}),utils=evaluate(dir+'lighter.utils.ts');
const names=['resolveApprovalFeeCaps','createApprovalClient','ensureWasmReady','createApprovalTransactionClient','fetchAccountLimitsForApproval','assertPartnerAttributionTierEligibleForApproval','resolveAccountIndex','fetchAccountSnapshot','getNextNonce'];
const U={kind:'undefined'},B=n=>({kind:'bigint',value:String(n)}),N=n=>({kind:'number',value:n});
function decode(v){if(v&&v.kind==='undefined')return undefined;if(v&&v.kind==='bigint')return BigInt(v.value);if(v&&v.kind==='number')return v.value==='NaN'?NaN:v.value==='Infinity'?Infinity:v.value==='-Infinity'?-Infinity:v.value==='-0'?-0:v.value;if(Array.isArray(v)){const a=v.map(decode);v.forEach((x,i)=>{if(x&&x.kind==='hole')delete a[i]});return a}if(v&&typeof v==='object')return Object.fromEntries(Object.entries(v).map(([k,v])=>[k,decode(v)]));return v}
function enc(v){if(v===undefined)return U;if(v===null)return{kind:'null'};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')}}if(typeof v==='bigint')return{kind:'bigint',decimal:String(v)};if(typeof v==='string')return{kind:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};if(typeof v==='boolean')return{kind:'boolean',value:v};if(Array.isArray(v))return{kind:'array',items:Array.from({length:v.length},(_,i)=>i in v?enc(v[i]):{kind:'hole'})};return{kind:'object',fields:Object.entries(v).map(([k,v])=>[k,enc(v)])}}
const base={accountIndex:B('123456789012345678901234567890'),apiKeyIndex:7,apiPrivateKey:'secret',apiClient:{imported:'http'},testnet:false};
const rows=[];function add(id,method,params={},extra={}){rows.push({id,method,params:{...base,...params},payload:{user_tier:'plus'},...extra})}
for(const [id,value]of [['missing',U],['zero',0],['finite',13],['negative',-7],['negative-zero',N('-0')],['nan',N('NaN')],['inf',N('Infinity')],['string','9']])add('caps-'+id,'resolveApprovalFeeCaps',{feeValue:value,feeCaps:{maxSpotMakerFee:U,extra:'retained'}});
add('caps-null','resolveApprovalFeeCaps',{feeValue:3,feeCaps:null});add('caps-sequence','resolveApprovalFeeCaps',{feeCaps:{}},{sequences:{feeValue:[1,2,3,4,5,6]}});
for(const [id,params]of [['main',{}],['test',{testnet:true}],['robinhood',{exchangeName:'lighter-robinhood'}],['override',{baseUrl:' https://custom/// '} ]])add('client-'+id,'createApprovalClient',params);
for(const ready of [false,true])add('wasm-'+ready,'ensureWasmReady',{}, {ready});add('wasm-failure','ensureWasmReady',{}, {ready:false,fail:'init'});
for(const [id,params]of [['main',{}],['test',{testnet:true}],['robinhood',{exchangeName:'lighter-robinhood',testnet:true}]])add('transaction-'+id,'createApprovalTransactionClient',params);
add('transaction-crypto-failure','createApprovalTransactionClient',{}, {fail:'crypto'});
for(const [id,extra]of [['ready',{ready:true}],['init',{ready:false}],['negative-time',{now:-1}],['invalid-time',{now:NaN}],['auth-failure',{fail:'getAuthToken'}],['get-failure',{fail:'get'}],['init-failure',{ready:false,fail:'init'}],['index-sequence',{sequences:{accountIndex:[B('11'),B('12')]}}]])add('limits-'+id,'fetchAccountLimitsForApproval',{},extra);
for(const [id,params,extra]of [
 ['zero-fees',{feeCaps:{maxPerpsMakerFee:0,maxPerpsTakerFee:0,maxSpotMakerFee:0,maxSpotTakerFee:0}},{}],
 ['missing-key',{apiKeyIndex:U,feeCaps:{maxPerpsTakerFee:1}},{}],['empty-secret',{apiPrivateKey:'',feeCaps:{maxPerpsTakerFee:1}},{}],
 ['plus',{feeCaps:{maxPerpsTakerFee:1}},{payload:{user_tier:'plus'}}],['standard',{feeCaps:{maxPerpsTakerFee:1}},{payload:{user_tier:'std'}}],
 ['unknown',{feeCaps:{maxPerpsTakerFee:1}},{payload:{user_tier:'unknown'}}],['best-effort',{feeCaps:{maxPerpsTakerFee:1}},{fail:'get'}],
 ['exact-error',{feeCaps:{maxPerpsTakerFee:1}},{fail:'get',errorMessage:tier.LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE}],
 ['nonerror',{feeCaps:{maxPerpsTakerFee:1}},{fail:'get',nonerror:true,errorMessage:tier.LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE}],
 ])add('tier-'+id,'assertPartnerAttributionTierEligibleForApproval',params,extra);
const address='0x'+'ab'.repeat(20);
for(const [id,identifier,payload]of [
 ['numeric',' 0009007199254740993 ',{}],['invalid','not-an-address',{}],['wallet',address,{sub_accounts:[{account_index:'8'}]}],
 ['precedence',address,{sub_accounts:[{account_index:'9'}],accounts:[{account_index:10}],data:{accounts:[{index:11}]}}],
 ['nested',address,{data:{sub_accounts:[{index:'0x10'}]}}],['zero',address,{accounts:[{account_index:0}]}],['negative',address,{accounts:[{index:'-1'}]}],
 ['bad-first',address,{sub_accounts:[{account_index:'bad'}],accounts:[{account_index:8}]}],['nullish',address,{accounts:[{account_index:null,index:'7'}]}],
 ['no-account',address,{}],['sparse-first',address,{accounts:[{kind:'hole'},{index:8}]}],
 ])add('resolve-'+id,'resolveAccountIndex',{accountIdentifier:identifier},{payload});
const integrators=[{account_index:'3',approval_expiry:'123',max_perps_taker_fee:1,max_perps_maker_fee:2,max_spot_taker_fee:3,max_spot_maker_fee:4},{index:'-1',approval_expiry:'bad'},{index:0,approval_expiry:null,max_perps_maker_fee:N('-0')},{account_index:'bad'}];
for(const [id,payload,extra]of [
 ['full',{accounts:[{user_tier:'plus',l1_address:address,approved_integrators:integrators}]},{}],
 ['missing',{},{}],['scalar',{accounts:[17]},{}],['empty',{accounts:[{}]},{}],['null',{accounts:[null]},{}],
 ['sparse',{accounts:[{approved_integrators:[{kind:'hole'},integrators[0]]}]},{}],['null-item',{accounts:[{approved_integrators:[null]}]},{}],
 ['nonarray',{accounts:[{l1_address:17,approved_integrators:{}}]},{}],
 ['index-sequence',{accounts:[{}]},{sequences:{accountIndex:[B('11'),B('12')]}}],
 ])add('snapshot-'+id,'fetchAccountSnapshot',{}, {payload,...extra});
add('nonce-main','getNextNonce');add('nonce-test','getNextNonce',{testnet:true});add('nonce-failure','getNextNonce',{}, {fail:'getNextNonce'});
add('limits-auth-rejection','fetchAccountLimitsForApproval',{}, {fail:'getAuthToken',reject:true});
add('snapshot-get-rejection','fetchAccountSnapshot',{}, {fail:'get',reject:true});
add('tier-rejection','assertPartnerAttributionTierEligibleForApproval',{feeCaps:{maxPerpsTakerFee:1}}, {fail:'get',reject:true});
add('nonce-rejection','getNextNonce',{}, {fail:'getNextNonce',reject:true});
const familyFloors={resolveApprovalFeeCaps:10,createApprovalClient:4,ensureWasmReady:3,createApprovalTransactionClient:4,fetchAccountLimitsForApproval:9,assertPartnerAttributionTierEligibleForApproval:10,resolveAccountIndex:11,fetchAccountSnapshot:10,getNextNonce:4};
for(const [method,n]of Object.entries(familyFloors))assert.equal(rows.filter(r=>r.method===method).length,n,'approval_reads::family_floor '+method);
assert.equal(rows.length,65,'approval_reads::source_fixture_floor');
async function run(row){let initReceipts=0;const events=[],constructs=[],calls=[],params=decode(row.params),payload=decode(row.payload),crypto={imported:'crypto'};
 for(const [key,values]of Object.entries(row.sequences||{})){let i=0;const list=values.map(decode);Object.defineProperty(params,key,{enumerable:true,get(){events.push('param:'+key);return list[Math.min(i++,list.length-1)]}})}
 const injected=row.nonerror?{message:row.errorMessage||'injected'}:new Error(row.errorMessage||'injected:'+row.fail);
 function check(name){events.push(name);if(row.fail===name&&!row.reject)throw injected}
 class Api{constructor(options){check('LighterApiClient');constructs.push({name:'LighterApiClient',options});this.options=options}get(url,config){check('get');calls.push({name:'get',args:[url,config]});return row.reject&&row.fail==='get'?Promise.reject(injected):Promise.resolve(payload)}getNextNonce(...args){check('getNextNonce');calls.push({name:'getNextNonce',args});return row.reject&&row.fail==='getNextNonce'?Promise.reject(injected):Promise.resolve(900719925474099312345n)}}
 class Tx{constructor(options){check('TransactionClient');constructs.push({name:'TransactionClient',options});if(row.method!=='createApprovalTransactionClient')assert.strictEqual(options.httpClient.options,constructs[0].options);assert.strictEqual(options.crypto,crypto)}getAuthToken(...args){check('getAuthToken');calls.push({name:'getAuthToken',args});return row.reject&&row.fail==='getAuthToken'?Promise.reject(injected):Promise.resolve('auth-token')}}
 const imports={'@tealstreet/lighter-sdk':{LighterApiClient:Api,TransactionClient:Tx},'@tealstreet/poseidon-crypto':{isWasmReady(){check('ready');return row.ready!==false},initWasm(){initReceipts++;check('init');return Promise.resolve()},getCryptoApi(){check('crypto');return crypto}},'./lighter.account-tier':tier,'./lighter.builder-fee':fees,'./lighter.types':types,'./lighter.utils':utils};
 const mod=evaluate(dir+'lighter.builder-approval.ts',imports,names,{Date:{now(){check('now');return row.now===undefined?1700000000123:row.now}}});
 let result,promise;try{promise=mod[row.method](params);events.push('after-call');result=enc(await promise)}catch(e){result={kind:'error',name:e.name||'thrown',message:enc(e.message),injected:e===injected}}
 return{...row,params:enc(decode(row.params)),payload:enc(payload),sequences:Object.fromEntries(Object.entries(row.sequences||{}).map(([k,v])=>[k,v.map(x=>enc(decode(x)))])),now:enc(row.now===undefined?1700000000123:row.now),result,initReceipts,constructs:enc(constructs),calls:enc(calls),events};
}
(async()=>{const cases=[];for(const row of rows)cases.push(await run(row));process.stdout.write(JSON.stringify({count:65,sources:Object.fromEntries([...sources].map(([p,s])=>[p,require('node:crypto').createHash('sha256').update(s).digest('hex')])),cases}))})().catch(e=>{console.error(e);process.exitCode=1});
