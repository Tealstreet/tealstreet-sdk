const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni/zklink/signer/utils.ts');
let source = fs.readFileSync(file,'utf8');
const mutations = {
 timestamp:['Math.floor(Date.now() / 1000)', 'Math.ceil(Date.now() / 1000)'],
 assert:['throw new Error(message)', 'throw new Error(message + "_MUTATED")'],
 assertTypeArray:['value instanceof Array', 'false'],
 assertTypeBoolean:["typeof value === 'boolean'", 'false'],
 assertTypeNumber:["typeof value === 'number', message", 'false, message'],
 assertTypeString:["typeof value === 'string', message", 'false, message'],
 assertSubAccountId:['// noop', 'throw new Error("MUTATED noop");'],
};
if (mutations[process.env.APEXOMNI_SIGNER_MUTATE_TS]) {
 const [from,to] = mutations[process.env.APEXOMNI_SIGNER_MUTATE_TS];
 assert.equal(source.split(from).length,2,'mutation applies exactly once'); source=source.replace(from,to);
}
const validatorMutations = {
 assertChainId:['value > 0','value <= 0'],
 assertAccountId:['value > 0','value <= 0'],
 assertNonce:['value >= 0','value > 0'],
 assertTokenId:['value > 0','value <= 0'],
 assertPubKeyHash:['value.length === 42','value.length === 41'],
 assertTs:['value > 1700000000','value >= 1700000000'],
 assertWithdrawFeeRatio:['value <= 10000','value < 10000'],
 assertTradingFeeRatio:['value >= -255','value > -255'],
};
const requested=process.env.APEXOMNI_SIGNER_MUTATE_TS;
if (validatorMutations[requested]) {
 const parsed=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
 const declarations=parsed.statements.filter(n=>ts.isFunctionDeclaration(n)&&n.name?.text===requested);
 assert.equal(declarations.length,1,'unique source function');
 const node=declarations[0];const begin=node.getStart(parsed),end=node.getEnd();
 const [from,to]=validatorMutations[requested];const body=source.slice(begin,end);
 assert.equal(body.split(from).length,2,'validator mutation applies exactly once');
 source=source.slice(0,begin)+body.replace(from,to)+source.slice(end);
}
if(requested)assert(mutations[requested]||validatorMutations[requested],'known source mutation');
const code = ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});
assert(!code.diagnostics.some(d=>d.category===ts.DiagnosticCategory.Error),'TS transpilation');
const mod = {exports:{}}; let time=0,calls=0;
const clock = {now(){calls++;return time;}};
// Prepared values and functions share the same Array realm; foreign arrays are
// captured separately because Rust prepared arrays have no JS realm identity.
new vm.Script(`(function(require,module,exports){${code.outputText}\n})`,{filename:file}).runInNewContext({Array,Date:clock})(name=>{
 assert(['ethers','zklink-web-sdk-alpha'].includes(name),`unexpected dependency ${name}`);
 return new Proxy({}, {get(_target,key){if(key==='__esModule')return true;throw Error('address and SDK initialization remain outside this corpus');}});
},mod,mod.exports);
const api=mod.exports;
function bits(n){ const b=Buffer.alloc(8);b.writeDoubleBE(n);return Number.isNaN(n)?'7ff8000000000000':b.toString('hex'); }
function outcome(fn){try{fn();return {ok:true};}catch(error){return {ok:false,name:error.name,message:error.message};}}
const values = [
 ['undefined',undefined],['null',null],['false',false],['true',true],['emptyString',''],['string','α😀'],
 ['zero',0],['negativeZero',-0],['nan',NaN],['negativeNan',-NaN],['infinity',Infinity],['negativeInfinity',-Infinity],['fraction',1.25],
 ['array',[]],['nestedArray',[1,[2]]],['object',{}],['arrayLike',{length:0}],['bigint',1n]
];
const names=['assertTypeArray','assertTypeBoolean','assertTypeNumber','assertTypeString','assertSubAccountId'];
const cases=values.map(([id,value])=>({id,type:typeof value,bits:typeof value==='number'?bits(value):null,results:Object.fromEntries(names.map(name=>[name,outcome(()=>api[name](value,`message:${name}:α😀`))]))}));
const asserts=[true,false].flatMap(condition=>['','diagnostic α😀','line\nbreak'].map(message=>({condition,message,result:outcome(()=>api.assert(condition,message))})));
const timestamps=[0,-0,1,-1,999,1000,1001,-1001,1700000000123,Number.MAX_SAFE_INTEGER,Number.MIN_VALUE,NaN,-NaN,Infinity,-Infinity].map(value=>{time=value;calls=0;const result=api.timestamp();return {inputBits:bits(value),outputBits:bits(result),type:typeof result,calls};});
const boundaries={foreignArray:outcome(()=>api.assertTypeArray(vm.runInNewContext('[]'),'foreign array')),loneSurrogateUnits:Array.from(outcome(()=>api.assert(false,'\ud800')).message,c=>c.charCodeAt(0))};
// Separately labelled supplement: existing type/clock corpus values stay intact.
function describe(value) {
 if(value===undefined)return {kind:'undefined'};
 if(value===null)return {kind:'null'};
 if(typeof value==='number')return {kind:'number',bits:bits(value)};
 if(typeof value==='bigint')return {kind:'bigint',value:String(value)};
 if(typeof value==='string'||typeof value==='boolean')return {kind:typeof value,value};
 if(Array.isArray(value))return {kind:'array',items:value.map(describe)};
 return {kind:'object',entries:Object.entries(value).map(([k,v])=>[k,describe(v)])};
}
function adjacent(value,delta){const b=Buffer.alloc(8);b.writeDoubleBE(value);b.writeBigUInt64BE(b.readBigUInt64BE()+BigInt(delta));return b.readDoubleBE();}
const validatorNames=Object.keys(validatorMutations);
const validatorValues=[...values.map(([id,value])=>[id,value]),
 ...[-1,-Number.MIN_VALUE,Number.MIN_VALUE,1,255,256,-255,-256,10000,10001,1700000000,1700000000.25,1e21,1e-7].map((n,i)=>['number'+i,n]),
 ...[255,-255,10000,1700000000].flatMap((n,i)=>[-1,1].map(d=>[`adjacent${i}:${d}`,adjacent(n,d)])),
 ['pubkeyNonhex','0x'+'z'.repeat(40)],['pubkeyAstral','0x'+'😀'.repeat(20)],['pubkeyShort','0x'+'a'.repeat(39)],['pubkeyLong','0x'+'a'.repeat(41)],['pubkeyUpper','0X'+'a'.repeat(40)],
 ['numericString','10000'],['arrayNullish',[undefined,null,-0,NaN,Infinity,1n,[null,1.25],{}]],['objectToString',{toString:null}],['nestedToString',[{toString:'not callable'}]],
];
const validators=validatorValues.flatMap(([id,value])=>['','validator α😀'].map(message=>({id,message,input:describe(value),results:Object.fromEntries(validatorNames.map(name=>[name,outcome(()=>api[name](value,message))]))})));
console.log(JSON.stringify({header:{node:process.version,nodeMajor:Number(process.versions.node.split('.')[0]),nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH},cases,asserts,timestamps,boundaries,validators}));
