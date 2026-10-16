const assert=require('node:assert/strict');
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const {createRequire}=require('node:module');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const actualRequire=createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'../../packages/safe-cex/package.json'));
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/backpack/backpack.signing.ts');
const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
assert.equal(compiled.diagnostics.length,0);
const api={};
vm.runInNewContext(compiled.outputText,{exports:api,Buffer,TextEncoder,require(id){
 if(id==='../../utils/virtualClock')return {virtualClock:{getCurrentTimeMs(){throw Error('verify must not read clock');}}};
 assert.ok(['@noble/ed25519','@noble/hashes/sha2.js'].includes(id));return actualRequire(id);
}},{filename:file});
const noble=actualRequire('@noble/ed25519');
const bits=n=>{assert.equal(typeof n,'number');if(Number.isNaN(n))return '7ff8000000000000';const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex');};
function observe(v){
 if(v===null)return ['null'];if(v===undefined)return ['undefined'];
 if(typeof v==='number')return ['number',bits(v)];
 if(typeof v==='string'||typeof v==='boolean')return [typeof v,v];
 if(Array.isArray(v))return ['array',v.map(observe)];
 return ['object',Object.keys(v).map(k=>[k,observe(v[k])])];
}
const b64=b=>Buffer.from(b).toString('base64');
function variants(raw){const clean=raw.replace(/=/g,'');return [raw,clean,raw.replace(/\+/g,'-').replace(/\//g,'_'),raw.match(/.{1,4}/g).join(' !\n'),raw+'ignored!',Array.from(clean,c=>String.fromCharCode(c.charCodeAt(0)+256)).join('')];}
async function main(){
 const cases=[];
 async function add(label,instruction,input,timestamp,window,signature,key,boundary=false){
  const actual=await api.verifySignature(instruction,input,timestamp,window,signature,key);
  assert.equal(typeof actual,'boolean','source native result type');
  cases.push({label,instruction,input:observe(input),timestamp:bits(timestamp),window:bits(window),signature,key,actual,boundary});
 }
 const inputs=[{}, {symbol:'λ🚀',price:1.25,quantity:3.5,n:-0},[{symbol:'BTC',price:7.25},{symbol:'ETH',price:9.5}]];
 const numbers=[-0,0,NaN,Infinity,-Infinity,1e21];
 for(let seedIndex=0;seedIndex<3;seedIndex++){
  const seed=Uint8Array.from({length:32},(_,i)=>(i+seedIndex*67)%256);
  const key=b64(await noble.getPublicKeyAsync(seed));
  for(let p=0;p<inputs.length;p++){
   const input=inputs[p],instruction=p===2?'orderExecute':'balanceQuery';
   const timestamp=numbers[seedIndex+p],window=numbers[seedIndex+p+1];
   const signature=await api.signRequest(instruction,input,timestamp,window,b64(seed));
   for(const s of variants(signature))for(const k of variants(key))await add('valid',instruction,input,timestamp,window,s,k);
   await add('changed instruction',instruction+'X',input,timestamp,window,signature,key);
   await add('changed timestamp',instruction,input,42,window,signature,key);
   await add('changed window',instruction,input,timestamp,43,signature,key);
   await add('changed params',instruction,p===2?[{different:true}]:{different:true},timestamp,window,signature,key);
   const corrupt=Buffer.from(signature,'base64');corrupt[0]^=1;
   await add('changed signature',instruction,input,timestamp,window,b64(corrupt),key);
   const wrong=Buffer.from(key,'base64');wrong[0]^=1;
   await add('changed key',instruction,input,timestamp,window,signature,b64(wrong));
   for(const n of [0,1,31,33,64])await add('key length '+n,instruction,input,timestamp,window,signature,b64(Buffer.alloc(n,7)));
   for(const n of [0,1,32,63,65])await add('signature length '+n,instruction,input,timestamp,window,b64(Buffer.alloc(n,7)),key);
   for(const bad of [null,[],[null],[[]]])await add('payload refusal',instruction,bad,timestamp,window,signature,key);
  }
 }
 // ZIP215 permits the identity key and identity R with scalar zero. The shared
 // native primitive deliberately uses strict verification; retain the boundary.
 const identity=Buffer.alloc(32);identity[0]=1;
 const weakSignature=Buffer.alloc(64);weakSignature[0]=1;
 for(const input of inputs)await add('weak identity key',Array.isArray(input)?'orderExecute':'balanceQuery',input,7,5000,b64(weakSignature),b64(identity),true);
 process.stdout.write(JSON.stringify({runtime:{node:process.version,arch:process.arch,platform:process.platform},cases}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
