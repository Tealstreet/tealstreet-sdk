const assert=require('node:assert/strict');
const fs=require('node:fs');const path=require('node:path');const vm=require('node:vm');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH,'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/backpack/backpack.signing.ts');
const source=fs.readFileSync(file,'utf8')+'\nexport { buildSortedQueryString as oracleSorted };';
const compiled=ts.transpileModule(source,{reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
assert.equal(compiled.diagnostics.length,0,'actual signing source transpiles');
const unused=()=>{throw Error('out-of-scope signing dependency invoked');};
const exportsObject={};
vm.runInNewContext(compiled.outputText,{exports:exportsObject,require(id){
 if(id==='@noble/ed25519')return {__esModule:true,hashes:{},signAsync:unused,verifyAsync:unused};
 if(id==='@noble/hashes/sha2.js')return {sha512:unused};
 if(id==='../../utils/virtualClock')return {virtualClock:{getCurrentTimeMs:unused}};
 throw Error('unexpected dependency '+id);
}},{filename:file});
function bits(n){assert.equal(typeof n,'number');if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');}
function observe(v){
 if(v===undefined)return ['undefined'];if(v===null)return ['null'];
 switch(typeof v){
 case 'number':return ['number',bits(v)];case 'bigint':return ['bigint',String(v)];case 'boolean':return ['boolean',v];case 'string':return ['string',v];
 case 'object':return Array.isArray(v)?['array',v.map(observe)]:['object',Object.keys(v).map(k=>[k,observe(v[k])])];
 default:throw Error('unsupported capture');
 }
}
function outcome(f){try{const value=f();assert.equal(typeof value,'string','preimage string typeof');return {ok:true,value};}catch(e){return {ok:false,error:e.name+': '+e.message};}}
function object(entries){return Object.fromEntries(entries);}
const rows=[{}, {z:2,a:1}, {n:null,u:undefined,no:false,zero:0,empty:''},
 {a:NaN,b:Infinity,c:-Infinity,d:-0,e:1e-7,f:1e21,g:123456789012345678901234567890n},
 {array:[null,undefined,-0,NaN,Infinity,[],[1,[false,'x,y']],{}]},
 {nested:{a:1}}, {nested:{toString:null}}, {nested:{toString:undefined}}, {nested:[{toString:1}]},
 object([['2','two'],['10','ten'],['01','one'],['🚀',3],['\ue000',4],['a&b=c','x +/?#'],['__proto__','own']]),
 {punct:"AZaz09-_.!~*'() :;/?:@&=+$,#[]%\\\"\n\t\0",unicode:'λ🚀日本語'},
 {toString:5}, {constructor:8,valueOf:6}];
const nums=[-0,0,NaN,Infinity,-Infinity,Number.MIN_VALUE,Number.MAX_VALUE,0.000001,0.0000001,1e20,1e21,9007199254740992,-123.875];
for(const n of nums)rows.push({n});
const params=[...rows,[],[{}],[rows[0],rows[1]],[null],[{},null],[undefined],[[{}]],false,1,123n,'','abc','λ🚀',null,undefined];
const sorted=params.map(input=>({input:observe(input),...outcome(()=>exportsObject.oracleSorted(input))}));
const payload=[];
for(const input of params)for(const instruction of ['orderExecute','subscribe','','λ🚀&x=1'])for(let i=0;i<nums.length;i++){
 const timestamp=nums[i],window=nums[(i+3)%nums.length];
 payload.push({input:observe(input),instruction,timestamp:bits(timestamp),window:bits(window),...outcome(()=>exportsObject.buildSigningPayload(instruction,input,timestamp,window))});
}
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const utf16=[{field:'\ud800'},object([['\udc00',1]])].map(input=>{
 const result=outcome(()=>exportsObject.oracleSorted(input));
 return {raw:JSON.stringify(input),units:result.ok?units(result.value):null,error:result.error??null};
});
const badInstruction='go\ud800';const badOutput=exportsObject.buildSigningPayload(badInstruction,{},1,2);
let getterCalls=0,toStringCalls=0;
const host={get field(){getterCalls++;return {toString(){toStringCalls++;return 'host';}};}};
const outside={value:exportsObject.oracleSorted(host),getterCalls,toStringCalls};
process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},sorted,payload,utf16,instructionUtf16:{raw:JSON.stringify(badInstruction),units:units(badOutput)},outside}));
