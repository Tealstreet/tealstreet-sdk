const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {root,ts,load}=require('./types-oracle.cjs');
const types=load(path.join(root,'packages/safe-cex/src/types.ts')),venue=load(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
const dependencies=path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'..');
const qs=require(path.join(dependencies,'qs')),crypto=require('node:crypto'),{AxiosHeaders}=require(path.join(dependencies,'axios'));
function tree(file){return ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);}
function declaration(t,name){const n=t.statements.find(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>d.name.getText(t)===name));assert.ok(n,name);return n.getText(t);}
function compile(source,scope){const c=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});assert.equal(c.diagnostics.filter(d=>d.category===ts.DiagnosticCategory.Error).length,0);scope.exports={};vm.runInNewContext(c.outputText,scope);return scope.exports;}
const exchange=tree(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.exchange.ts')),api=tree(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.api.ts')),utils=tree(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.utils.ts'));
const cls=exchange.statements.find(n=>ts.isClassDeclaration(n)&&n.name?.text==='WeexExchange');
const clientId=tree(path.join(root,'packages/safe-cex/src/exchanges/clientOrderId.ts'));
const {patternClientOrderIdRule}=compile(declaration(clientId,'patternClientOrderIdRule'),{});
const pager=load(path.join(root,'packages/safe-cex/src/exchanges/income-paging.ts')).paginateIncomeRange;
const helperNames=['toNumber','toIntegerString','normalizeTimestampToMs','normalizeIncome'];
const category=load(path.join(root,'packages/safe-cex/src/utils/incomeCategoryFromType.ts')).incomeCategoryFromType;
const constants=[declaration(exchange,'WEEX_INCOME_MAX_WINDOW_MS'),declaration(exchange,'WEEX_INCOME_PAGE_LIMIT'),declaration(exchange,'extractArray')].join('\n');
const cases=[];const add=(id,method,input={})=>cases.push({id,input:{method,...input}});
add('serialize_repeat','serialize',{params:{b:[2,1],a:'x y'}});add('serialize_sorted','serialize',{params:{z:3,a:1,m:2}});add('serialize_nested','serialize',{params:{o:{b:2,a:1},a:[null,'x']}});add('serialize_null','serialize',{params:null});
add('api_required','api',{options:{}});add('api_signed','api',{options:{passphrase:'p',key:'k',secret:'s'}});add('api_public','api',{options:{publicDataOnly:true}});add('api_timeout','api',{options:{publicDataOnly:true,testnet:true,extra:{recvWindow:0}}});
const auth={passphrase:'p',key:'k',secret:'s'};
add('request_signed','request',{options:auth,config:{url:'/capi/v3/order',method:'get',params:{z:1,a:[2,3]},headers:{X:'x'}}});add('request_public','request',{options:auth,config:{url:venue.PUBLIC_ENDPOINTS[0]+'?a=1'}});add('request_remote','request',{options:{...auth,remoteSigner:true},config:{url:'/private',headers:{X:'x'}}});add('request_nokey','request',{options:{passphrase:'p'},config:{url:'/private'}});add('request_post','request',{options:auth,config:{url:'/private',method:'post',data:{b:2,a:1}}});add('request_empty','request',{options:auth,config:{url:'/private',params:{},data:0}});
add('response_zero','response',{response:{data:{code:0,x:3}}});add('response_200','response',{response:{data:{code:'200'}}});add('response_error','response',{response:{data:{code:7,msg:'bad',message:'other'}}});add('response_null','response',{response:{data:{code:null}}});
add('constructor_fields','constructor',{options:auth});
add('validate_success','validateAccount');add('validate_disabled','validateAccount',{config:{canTrade:false}});add('validate_error','validateAccount',{fail:'config',error:{response:{data:{msg:'venue error'}},message:'fallback'}});add('validate_balance_failure','validateAccount',{fail:'balance'});
add('account_hash','getAccount',{options:{key:'key-雪'}});add('account_fallback','getAccount',{options:{key:''}});add('account_error','getAccount',{validation:{error:'invalid'}});
add('start_public','start',{publicOnly:true});add('start_private','start',{config:{dualSidePosition:true}});add('start_external','start',{external:true});add('start_disposed','start',{disposeAt:'markets'});
add('tick_success','tick');add('tick_balance_failure','tick',{fail:'balance'});add('tick_orders_failure','tick',{fail:'orders'});add('tick_public','tick',{publicOnly:true});
add('income_default','fetchIncomeHistory',{data:{items:[{symbol:'BTC',incomeType:'FUNDING_FEE',income:'2',time:1700000000000,billId:'7'}]}});add('income_symbol','fetchIncomeHistory',{opts:{symbol:'BTC',startTime:0,endTime:9,limit:1000},data:[]});add('income_type','fetchIncomeHistory',{opts:{incomeType:'COMMISSION',limit:-2},data:{data:{items:[]}}});add('income_public','fetchIncomeHistory',{publicOnly:true});
add('range_empty','fetchIncomeHistoryRange',{start:0,end:10,pages:[[]]});add('range_short','fetchIncomeHistoryRange',{start:0,end:10,pages:[[{symbol:'BTC',incomeType:'FUNDING_FEE',income:2,asset:'USDT',timestamp:5,tranId:'a',category:'funding'}],[],[]]});add('range_failure','fetchIncomeHistoryRange',{start:0,end:10,fail:'range'});add('range_invalid','fetchIncomeHistoryRange',{start:10,end:0});
assert.equal(cases.length,42,'runtime::fixture_floor');
function enc(v){if(v===undefined)return{kind:'undefined'};if(v===null)return{kind:'null'};if(typeof v==='number'){const b=Buffer.alloc(8);b.writeDoubleBE(v);return{kind:'number',bits:b.toString('hex')};}if(typeof v==='string')return{kind:'string',units:Array.from({length:v.length},(_,i)=>v.charCodeAt(i))};if(typeof v==='boolean')return{kind:'boolean',value:v};if(typeof v==='function')return{kind:'function',name:v.name};if(v instanceof Map||Object.prototype.toString.call(v)==='[object Map]')return{kind:'map'};if(Array.isArray(v))return{kind:'array',items:v.map(enc)};return{kind:'object',fields:Object.entries(v).map(([k,x])=>[k,enc(x)])};}
async function run(c){const x=c.input,requests=[],updates=[],logs=[],reported=[],calls=[],sleeps=[],registrations=[],creates=[];let index=0,disposed=false,instance,requestHook,responseHook;
const error=()=>Object.assign(new Error('failure'),x.error??{});
const ready=(name,value)=>{calls.push(name);return Promise.resolve().then(()=>{if(x.fail===name)throw error();if(x.disposeAt===name)disposed=true;return value;});};
const record=new Proxy({loaded:{markets:false,tickers:false,balance:false,positions:false,orders:false}}, {get(t,k){if(k==='update')return p=>{updates.push(p);if(p.loaded)t.loaded=p.loaded;};if(k==='setSetting')return(k,v)=>updates.push({setting:k,value:v});if(k==='addOrUpdatePositionsFull')return p=>updates.push({fullPositions:p});return t[k];}});
class Base{constructor(name,options,store){this.options=options;this.store=store;this.isPublicDataOnly=!!x.publicOnly;}get isDisposed(){return disposed;}createXhrWithSecondaryKey(opts,factory){calls.push('xhr');creates.push({constructorFields:Object.keys(this)});assert.equal(factory.name,'createAPI');assert.strictEqual(opts,this.options);return 'xhr';}}
class Pub{constructor(){calls.push('public');return{kind:'public'};}}class Priv{constructor(){calls.push('private');return{kind:'private'};}}
const scope={...types,...venue,BaseExchange:Base,WeexPublicWebsocket:Pub,WeexPrivateWebsocket:Priv,patternClientOrderIdRule,uuid:()=> 'uuid',createAPI:function createAPI(){},createHash:crypto.createHash,paginateIncomeRange:pager,sleep:async ms=>{sleeps.push(ms);},loop:(f,ms)=>{calls.push('loop');updates.push({loopInterval:ms});},incomeCategoryFromType:category};
const exports=compile(constants+'\n'+helperNames.map(n=>declaration(utils,n)).join('\n')+'\n'+cls.getText(exchange).replace('export class WeexExchange','export class Slice'),scope);
const incomeScope=exports;
let result,errorValue=null,initial;
try{
 if(['serialize','api','request','response'].includes(x.method)){
  const fake={interceptors:{request:{use:f=>{registrations.push('request');requestHook=f;}},response:{use:f=>{registrations.push('response');responseHook=f;}}}};
  const apiScope={...venue,qs,AxiosHeaders,createHmac:crypto.createHmac,virtualClock:{getCurrentTimeMs:()=>123456},bigIntAsStringJsonParse:function transform(){},safeCexAxiosCreate:(cfg,opts,ex)=>{creates.push({config:cfg,options:opts,exchange:ex});return fake;},retry:()=>registrations.push('retry'),WEEX_RETRY_CONFIG:{}};
  const e=compile(['serializeWeexParams','signWeexMessage','createAPI'].map(n=>declaration(api,n)).join('\n'),apiScope);
  if(x.method==='serialize')result=e.serializeWeexParams(x.params);
  else {e.createAPI(x.options??auth,{});if(x.method==='api')result='xhr';else if(x.method==='request'){result=requestHook(x.config);if(result.headers instanceof AxiosHeaders)result={...result,headers:result.headers.toJSON()};result={config:result,same:result===x.config};}else{result=responseHook(x.response);result={response:result,same:result===x.response};}}
 }else{
  instance=new exports.Slice('WEEX',x.options??{},record);
  if(x.method==='constructor'){result=instance;}
  else{
   const original=instance[x.method];calls.length=0;creates.length=0;
   instance.xhr={get:()=>{requests.push({method:'GET',endpoint:venue.ENDPOINTS.ACCOUNT_CONFIG});return ready('config',{data:x.config??{canTrade:true}});},post:(endpoint,payload)=>{requests.push({method:'POST',endpoint,payload});return ready('income',{data:x.data??[]});}};
   instance.publicWebsocket={connectAndSubscribe:()=>calls.push('connectPublic')};instance.privateWebsocket={connectAndSubscribe:()=>calls.push('connectPrivate')};
   instance.initExternalPublicData=()=>ready('external',x.external??false);instance.fetchMarkets=()=>ready('markets',[{symbol:'BTC'},{symbol:'ETH'}]);instance.fetchTickers=()=>ready('tickers',[{symbol:'BTC'}]);instance.fetchSymbolConfigs=()=>ready('symbols',undefined);
   instance.fetchBalanceAndPositions=()=>ready('balance',{balance:{total:3,free:2},positions:[{id:'p'}]});instance.fetchOrders=()=>ready('orders',[{id:'o'}]);instance.validateAccount=()=>ready('validate',x.validation??{error:''});instance.tick=()=>ready('tick',undefined);instance.getTickInterval=()=>17;
   instance.log=(message,severity)=>logs.push({message,severity});instance.emitAndLogError=message=>reported.push(message);
   if(x.method==='fetchIncomeHistoryRange')instance.fetchIncomeHistory=opts=>{requests.push({method:'PAGE',params:opts});return ready('range',x.pages?.[index++]??[]);};
   const args=x.method==='fetchIncomeHistory'?[x.opts]:x.method==='fetchIncomeHistoryRange'?[x.start,x.end,x.incomeType]:[];
   const p=original(...args);initial={requests:structuredClone(requests),calls:[...calls],updates:structuredClone(updates),sleeps:[...sleeps]};result=await p;
  }
 }
}catch(e){errorValue={name:e.name,message:e.message};if(e.response)errorValue.response=e.response;}
return{id:c.id,input:enc(x),expected:enc({result,error:errorValue,requests,updates,logs,reported,calls,sleeps,registrations,creates,initial})};}
(async()=>{const rows=[];for(const c of cases)rows.push(await run(c));process.stdout.write(JSON.stringify(rows));})().catch(e=>{console.error(e);process.exitCode=1;});
