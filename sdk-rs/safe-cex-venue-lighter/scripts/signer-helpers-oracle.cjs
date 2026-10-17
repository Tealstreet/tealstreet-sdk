const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const source=fs.readFileSync(path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/lighter/lighter.signer.ts'),'utf8');
const ast=ts.createSourceFile('signer.ts',source,ts.ScriptTarget.Latest,true);
const names=['getProvider','isLighterInvalidNonceError'];
const funcs=ast.statements.filter(ts.isFunctionDeclaration).filter(f=>names.includes(f.name.text));
assert.equal(funcs.length,names.length,'signer helper inventory');
const out={};
vm.runInNewContext(ts.transpileModule(funcs.map(f=>f.getText(ast)).join('\n')+'\n'+names.map(n=>`exports.${n}=${n};`).join('\n'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,{exports:out});
const outcome=f=>{try{const v=f();return v===undefined?{undefined:true}:{ok:v};}catch(error){return {error:{name:error.name,message:error.message}};}};
const inputs=[undefined,null,false,true,0,21104,21104.5,'','21104',' 21104','invalid nonce','INVALID NONCE','invalid\uFEFFnonce','invalid\u0085nonce','invalid nonce tail','İnvalid nonce','INVaLID NONCE',[],[21104],['invalid nonce'],{}, {toString:null},{toString:false},{toString:0},{toString:'invalid nonce'},[{toString:null}], {message:'invalid nonce'}, {code:21104,message:'other'}, {code:'21104',message:{toString:null}}];
const identityRecords=require('./collection-identity.cjs').records;
inputs.push(...identityRecords);
const nonce=[];
for(const value of inputs)nonce.push({...value===undefined?{}:{value},expected:outcome(()=>out.isLighterInvalidNonceError(value))});
for(const code of inputs)for(const message of inputs){const value={code,message};nonce.push({value,expected:outcome(()=>out.isLighterInvalidNonceError(value))});}
const provider=[];
for(const extra of inputs){provider.push({...extra===undefined?{}:{extra},expected:outcome(()=>out.getProvider({extra}))});}
for(const value of inputs){const extra={lighterSignerProvider:value};provider.push({extra,expected:outcome(()=>out.getProvider({extra}))});}
for(const value of ['tealstreet-sdk','unknown']){const extra={lighterSignerProvider:value};provider.push({extra,expected:outcome(()=>out.getProvider({extra}))});}
for(const [index,value] of identityRecords.entries()){const extra={lighterSignerProvider:value,shadow:identityRecords[1-index]};provider.push({extra,expected:outcome(()=>out.getProvider({extra}))});}
const nativeNonce = [];
for (const [label, code, message] of [['ordinary', 400, 'other'], ['code', 21104, 'other'], ['message', 400, 'INVALID NONCE']]) {
 const value = {code, message, details:'\ud800'}; value.cycle = value;
 nativeNonce.push({label, expected:outcome(()=>out.isLighterInvalidNonceError(value))});
}
for (const mode of ['message-throw', 'string-hint', 'inherited', 'symbol']) {
 const trace = [], sentinel = {}, code = {toString(){trace.push('string');return '21104';},valueOf(){throw sentinel;}};
 let value;
 if (mode === 'message-throw') value = Object.defineProperties({}, {code:{get(){trace.push('code');return 21104;}},message:{get(){trace.push('message');throw sentinel;}}});
 if (mode === 'string-hint') value = {code, message:'other'};
 if (mode === 'inherited') value = Object.create({code:21104,message:'other'});
 if (mode === 'symbol') value = {code:Symbol.iterator,message:Symbol.iterator};
 let expected;
 try { expected={ok:out.isLighterInvalidNonceError(value)}; } catch(error) { expected={same:error===sentinel}; }
 nativeNonce.push({label:mode, expected, trace});
}
process.stdout.write(JSON.stringify({nonce,provider,nativeNonce}));
