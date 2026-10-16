const assert=require('node:assert/strict');const fs=require('node:fs');const path=require('node:path');const vm=require('node:vm');const {createRequire}=require('node:module');
assert.ok(process.env.SAFE_CEX_TYPESCRIPT_PATH,'SAFE_CEX_TYPESCRIPT_PATH is required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const actualRequire=createRequire(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH,'../../packages/safe-cex/package.json'));
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/backpack/backpack.signing.ts');
const compiled=ts.transpileModule(fs.readFileSync(file,'utf8'),{reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
assert.equal(compiled.diagnostics.length,0,'actual signing source transpiles');
let now=0,clockCalls=0;
const api={};
vm.runInNewContext(compiled.outputText,{exports:api,Buffer,TextEncoder,require(id){
 if(id==='../../utils/virtualClock')return {virtualClock:{getCurrentTimeMs(){clockCalls++;return now;}}};
 assert.ok(['@noble/ed25519','@noble/hashes/sha2.js'].includes(id),'actual crypto dependency allowlist');return actualRequire(id);
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

async function outcome(f,headers=false){try{const value=await f();
 if(headers){for(const [k,v] of Object.entries(value)){assert.equal(typeof k,'string');assert.equal(typeof v,'string');}return {ok:true,value:Object.entries(value)};}
 assert.equal(typeof value,'string','actual signature typeof');return {ok:true,value};
}catch(e){return {ok:false,error:e.name+': '+e.message};}}
async function main(){
const secrets=[];
for(const seed of [Buffer.alloc(32),Buffer.alloc(32,255),Buffer.from(Array.from({length:32},(_,i)=>i))]){
 const raw=seed.toString('base64');const clean=raw.replace(/=/g,'');
 secrets.push(raw,clean,raw.replace(/\+/g,'-').replace(/\//g,'_'),raw.match(/.{1,4}/g).join(' !\n'),raw+'ignored!trash',Array.from(clean,c=>String.fromCharCode(c.charCodeAt(0)+256)).join(''),clean+'A');
 const alphabet='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';const last=alphabet.indexOf(clean.at(-1));secrets.push(clean.slice(0,-1)+alphabet[last|3]);
}
for(const n of [0,1,31,33,64])secrets.push(Buffer.alloc(n,7).toString('base64'));
secrets.push('🚀'+secrets[0],secrets[0].slice(0,16)+'='+secrets[0].slice(16),'a','!@#');
const params=[{}, {z:2,a:1,symbol:'λ🚀',price:'1.25',quantity:'3.5'},[{symbol:'BTC',price:7},{symbol:'ETH',price:9}],[],[null],null];
const numbers=[-0,0,1,1e21,NaN,Infinity,-Infinity,1700000000123];
const sign=[];
for(let s=0;s<secrets.length;s++)for(let p=0;p<params.length;p++){
 const instruction=p===2?'orderExecute':'balanceQuery';const timestamp=numbers[(s+p)%numbers.length];const window=numbers[(s+p+3)%numbers.length];
 const input=params[p],secret=secrets[s];
 sign.push({instruction,input:observe(input),secret,timestamp:bits(timestamp),window:bits(window),...await outcome(()=>api.signRequest(instruction,input,timestamp,window,secret))});
}
const headers=[];
const windows=[undefined,-0,0,5000,60001,NaN,Infinity,-Infinity];
for(let s=0;s<secrets.length;s++)for(let w=0;w<windows.length;w++){
 const input=params[s%params.length];const instruction=s%params.length===2?'orderExecute':'accountQuery';now=numbers[(s+w)%numbers.length];clockCalls=0;
 const apiKey='caller-key:λ🚀:'+s;const secret=secrets[s];const window=windows[w];
 const result=await outcome(()=>api.generateAuthHeaders(instruction,input,apiKey,secret,window),true);
 headers.push({instruction,input:observe(input),apiKey,secret,timestamp:bits(now),window:window===undefined?null:bits(window),clockCalls,...result});
}
// At the chosen string boundary, source signs TextEncoder replacement bytes for
// a lone surrogate instruction and passes a lone surrogate caller key through.
now=7;clockCalls=0;const instruction='go\ud800',key='key\udc00';
const boundary=await api.generateAuthHeaders(instruction,{},key,secrets[0]);
const boundarySignature=await api.signRequest('go\ufffd',{},7,5000,secrets[0]);
assert.equal(boundary['X-Signature'],boundarySignature,'source TextEncoder replacement signature');
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
now=11;clockCalls=0;
const pending=api.generateAuthHeaders('clockProbe',{},'caller-key',secrets[0]);
const callsAtConstruction=clockCalls;now=22;
const eagerHeaders=await pending;
const eagerClock={callsAtConstruction,callsAfterAwait:clockCalls,headers:Object.entries(eagerHeaders)};
process.stdout.write(JSON.stringify({eagerClock,runtime:{node:process.version,arch:process.arch,platform:process.platform},sign,headers,boundary:{instructionRaw:JSON.stringify(instruction),keyRaw:JSON.stringify(key),keyUnits:units(boundary['X-API-Key']),signature:boundary['X-Signature'],replacementSignature:boundarySignature}}));
}
main().catch(e=>{console.error(e);process.exitCode=1;});
