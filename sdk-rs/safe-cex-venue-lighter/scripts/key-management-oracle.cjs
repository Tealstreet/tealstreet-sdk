'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const file='packages/lighter-sdk/src/signer/key-manager.ts',root=path.resolve(__dirname,'../../..');
const tree=ts.createSourceFile(file,fs.readFileSync(path.join(root,file),'utf8'),ts.ScriptTarget.Latest,true);assert.equal(tree.parseDiagnostics.length,0);
const names=['KeyPair','KeyManagerError','isValidPrivateKey','isValidPublicKey','normalizePrivateKey','normalizePublicKey','secureCompareKeys'];
const members=names.map(name=>{const ds=tree.statements.filter(n=>n.name?.text===name);assert.equal(ds.length,1);return{file,name,body:ds[0].getText(tree)}});
const code=ts.transpileModule(members.map(m=>m.body).join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS},reportDiagnostics:true});assert.equal(code.diagnostics.length,0);const api={};new Function('exports','Buffer',code.outputText)(api,Buffer);
const units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i)),str=s=>({kind:'string',units:units(s)}),U={kind:'undefined'};
const decode=v=>v.kind==='string'?String.fromCharCode(...v.units):v.kind==='buffer'?Buffer.from(v.bytes):v.kind==='uint8'?Uint8Array.from(v.bytes):v.kind==='marker'?marker:undefined;
const marker={opaque:'cause'};
function cause(v){if(v===undefined)return'undefined';if(v===marker)return'marker';throw Error('unexpected cause')}
const rows=[];function run(method,args){let result,error=null;try{if(method==='KeyManagerError.constructor'){const e=new api.KeyManagerError(decode(args[0]),decode(args[1]));result={name:e.name,message:units(e.message),cause:cause(e.cause),instance:e instanceof api.KeyManagerError,errorInstance:e instanceof Error};}else{const v=api[method](...args.map(decode));result=typeof v==='string'?str(v):v;}}catch(e){error={name:e.name,message:units(e.message),cause:cause(e.cause),instance:e instanceof api.KeyManagerError,errorInstance:e instanceof Error};}rows.push({method,args,result:error?null:result,error})}
const keys=['','a','a'.repeat(79),'a'.repeat(80),'a'.repeat(81),'0123456789abcdef'.repeat(5),'ABCDEF0123456789'.repeat(5),'0x'+'a'.repeat(78),'g'+'a'.repeat(79),'a'.repeat(79)+'\n','a'.repeat(79)+'\ud800','a'.repeat(78)+'😀','a'.repeat(79)+'\u0000','a'.repeat(79)+'\uff46','a'.repeat(79)+'F'];
for(const method of ['isValidPrivateKey','isValidPublicKey'])for(const key of keys)run(method,[str(key)]);
const byteValues=[[],[0],Array(39).fill(0),Array(40).fill(0),Array.from({length:40},(_,i)=>(i*7)%256),Array(40).fill(255),Array(41).fill(128)];
for(const method of ['normalizePrivateKey','normalizePublicKey']){
for(const key of [...keys,'0x'+'Ab'.repeat(40),'0X'+'ab'.repeat(40),'0x0x'+'a'.repeat(76),'0x'+'a'.repeat(79)+'\ud800'])run(method,[str(key)]);
for(const kind of ['buffer','uint8'])for(const bytes of byteValues)run(method,[{kind,bytes}]);run(method,[U]);}
for(const[a,b]of [['',''],['','a'],['abc','abc'],['abc','abd'],['abc','xbc'],['abc','ab'],['\ud800','\ud800'],['\ud800','\ud801'],['😀','😁'],['a'.repeat(80),'a'.repeat(79)+'b'],['a'.repeat(80),'b'+'a'.repeat(79)],['A','a'],['\u0000','\u0000'],['\uffff','\u0000'],['\ud800\udc00','\ud801\udc00']])run('secureCompareKeys',[str(a),str(b)]);
for(const message of ['','plain','bad\ud800key','😀'])for(const c of [U,{kind:'marker'}])run('KeyManagerError.constructor',[str(message),c]);
const expected={isValidPrivateKey:15,isValidPublicKey:15,normalizePrivateKey:34,normalizePublicKey:34,secureCompareKeys:15,'KeyManagerError.constructor':8};for(const[name,count]of Object.entries(expected))assert.equal(rows.filter(r=>r.method===name).length,count,'independent '+name+' floor');assert.equal(rows.length,121,'independent total floor');
const payload={members,rows},contentHash=require('node:crypto').createHash('sha256').update(JSON.stringify(payload)).digest('hex');process.stdout.write(JSON.stringify({...payload,contentHash}));
