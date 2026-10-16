// Execute maintained complete modules; assertions observe native JS values before JSON.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'BEHAVIOUR-COMPILER-REQUIRED');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const packageRequire = createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH, '../package.json'));
const source = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/cryptocom');
function execute(file, suffix, dependency) {
  const filename = path.join(source, file);
  const compiled = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
    fileName: filename, reportDiagnostics: true,
    compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true},
  });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0, 'BEHAVIOUR-SOURCE-TRANSPILE');
  const context = vm.createContext({exports: {}, require: dependency});
  vm.runInContext(compiled.outputText + suffix, context, {filename});
  return context.exports;
}
const types = execute('cryptocom.types.ts', '', name => {throw Error('unexpected type import: ' + name);});
const api = execute('cryptocom.api.ts', '\nexports.__public = isPublicEndpoint; exports.__write = isNonRetryablePrivateWrite;', name => {
  if (name === './cryptocom.types') return types;
  if (['axios-retry', '../../safeCexAxiosCreate', './cryptocom.signing'].includes(name)) return {};
  throw Error('unexpected API import: ' + name);
});
const retained = types.PUBLIC_ENDPOINTS;
assert.equal(retained.length,9);
const table = [api.__public('/audit/custom'),api.__public('/public/get-book')];
retained.push('/audit/custom');table.push(api.__public('/audit/custom'));
retained.splice(1,1);table.push(api.__public('/public/get-book'));
retained.length=2; delete retained[0]; retained[1]='\ud83d';table.push(api.__public('😀'),api.__public(''));
retained[3]='\ud800';table.push(api.__public('a\ud800b'));
delete retained[1];table.push(api.__public('😀'));
retained[0]='';table.push(api.__public(''));
assert.equal(retained,types.PUBLIC_ENDPOINTS,'PUBLIC-SOURCE-RETAINED-IDENTITY');
assert.deepEqual(table,[false,true,true,false,true,false,true,false,true],'PUBLIC-SOURCE-INDEPENDENT-SEQUENCE');
let now = () => 1000;
const signing = execute('cryptocom.signing.ts', '', name => {
  if (name === 'create-hmac') return packageRequire('create-hmac');
  if (name === '../../utils/virtualClock') return {virtualClock: {getCurrentTimeMs: () => now()}};
  throw Error('unexpected signing import: ' + name);
});
let params={counter:1,child:{value:2},array:[{value:3},4],negzero:-0};
const old=params, child=params.child, array=params.array, element=array[0];
const request=signing.generateAuthenticatedRequest('method',params,'key','secret',97);
assert.equal(request.params,params,'PARAMS-SOURCE-ROOT-IDENTITY');
const signed=request.sig;
params.counter=7;const counter=request.params.counter;
child.value=8;const childValue=request.params.child.value;
element.value=9;const arrayChild=request.params.array[0].value;
request.params.reverse=11;const reverse=params.reverse;
const primitive=array[1];array[1]=5;
params.child={value:10};const oldChild=child.value;
params={replacement:true};
const oldRoot=request.params===old;
let scalar=3;const scalarRequest=signing.generateAuthenticatedRequest('method',scalar,'key','secret',97);scalar={replacement:true};
const observations={counter,child:childValue,arrayChild,reverse,primitive,oldChild,oldRoot,scalar:scalarRequest.params,signedFrozen:request.sig===signed,negativeZero:Object.is(request.params.negzero,-0),keys:Object.keys({z:1,a:2})};
assert.deepEqual(observations,{counter:7,child:8,arrayChild:9,reverse:11,primitive:4,oldChild:8,oldRoot:true,scalar:3,signedFrozen:true,negativeZero:true,keys:['z','a']},'PARAMS-SOURCE-INDEPENDENT-OBSERVATIONS');
const clockParams={counter:1};let calls=0;
now=()=>{clockParams.counter=7+calls;return 1000+calls++;};
const clockRequest=signing.generateAuthenticatedRequest('method',clockParams,'key','secret');
assert.equal(calls,2,'PARAMS-SOURCE-CLOCK-COUNT');
assert.equal(clockRequest.params.counter,8);
const clockDigest=signing.generateSignature('method',1000,'key','secret',{counter:8},1001);
assert.equal(clockRequest.sig,clockDigest,'PARAMS-SOURCE-CLOCK-BEFORE-SIGNING');
let diamond={leaf:0};
for(let i=0;i<100;i++) diamond={a:diamond,b:diamond};
now=()=>1000;
const diamondDigest=signing.generateAuthenticatedRequest('method',diamond,'key','secret',97).sig;
let arrayDiamond=[];
for(let i=0;i<100;i++) arrayDiamond=[arrayDiamond,arrayDiamond];
let errorBeforeLargeSibling=false;
try { signing.generateAuthenticatedRequest('method',{z:arrayDiamond,a:{b:{c:{toString:0}}}},'key','secret'); }
catch(error) { assert.equal(error.name,'TypeError'); assert.equal(error.message,'Cannot convert object to primitive value'); errorBeforeLargeSibling=true; }
assert.equal(errorBeforeLargeSibling,true,'PARAMS-SOURCE-EARLY-ERROR');
const privateObservations=[undefined,'','\ud800/private/create-order','/private/\ud800create-order','/private/cancel-order\udfff','prefix/private/advanced/cancel-order?x=1','/public/get-book'].map(value=>api.__write(value));
assert.deepEqual(privateObservations,[false,false,true,false,true,true,false],'PRIVATE-SOURCE-UTF16');
const first=types.mapCandlestickInterval('toString'),second=types.mapCandlestickInterval('toString');
assert.equal(first,second,'INTERVAL-SOURCE-IDENTITY');first.auditMarker=7;assert.equal(second.auditMarker,7,'INTERVAL-SOURCE-LIVE-REFERENT');
let errorUnits;try{types.mapCandlestickInterval('\ud800');}catch(error){errorUnits=Array.from({length:error.message.length},(_,i)=>error.message.charCodeAt(i));}
assert.deepEqual(errorUnits,[...Array.from('Unsupported Crypto.com candlestick interval: ',c=>c.charCodeAt(0)),0xd800]);
const interval={same:first===second,marker:second.auditMarker,errorUnits};delete first.auditMarker;
console.log(JSON.stringify({table,params:observations,clockDigest,diamondDigest,errorBeforeLargeSibling,private:privateObservations,interval}));
