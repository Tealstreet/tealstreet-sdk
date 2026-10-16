// createAPI reference capture plus direct toJsonString comparisons against Rust.
// Execute the real interceptor, real Axios transforms and real qs. The adapter is offline.
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const axios=require('axios');
const qs=require('qs');
const retryModule=require('axios-retry');
const createHmac=require('create-hmac');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root=path.resolve(__dirname,'../../..');
function evaluate(file,bindings={},selected) {
 let text=fs.readFileSync(path.join(root,file),'utf8');
 if(process.env.BITGET_SIGNING_MUTATION && file.endsWith('bitget.api.ts')) {
  assert.ok(['literal-replacement','all-replacements'].includes(process.env.BITGET_SIGNING_MUTATION),'known signing mutation');
  const from='json.replace(key, key.substring(1))';
  assert.equal(text.split(from).length,2,'signing mutation source anchor');
  text=text.replace(from,process.env.BITGET_SIGNING_MUTATION==='all-replacements'?'json.replaceAll(key, key.substring(1))':'json.replace(key, () => key.substring(1))');
 }
 const ast=ts.createSourceFile(file,text,ts.ScriptTarget.Latest,true);
 const nodes=ast.statements.filter(n=>!ts.isImportDeclaration(n)&&(!selected||ts.isVariableStatement(n)&&n.declarationList.declarations.some(d=>selected.includes(d.name.text))));
 const exports={};
 new Function('exports',...Object.keys(bindings),ts.transpileModule(nodes.map(n=>n.getText(ast)).join('\n')+(file.endsWith('bitget.api.ts')?'\nexports.toJsonString=toJsonString;':''),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(exports,...Object.values(bindings));
 return exports;
}
const protocol=evaluate('packages/safe-cex/src/exchanges/bitget/bitget.types.ts',{},['BASE_URL','BROKER_ID','ENDPOINTS','RECV_WINDOW']);
const reads=evaluate('packages/safe-cex/src/utils/retryReadsOnly.ts',{isNetworkError:retryModule.isNetworkError});
const signatureRejection=evaluate('packages/safe-cex/src/utils/signatureRejection.ts');
const getKey=evaluate('packages/safe-cex/src/utils/get-key.ts',{memoize:require('lodash/memoize'),snakeCase:require('lodash/snakeCase'),camelCase:require('lodash/camelCase')});
let captures=[];
const api=evaluate('packages/safe-cex/src/exchanges/bitget/bitget.api.ts',{
 ...protocol,...reads,...getKey,...signatureRejection,AxiosHeaders:axios.AxiosHeaders,
 safeCexAxiosCreate:config=>axios.create(config),
 retry:retryModule.default||retryModule,
 qs,virtualClock:{getCurrentTimeMs:()=>1700000000123},
 installTransportAdapter:()=>{throw Error('Unexpected live adapter installation');},
 appendHttpFailure:()=>{throw Error('Unexpected error response');},
 createHmac:(algorithm,key)=>{
  const actual=createHmac(algorithm,key);
  return {update(preimage){captures.push(preimage);actual.update(preimage);return this;},digest(){return actual.digest();}};
 }
});
const requests=[
 ['public',{},{}],
 ['body_complete',{method:'post',data:{symbol:'BTCUSDT',side:'buy',size:'1',clientOid:'test'}}],
 ['body_missing_optional',{method:'post',data:{symbol:'BTCUSDT',side:'buy',size:'1',clientOid:undefined}}],
 ['body_null_optional',{method:'post',data:{symbol:'BTCUSDT',side:'buy',size:'1',clientOid:null}}],
 ['symbol_dollars',{method:'post',data:{symbol:'BTC$USDT$',_tag:'_value'}}],
 // ts-bug: toJsonString replaces serialized values as well as property names.
 ['underscore_in_value_before_key',{method:'post',data:{text:'prefix _tag then',_tag:'_value',nested:{_inner:'_nested'}}}],
 ['integer_keys_numbers',{method:'post',data:{z:1e21,'10':1e-7,'2':1e20,a:-0,huge:Infinity,nan:NaN}}],
 ['nested_optional',{method:'post',data:{nested:{present:1,absent:undefined},array:[undefined,null,1]}}],
 ['unicode_whitespace',{method:'post',params:{memo:'\uFEFFx\u0085'},data:{memo:'\uFEFF_value\u0085'}}],
 ['invalid_body_symbol',{method:'post',data:{symbol:42}}],
 ['invalid_param_symbol',{params:{symbol:42}}],
 ['array_body',{method:'post',data:[1,undefined,'_value']}],
 ['string_body',{method:'post',data:'abc'}],
 ['raw_json_body',{method:'post',data:'{"symbol":"BTCUSDT"}'}],
 ['null_body',{method:'post',data:null}],
 ['false_body',{method:'post',data:false}],
 ['params_empty',{params:{}}],
 ['params_optional',{params:{symbol:'BTC$USDT$',optional:undefined,nil:null}}],
 ['params_reserved',{params:{symbol:'BTCUSDT',memo:'a&b=c + /',list:['x y','z'],nested:{a:'?'}}}],
 ['existing_query',{url:'/api/v2/mix/order/detail?existing=1',params:{x:2}}],
 ['kline_symbol',{url:protocol.ENDPOINTS.KLINE,params:{symbol:'BTC$USDT$',granularity:'1m'}}],
 ['remote_signer',{method:'post',data:{symbol:'BTC$USDT$',_tag:'_value'}},{remoteSigner:true}],
 ['no_secret',{method:'post',data:{symbol:'BTC$USDT$'}},{secret:undefined}],
];
// ts-bug: replacement strings expand dollar tokens inside serialized JSON.
for(const [name,key]of [['dollar_dollar','_$$'],['match','_a$&b'],['prefix','_$`'],['suffix',"_$'"]]) {
 requests.push(['key_'+name,{method:'post',data:{before:'x',[key]:'value',after:'y'}}]);
}
requests.push(['key_repeated_in_value',{method:'post',data:{text:'before _a$&b then _a$&b','_a$&b':'value'}}]);
const toJsonInputs=[undefined,null,false,true,0,-0,NaN,Infinity,-Infinity,1e21,1e-7,'_value',[],[undefined,'_value'],{},
 {text:'prefix _tag then',_tag:'_value',nested:{_inner:'_nested'}},
 {_tag:undefined,text:'_tag then _tag'},
 {_tag:null,text:'_tag then _tag'},
 {z:1,'10':2,'2':3,_tag:'value'},
 {text:'astral 😀 BOM\uFEFF NEL\u0085 NUL\0',_tag:'_value'},
 1n,{nested:1n}];
for(const key of ['_','__','_tag','_$$','_a$&b','_$`',"_$'",'_😀'])
 for(const text of ['',key,key+' then '+key,'"_'+key])
  for(const value of [undefined,null,'_value'])toJsonInputs.push({text,[key]:value,after:key});
for(const first of ['_','__','_tag','_$$','_a$&b','_$`',"_$'",'_😀'])
 for(const second of ['_','__','_tag','_$$','_a$&b','_$`',"_$'",'_😀'])
  toJsonInputs.push({text:first+' '+second,[first]:undefined,[second]:'_value',tail:first+second});
const encodeInput=value=>{
 if(value===undefined)return ['undefined'];if(value===null)return ['null'];
 if(typeof value==='number')return ['number',Object.is(value,-0)?'-0':String(value)];
 if(typeof value==='bigint')return ['bigint',String(value)];
 if(typeof value==='string')return ['string',value];if(typeof value==='boolean')return ['bool',value];
 if(Array.isArray(value))return ['array',value.map(encodeInput)];
 return ['object',Object.entries(value).map(([k,v])=>[k,encodeInput(v)])];
};
const toJsonCases=toJsonInputs.map((value,index)=>{
 let result;try{result={value:api.toJsonString(value)};}catch(error){result={error:{name:error.name,message:error.message}};}
 return {name:'toJsonString:'+index,input:encodeInput(value),result};
});
const toJsonSurrogates=[String.raw`{"_\ud800":"_value"}`,String.raw`"\udfff"`].map(escaped=>({escaped,result:api.toJsonString(JSON.parse(escaped))}));
(async()=>{
 const cases=[];
 for(const [name,request,overrides]of requests){
  captures=[];
  const options=name==='public'?{}:{key:'fixture-key',secret:'fixture-secret',passphrase:'fixture-passphrase',...overrides};
  let wire;
  const xhr=api.createAPI(options,{name:'bitget',emitAndLogError:()=>{throw Error('Unexpected emitted error');}});
  let error;
  try {
   await xhr.request({url:protocol.ENDPOINTS.PLACE_ORDER,...request,adapter:async config=>{
    wire={method:config.method,url:xhr.getUri(config),data:config.data===undefined?{$undefined:true}:config.data,headers:config.headers.toJSON()};
    return {status:200,statusText:'OK',headers:{},config,data:{code:'00000',data:{}}};
   }});
  }catch(e){error={name:e.name,message:e.message};}
  cases.push({name,preimages:captures,wire,error});
 }
 const output=JSON.stringify({cases,toJsonCases,toJsonSurrogates})+'\n';
 const fixture=path.join(__dirname,'../tests/signing-reference.json');
 if(process.argv.includes('--write'))fs.writeFileSync(fixture,output);
 else if(process.argv.includes('--check')){assert.equal(fs.readFileSync(fixture,'utf8'),output,'Bitget TypeScript signing reference drift');console.log(`Bitget signing reference: ${cases.length} hashing-boundary and adapter captures (Rust API still pending)`);}
 else process.stdout.write(output);
})().catch(error=>{console.error(error);process.exitCode=1;});
