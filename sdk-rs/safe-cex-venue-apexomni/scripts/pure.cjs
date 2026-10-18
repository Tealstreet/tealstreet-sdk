const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createRequire} = require('node:module');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const dependency = createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'package.json'));
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni');
function declarations(file, wanted) {
  const source = fs.readFileSync(path.join(root,file),'utf8');
  const tree = ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true);
  const selected=[];
  for (const node of tree.statements) {
    const names = ts.isFunctionDeclaration(node) ? [node.name?.text] : ts.isVariableStatement(node) ? node.declarationList.declarations.map(d=>d.name.getText(tree)) : [];
    if (names.some(name=>wanted.has(name))) { selected.push(node.getText(tree)); for(const name of names) wanted.delete(name); }
  }
  assert.equal(wanted.size,0,'actual helper declaration closure');
  return selected.join('\n');
}
const source = [
  declarations('apexomni.utils.ts',new Set(['hmacSha256Base64'])),
  declarations('apexomni.exchange.ts',new Set(['textEncoder','hashClientOrderId','feeHelper'])),
  declarations('apexomni.api.ts',new Set(['serializeApexPayload','serializeApexPayloadLegacy'])),
].join('\n');
const compiled = ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
const api = new vm.Script(compiled+'\n({hmacSha256Base64,hashClientOrderId,feeHelper,serializeApexPayload,serializeApexPayloadLegacy})').runInNewContext({exports:{},Buffer,TextEncoder,
  ...dependency('@noble/hashes/hmac.js'), ...dependency('@noble/hashes/sha2.js'), ...dependency('@noble/hashes/utils.js'),
  BigNumber:dependency('bignumber.js').default,
  qs:dependency('qs'),
});
const strings = ['', 'hello', 'é', '😀', '\u0000', '\ud800', '\udfff', '\ud800A', 'A\udfff', '\ud800\ud800', '\udfff\ud800', '\udbff\udfff'];
const units = value=>Array.from({length:value.length},(_,i)=>value.charCodeAt(i));
const hashes = strings.map(input=>({units:units(input),bytes:Array.from(api.hashClientOrderId(input))}));
const hmacs = strings.flatMap(message=>strings.map(key=>({message:units(message),key:units(key),output:api.hmacSha256Base64(message,key)})));
function result(call) { try { return {ok:true,value:call()}; } catch(error) { return {ok:false,message:error.message}; } }
function bits(value) { const buffer=Buffer.alloc(8);buffer.writeDoubleBE(value);return Number.isNaN(value)?'7ff8000000000000':buffer.toString('hex'); }
const payloads=[null,undefined,0,false,'','a=0&b=false',{},[],[0,false,null,'x'],{z:0,a:false,b:'',n:null,u:undefined,s:'a b+!'},{b:{z:1,a:0},a:[null,false,0,'é😀']},{é:1,E:2,e:3,'10':4,'2':5},{NaN:NaN,inf:Infinity,negativeZero:-0,big:9007199254740993n}];
function native(value) {
  if(value===undefined)return {kind:'undefined'};
  if(value===null)return {kind:'null'};
  if(typeof value==='number')return {kind:'number',bits:bits(value)};
  if(typeof value==='bigint')return {kind:'bigint',value:String(value)};
  if(Array.isArray(value))return {kind:'array',values:value.map(native)};
  if(typeof value==='object')return {kind:'object',values:Object.entries(value).map(([key,v])=>[key,native(v)])};
  return {kind:typeof value,value};
}
const queries=payloads.map(input=>({input:native(input),primary:result(()=>api.serializeApexPayload(input)),legacy:result(()=>api.serializeApexPayloadLegacy(input))}));
const fees=[];
for(const input of ['0','-0','1.005','-1.005','0.0000001','999.995','NaN','Infinity','-Infinity','1e21']) {
  for(const digits of [0,2,6,18,-1,101]) {
    for(const operation of ['floor','ceil','round']) {
      fees.push({input,digits:bits(digits),operation,result:result(()=>api.feeHelper(input,digits,operation))});
    }
  }
}
assert.equal(hashes.length,12); assert.equal(hmacs.length,144); assert.equal(queries.length,13); assert.equal(fees.length,180);
console.log(JSON.stringify({header:{node:process.version,nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH,locale:Intl.DateTimeFormat().resolvedOptions().locale},hashes,hmacs,queries,fees}));
