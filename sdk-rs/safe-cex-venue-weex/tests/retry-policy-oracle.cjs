const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const {root, ts, header, capture, load} = require('./types-oracle.cjs');
const actualNetworkError = require('axios-retry').isNetworkError;
const file = path.join(root,'packages/safe-cex/src/exchanges/weex/weex.api.ts');
const ast = ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names = ['WEEX_RATE_LIMIT_MAX_RETRIES','WEEX_READ_ONLY_POST_ENDPOINTS','parseRetryAfterMs','isWeexReadOnlyRequest','shouldRetryWeexRequest'];
const nodes = ast.statements.filter(n=>ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>names.includes(d.name.getText(ast))));
assert.equal(nodes.length,5,'retry_policy::source_inventory');
process.env.TZ = 'UTC';
const units = text => Array.from({length:text.length},(_,i)=>text.charCodeAt(i));
let dateEvents=[], networkCalls=0, clockCalls=0;
const clock=1700000000000;
const context={exports:{},...load(path.join(root,'packages/safe-cex/src/exchanges/weex/weex.types.ts')),
 Date:{now:()=>{clockCalls++;return clock;},parse:text=>{const result=Date.parse(text);dateEvents.push({units:units(text),parsed:capture(result)});return result;}},
 isNetworkError:error=>{networkCalls++;return actualNetworkError(error);}};
vm.runInNewContext(ts.transpileModule(nodes.map(n=>n.getText(ast)).join('\n')+'\nexport {parseRetryAfterMs,isWeexReadOnlyRequest,shouldRetryWeexRequest};',{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,context,{filename:file});
const dateInputs=[
 ['seconds',2.5,clock],['string_seconds',' 3.25 ',clock],['hex_seconds','0x10',clock],['overflow_product',1e308,clock],
 ['zero',0,clock],['negative_zero',-0,clock],['negative',-3,clock],['nan',NaN,clock],['infinity',Infinity,clock],['negative_infinity',-Infinity,clock],
 ['empty','',clock],['space',' ',clock],['bad','not a date',clock],['iso_future','2024-01-02T03:04:05Z',clock],['iso_past','2000-01-01T00:00:00Z',clock],
 ['local_date','2024-01-02T03:04:05',clock],['now_nan','2024-01-02T03:04:05Z',NaN],['now_infinity','2024-01-02T03:04:05Z',Infinity],
 ['surrogate','\ud800',clock],['surrogate_prefix','\ud8002024-01-02',clock],
 ['null',null,clock],['boolean',true,clock],['array',['5'],clock],['object',{toString(){throw Error('must not coerce');}},clock],
 ['function',()=>5,clock],['bigint',5n,clock],['symbol',Symbol('5'),clock],['undefined',undefined,clock],['default_now','2024-01-02T03:04:05Z',undefined],
];
const dates=dateInputs.map(([id,value,now])=>{
 dateEvents=[];clockCalls=0;
 const result=context.exports.parseRetryAfterMs(value,now);
 const input=typeof value==='number'?{kind:'number',value:capture(value)}:typeof value==='string'?{kind:'string',units:units(value)}:{kind:'other'};
 return {id,input,now:capture(now===undefined?clock:now),clockCalls,dateEvents:dateEvents.slice(),result:capture(result)};
});
const income=context.ENDPOINTS.INCOME;
const configs=[
 ['get',{method:'GET'}],['head',{method:'head'}],['get_surrogate_url',{method:'get',url:'\ud800'}],
 ['income',{method:'POST',url:income}],['income_query',{method:'post',url:income+'?a=1'}],['post_other',{method:'POST',url:'/capi/v3/order'}],
 ['post_missing',{method:'POST'}],['post_empty',{method:'POST',url:''}],['post_suffix',{method:'POST',url:income+'/'}],['post_case',{method:'POST',url:income.toUpperCase()}],
 ['post_long_s',{method:'poſt',url:income}],['post_ligature',{method:'poﬆ',url:income}],['post_ligature_long_s',{method:'poﬅ',url:income}],
 ['get_surrogate_method',{method:'GET\ud800',url:income}],['post_surrogate_url',{method:'POST',url:'\ud800'+income}],
 ['delete',{method:'DELETE',url:income}],['missing_method',{url:income}],['missing_config',undefined],['empty_method',{method:'',url:income}],
];
function configFields(config) {return {...(config?.method!==undefined?{method:units(config.method)}:{}),...(config?.url!==undefined?{url:units(config.url)}:{})};}
const readOnly=configs.map(([id,config])=>({id,...configFields(config),result:context.exports.isWeexReadOnlyRequest(config)}));
const policyInputs=[
 ['network_get',{config:{method:'GET'},code:'ECONNRESET'}],['network_post',{config:{method:'POST',url:'/capi/v3/order'},code:'ECONNRESET'}],
 ['network_income',{config:{method:'POST',url:income},code:'ECONNRESET'}],['network_head',{config:{method:'HEAD'},code:'ENOTFOUND'}],
 ['canceled',{config:{method:'GET'},code:'ERR_CANCELED'}],['timeout',{config:{method:'GET'},code:'ECONNABORTED'}],['unsafe_tls',{config:{method:'GET'},code:'CERT_HAS_EXPIRED'}],
 ['no_code',{config:{method:'GET'}}],['server_error',{config:{method:'GET'},response:{status:500},code:'ECONNRESET'}],
 ['rate_first',{config:{method:'GET'},response:{status:429}}],['rate_second',{config:{method:'GET','axios-retry':{retryCount:1}},response:{status:429}}],
 ['rate_negative',{config:{method:'GET','axios-retry':{retryCount:-1}},response:{status:429}}],['rate_nan',{config:{method:'GET','axios-retry':{retryCount:NaN}},response:{status:429}}],
 ['rate_infinity',{config:{method:'GET','axios-retry':{retryCount:Infinity}},response:{status:429}}],['rate_null',{config:{method:'GET','axios-retry':{retryCount:null}},response:{status:429}}],
 ['mutation_rate',{config:{method:'DELETE',url:income},response:{status:429}}],['missing_config_network',{code:'ECONNRESET'}],
 ['response_no_status',{config:{method:'GET'},response:{},code:'ECONNRESET'}],['rate_income',{config:{method:'POST',url:income},response:{status:429}}],
];
const policy=policyInputs.map(([id,error])=>{
 networkCalls=0;
 const result=context.exports.shouldRetryWeexRequest(error);
 const count=error.config?.['axios-retry']?.retryCount;
 return {id,...configFields(error.config),...(error.response?.status!==undefined?{status:capture(error.response.status)}:{}),...(count!=null?{retryCount:capture(count)}:{}),network:actualNetworkError(error),networkCalls,result};
});
process.stdout.write(JSON.stringify({header,dates,readOnly,policy}));
