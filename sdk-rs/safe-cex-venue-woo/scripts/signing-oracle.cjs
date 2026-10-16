// Capture the unchanged signer at HMAC update and Axios transform boundaries.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtimeRequire = require('./oracle-runtime.cjs');
const axios = runtimeRequire('axios');
const qs = runtimeRequire('qs');
const createHmac = runtimeRequire('create-hmac');
const root = path.resolve(__dirname, '../../..');
const filename = path.join(root,'packages/safe-cex/src/exchanges/woo/woo.api.ts');
let source = fs.readFileSync(filename,'utf8')+'\nexport { signV1, signV3 };';
if(process.argv.includes('--snapshot-options')) {
  assert.ok(source.includes('  const xhr = safeCexAxiosCreate('));
  source=source.replace('  const xhr = safeCexAxiosCreate(', '  options = { ...options };\n  const xhr = safeCexAxiosCreate(');
}
const js = ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
const out = {};
let preimage;
const constants = JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/fixtures/constants.json'),'utf8')).ordered;
const bindings = {
  axios,
  'axios-retry': () => {},
  'create-hmac': (algorithm, key) => {
    const hmac = createHmac(algorithm, key);
    const update = hmac.update.bind(hmac);
    hmac.update = (text) => { preimage = text; return update(text); };
    return hmac;
  },
  qs,
  '../../safeCexAxiosCreate': {safeCexAxiosCreate: config => axios.create(config)},
  '../../transport/registry': {getTransport: options => ({proxyPrefix:()=>options.proxy || ''})},
  '../../utils/virtualClock': {virtualClock:{getCurrentTimeMs:()=>1700000000123}},
  './woo.types': Object.fromEntries(Object.entries(constants).map(([k,v])=>[k, ['BASE_URL','ENDPOINTS'].includes(k)?Object.fromEntries(v):v])),
  '../../utils/retryReadsOnly': {networkErrorOnRead:()=>()=>false},
};
vm.runInNewContext(js,{exports:out,require:name=>{assert.ok(name in bindings,`unbound import ${name}`);return bindings[name];}},{filename});
const cases = [
  ['v1-collation-dynamic',1,{url:'/v1/orders',params:Object.fromEntries(['Z','z','ä','a','brokerId','broker_id','10','2','_','漢','中','😀'].map((key,i)=>[key,i]))}],
  ['v1-nested-repeat-long',1,{url:'/v1/order',method:'put',data:{z:{Z:2,a:1},list:Array.from({length:12},(_,i)=>i)}}],
  ['v1-explicit-null-data',1,{url:'/v1/order',method:'post',data:null,params:{a:null,b:false}}],
  ['v1-absolute-proxy',1,{url:'https://example.test/v1/orders',method:'get',params:{b:2,a:1}},{proxy:'https://proxy.test'}],
  ['v1-form',1,{url:'/v1/order',method:'post',data:{symbol:'PERP_BTC_USDT',side:'BUY',order_type:'LIMIT',order_price:1e-7,order_quantity:1e21}}],
  ['v1-missing-optional',1,{url:'/v1/order',method:'post',data:{symbol:'PERP_BTC_USDT',order_price:undefined,side:'BUY'}}],
  ['v1-query-repeat',1,{url:'/v1/orders',params:{z:['one','two'],a:'a b',empty:null}}],
  ['v1-empty-query',1,{url:'/v1/orders',params:{}}],
  ['v1-falsy-data-uses-params',1,{url:'/v1/order',method:'delete',data:false,params:{z:2,a:1}}],
  ['v1-patch-is-not-mutation',1,{url:'/v1/order',method:'patch',data:{z:2,a:1}}],
  ['v3-two-key-order',3,{url:'/v3/order',method:'post',data:{z:2,a:1}}],
  ['v3-missing-optional',3,{url:'/v3/order',method:'post',data:{symbol:'PERP_BTC_USDT',price:undefined,side:'BUY'}}],
  ['v3-null-distinct-from-missing',3,{url:'/v3/order',method:'post',data:{symbol:'PERP_BTC_USDT',price:null,side:'BUY'}}],
  ['v3-query-order-indices',3,{url:'/v3/orders',params:{z:['one','two'],a:'a b'}}],
  ['v3-empty-query-question-mark',3,{url:'/v3/orders',params:{}}],
  ['v3-falsy-data-signs-empty',3,{url:'/v3/order',method:'post',data:false}],
  ['v3-falsy-zero-signs-empty',3,{url:'/v3/order',method:'post',data:0}],
  ['v3-raw-string-is-json-quoted',3,{url:'/v3/order',method:'post',data:'{"z":2,"a":1}'}],
  ['v3-number-spelling',3,{url:'/v3/order',method:'post',data:{tiny:1e-7,large:1e21,minusZero:-0}}],
  ['v3-index-keys-first',3,{url:'/v3/order',method:'post',data:{z:1,'10':2,'2':3,a:4}}],
  ['v3-proxy-in-signed-url',3,{url:'https://api-pub.woox.io/v1/hist/kline',method:'get',params:{symbol:'PERP_BTC_USDT'}},{proxy:'https://proxy.invalid'}],
  ['v3-unicode-body',3,{url:'/v3/order',method:'post',data:{z:'\uFEFF',a:'\u0085'}}],
];
async function main() {
  const captured=[];
  for(const [name,version,config,extra] of cases) {
    preimage=undefined;
    const signed=out[version===1?'signV1':'signV3'](config,{key:'fixture-key',secret:'fixture-secret',...extra});
    assert.equal(typeof preimage,'string');
    let adapterInput;
    await axios.request({...signed,adapter:async request=>{
      adapterInput={url:request.url,method:request.method,body:request.data,headers:request.headers.toJSON()};
      return {data:null,status:200,statusText:'OK',headers:{},config:request};
    }});
    captured.push({name,version,input:config,options:extra||{},preimage,signature:signed.headers.get('x-api-signature'),signed:{url:signed.url,method:signed.method,params:signed.params,data:signed.data},adapterInput});
  }
  // Exercise the exported factory, not only its instrumented private signers.
  const dynamicParams=Object.fromEntries(['brokerId','broker_id','Z','a','ä','z','2','10','?','.'].map((key,index)=>[key,index]));
  const client=out.createAPI({key:'fixture-key',secret:'fixture-secret'},{});
  preimage=undefined;
  let adapterInput;
  await client.get('/v1/orders',{params:dynamicParams,adapter:async request=>{
    adapterInput={url:request.url,method:request.method,body:request.data,headers:request.headers.toJSON()};
    return {data:null,status:200,statusText:'OK',headers:{},config:request};
  }});
  assert.ok(preimage.includes('broker_id=1&brokerId=0'), 'dynamic keys must reach the real factory signer');
  captured.push({name:'exported-createAPI-dynamic-keys',input:{url:'/v1/orders',params:dynamicParams},preimage,adapterInput});
  // Capture interceptor branches before implementing the Rust client factory.
  const factoryCases = [
    ['public-bypass', {url:'/v1/public/info',method:'post',data:{a:1}}, {}],
    ['broker-live', {url:'/v1/order',method:'post',data:{symbol:'PERP_BTC_USDT'}}, {}],
    ['broker-testnet', {url:'/v1/order',method:'post',data:{}}, {testnet:true}],
    ['algo-broker', {url:'/v3/algo/order',method:'post',data:{}}, {}],
    ['broker-missing-data', {url:'/v1/order',method:'post'}, {}],
    ['broker-null-data', {url:'/v1/order',method:'post',data:null}, {}],
    ['broker-string-data', {url:'/v1/order',method:'post',data:'{"symbol":"PERP_BTC_USDT"}'}, {}],
    ['broker-array-data', {url:'/v1/order',method:'post',data:[]}, {}],
    ['no-key', {url:'/v3/order',method:'post',data:{a:1}}, {key:''}],
    ['no-secret', {url:'/v3/order',method:'post',data:{a:1}}, {secret:''}],
    ['remote-bypass-after-broker', {url:'/v1/order',method:'post',data:{}}, {remoteSigner:{accountName:'fixture'}}],
    ['v1-substring', {url:'/v3/order?v1',method:'post',data:{a:1}}, {}],
    ['recv-window-zero', {url:'/v1/public/info'}, {extra:{recvWindow:0}}],
    ['recv-window-null', {url:'/v1/public/info'}, {extra:{recvWindow:null}}],
  ];
  for (const [name, input, extra] of factoryCases) {
    const client=out.createAPI({key:'fixture-key',secret:'fixture-secret',...extra},{});
    preimage=undefined;
    let adapterInput;
    await client.request({...input,adapter:async request=>{
      adapterInput={baseURL:request.baseURL,url:request.url,method:request.method,body:request.data,params:request.params,timeout:request.timeout,headers:request.headers.toJSON()};
      return {data:null,status:200,statusText:'OK',headers:{},config:request};
    }});
    captured.push({name:`factory-${name}`,version:'factory',input,options:extra,preimage:preimage??null,adapterInput});
  }
  const mutableOptions={key:'fixture-key',secret:'fixture-secret'};
  const aliasClient=out.createAPI(mutableOptions,{});
  const alias=[];
  for(const secret of ['fixture-secret','changed-secret']) {
    mutableOptions.secret=secret;
    await aliasClient.get('/v1/orders',{adapter:async request=>{
      alias.push({secret,signature:request.headers.get('x-api-signature')});
      return {data:null,status:200,statusText:'OK',headers:{},config:request};
    }});
  }
  assert.notEqual(alias[0].signature,alias[1].signature,'source factory retains live secret alias');
  const aliasTarget=path.join(__dirname,'../tests/fixtures/factory-options-alias.json');
  const aliasBytes=JSON.stringify(alias,null,2)+'\n';
  if(process.argv.includes('--check'))assert.equal(fs.readFileSync(aliasTarget,'utf8'),aliasBytes,'factory option alias drift');
  else fs.writeFileSync(aliasTarget,aliasBytes);
  const bytes=JSON.stringify(captured,null,2)+'\n';
  const target=path.join(__dirname,'../tests/fixtures/signing.json');
  if(process.argv.includes('--check')) assert.equal(fs.readFileSync(target,'utf8'),bytes,'WOO signing boundary drift');
  else fs.writeFileSync(target,bytes);
  console.log(`WOO signing oracle: ${captured.length} live HMAC/Axios captures; no network`);
}
main().catch(error=>{console.error(error);process.exitCode=1;});
