// The actual TS SDK class bodies and installed Starknet signer, no network calls.
'use strict';
const assert=require('node:assert/strict'), fs=require('node:fs'), path=require('node:path'), vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
const filename=path.join(root,'packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts');
const source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
assert.equal(source.parseDiagnostics.length,0);
const declarations=['signParadexTypedData','ParadexRequestSigner','ParadexJWTManager'].map(name=>{
  const found=source.statements.filter(node=>node.name?.text===name);assert.equal(found.length,1,name);return found[0].getText(source);
});
const compiled=ts.transpileModule(declarations.join('\n'),{reportDiagnostics:true,compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}});
assert.equal(compiled.diagnostics.length,0);
const starknet=require('starknet');
const sdkAccount={account:new starknet.Account({nodeUrl:'https://fixture.invalid/rpc'},'0x123','0x'+('1'.padStart(64,'0'))),config:{paradexChainId:'PRIVATE_SN_POTC_MAINNET'}};
const inputs=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(inputs.length,3,'fixed wire signature floor');
let clock=0, authHeaders;
const moduleState={exports:{}};
vm.runInNewContext(compiled.outputText,{
 module:moduleState,exports:moduleState.exports,
 require(name){assert.equal(name,'starknet');return starknet;},
 BigNumber:require('bignumber.js'),Date:{now:()=>clock},TextEncoder,console,
 fetch:async(url,options)=>{assert.ok(url.endsWith('/auth/0x456'));assert.equal(options.method,'POST');authHeaders=options.headers;return{ok:true,json:async()=>({jwt_token:'source.jwt.token'})};},
});
(async()=>{
 const signer=new moduleState.exports.ParadexRequestSigner(sdkAccount,{});
 const outputs=[];
 for(let index=0;index<3;index++){
  const input=inputs[index];clock=input.timestamp;
  if(input.kind==='JWT'){
   const jwt=new moduleState.exports.ParadexJWTManager(sdkAccount,false,'0x456',{});
   await jwt.generateNewToken();outputs.push({kind:input.kind,headers:authHeaders});
  }else{
   const result=await signer[input.kind==='Order'?'signOrder':'signModifyOrder'](input.params);
   outputs.push({kind:input.kind,signature:result.signature,timestamp:result.signature_timestamp});
  }
 }
 const invalidAccount={...sdkAccount,account:new starknet.Account({nodeUrl:'https://fixture.invalid/rpc'},'0x123','0x'+('f'.repeat(64)))};
 const invalidJWT=new moduleState.exports.ParadexJWTManager(invalidAccount,false,'0x456',{});
 await assert.rejects(()=>invalidJWT.generateNewToken(),undefined,'actual SDK rejects an admitted key during signing');
 process.stdout.write(JSON.stringify({node:process.version,major:Number(process.versions.node.split('.')[0]),arch:process.arch,v8:process.versions.v8,rows:outputs}));
})().catch(error=>{console.error(error);process.exitCode=1;});
