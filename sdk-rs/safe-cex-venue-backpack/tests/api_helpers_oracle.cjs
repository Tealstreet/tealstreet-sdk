const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const venue = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/backpack');
function execute(file, suffix = '') {
  const source = fs.readFileSync(path.join(venue,file),'utf8') + suffix;
  const compiled = ts.transpileModule(source, {fileName:file,reportDiagnostics:true,
    compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
  assert.equal(compiled.diagnostics.length,0,'actual source transpiles');
  const exports = {};
  const allowedUnused = new Set(['axios-retry','qs','../../remoteSigningAdapter','../../safeCexAxiosCreate',
    '../../utils/httpFailure','./backpack.signing','../../utils/retryReadsOnly']);
  const requireUnused = id => {
    if(id === './backpack.types') return execute('backpack.types.ts');
    assert.ok(allowedUnused.has(id), `unexpected dependency ${id}`);
    return new Proxy({}, {get(_, key){if(key === "__esModule")return false;throw new Error(`out-of-scope dependency used: ${id}`);}});
  };
  vm.runInNewContext(compiled.outputText, {exports,require:requireUnused}, {filename:file});
  return exports;
}
const api = execute('backpack.api.ts','\nexport { isRecord as oracleIsRecord, isPublicEndpoint as oracleIsPublicEndpoint };');
const types = execute('backpack.types.ts');
function bits(value) {
  assert.equal(typeof value,'number','numeric typeof before serialization');
  if(Number.isNaN(value))return '7ff8000000000000';
  const buffer=Buffer.alloc(8);buffer.writeDoubleBE(value);return buffer.toString('hex');
}
function observe(value) {
  if(value===undefined)return ['undefined'];
  if(value===null)return ['null'];
  switch(typeof value) {
    case 'boolean':return ['boolean',value];
    case 'number':return ['number',bits(value)];
    case 'bigint':return ['bigint',String(value)];
    case 'string':return ['string',value];
    case 'object':return Array.isArray(value)?['array',value.map(observe)]:['object',Object.keys(value).map(k=>[k,observe(value[k])])];
    default:throw new Error('unsupported capture type '+typeof value);
  }
}
function object(entries) {
  const result={};
  for(const [key,value] of entries)Object.defineProperty(result,key,{value,enumerable:true,configurable:true,writable:true});
  return result;
}
const values = [undefined,null,false,true,0,-0,1.125,NaN,Infinity,-Infinity,'','object',123n,[],[{}],[{},{}],
  [null],[undefined],[{},null],{}, {a:undefined}, {a:1,b:2},
  object([['10',10],['2',2],['01',1],['__proto__','own proto'],['constructor','own ctor']]),
  {collision:'query',nested:{n:-0}}, {collision:'body',nested:{n:Infinity}},
  {numbers:[-0,NaN,Infinity,-Infinity],a:123n}, 'λ🚀', {'λ🚀':'日本語'}, {'1':1}, {'0':0}];
const records=values.map(value=>({input:observe(value),expected:api.oracleIsRecord(value)}));
const collect=[];
for(const query of values)for(const body of values) {
  const input={query:observe(query),body:observe(body)};
  try {collect.push({...input,ok:true,expected:observe(api.collectAuthParams(query,body))});}
  catch(error) {collect.push({...input,ok:false,error:error.message});}
}
const windows=[undefined,NaN,Infinity,-Infinity,-0,0,-Number.MIN_VALUE,Number.MIN_VALUE,-10,0.1,0.5,0.999,1,1.99,4999.999,5000,59999.999,60000,60000.1,60001,Number.MAX_VALUE];
for(let p=-1074;p<1024;p+=53) windows.push(2**p,-(2**p));
const auth=windows.map(value=>({input:value===undefined?null:bits(value),expected:bits(api.resolveBackpackAuthWindow(value))}));
const endpoints=['','/','/api/v1/unknown',...Object.values(types.ENDPOINTS)];
const urls=endpoints.flatMap(url=>[url,'https://example.invalid'+url,url+'/suffix','/prefix?next='+url,url.toUpperCase(),url.replace('/v1/','/v10/')]);
urls.push('/api/v1/orders/api/v1/capital/collateral','/api/v1/order/history/orders','/api/v1/capital/collateral/history/fills','/wapi/v1/capital/deposits/api/v1/account');
const publicCases=urls.map(url=>({url,expected:api.oracleIsPublicEndpoint(url)}));
const instructions=urls.flatMap(url=>['GET','POST','DELETE','PATCH','PUT','HEAD','get','', 'λ🚀'].map(method=>({method,url,expected:api.getInstruction(method,url)})));
const batch=[{nested:{n:1}}];const batchOutput=api.collectAuthParams(undefined,batch);const arrayBefore=observe(batchOutput);
batch[0].nested.n=2;
const body={nested:{n:3}};const bodyOutput=api.collectAuthParams({},body);const nestedBefore=observe(bodyOutput);
body.nested.n=4;
const alias={arraySame:batchOutput===batch,arrayBefore,arrayAfter:observe(batchOutput),nestedSame:bodyOutput.nested===body.nested,nestedBefore,nestedAfter:observe(bodyOutput)};
// The chosen prepared-data boundary excludes host objects, executable getters,
// symbols and sparse arrays. Capture source behavior rather than calling them JSON.
let getterCalls=0;const getterObject={get field(){getterCalls++;return 9;}};
const getterOutput=api.collectAuthParams({},getterObject);
const symbol=Symbol('key');const symbolObject={[symbol]:7};const symbolOutput=api.collectAuthParams({},symbolObject);
const sparse=new Array(1);const sparseOutput=api.collectAuthParams({},sparse);
const outside={dateIsRecord:api.oracleIsRecord(new Date(0)),functionIsRecord:api.oracleIsRecord(()=>{}),getterCalls,getterOutput:observe(getterOutput),symbolKeys:Reflect.ownKeys(symbolOutput).filter(k=>typeof k==='symbol').length,sparseSame:sparseOutput===sparse,sparseHasZero:0 in sparseOutput};
const utf16=['/api/v1/time\ud800','/api/v1/orders\udc00'].map(url=>{
 assert.equal(typeof url,'string','URL typeof before serialization');
 return {raw:JSON.stringify(url),units:Array.from({length:url.length},(_,i)=>url.charCodeAt(i)),public:api.oracleIsPublicEndpoint(url),instruction:api.getInstruction('GET',url)};
});
const utf16Data=['\ud800',object([['\udc00',1]]),{field:'\ud800'}].map(value=>{
 const result=api.collectAuthParams({},value);
 const strings=[...(typeof value==='string'?[value]:Object.keys(value).concat(Object.values(value).filter(v=>typeof v==='string')))];
 return {raw:JSON.stringify(value),record:api.oracleIsRecord(value),resultRecord:api.oracleIsRecord(result),output:Object.keys(result).map(k=>({key:Array.from({length:k.length},(_,i)=>k.charCodeAt(i)),value:typeof result[k]==='string'?['units',Array.from({length:result[k].length},(_,i)=>result[k].charCodeAt(i))]:observe(result[k])})),units:strings.map(s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)))};
});
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},utf16Data,auth,records,collect,publicCases,instructions,alias,outside,utf16}));
