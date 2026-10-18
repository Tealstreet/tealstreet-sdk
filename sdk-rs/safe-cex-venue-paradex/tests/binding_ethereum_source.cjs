'use strict';
// Actual TS initializer/JWT bodies and the checked-in Paradex SDK implementation.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),SDK=require('@tealstreet/paradex-sdk');
const filename=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts');
const source=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
const declarations=['initializeParadexSDK','signParadexTypedData','ParadexJWTManager'].map(name=>{
 const nodes=source.statements.filter(node=>node.name?.text===name);assert.equal(nodes.length,1,name);return nodes[0].getText(source);
});
const compiled=ts.transpileModule(declarations.join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
assert.equal(compiled.diagnostics.length,0);
const inputs=JSON.parse(fs.readFileSync(0,'utf8'));assert.equal(inputs.length,2,'Ethereum source case floor');
(async()=>{
 const rows=[];
 for(const input of inputs){
  let headers;
  const module={exports:{}};
  vm.runInNewContext(compiled.outputText,{module,exports:module.exports,
   require(name){assert.equal(name,'starknet');return require(name);},
   getParadexSDK:async()=>({...SDK,Config:{...SDK.Config,fetchConfig:async environment=>{
    assert.equal(environment,input.options.testnet?'testnet':'prod');return input.config;
   }}}),Wallet:require('ethers').Wallet,
   Date:{now:()=>input.timestamp},console:{log(){},error(){}},
   fetch:async(url,init)=>{assert.ok(url.endsWith('/auth/0x456'));headers=init.headers;return{ok:true,json:async()=>({jwt_token:'source.ethereum.jwt'})};},
  });
  const account=await module.exports.initializeParadexSDK(input.options);
  assert.ok(account,'actual SDK initialization must succeed');
  const jwt=new module.exports.ParadexJWTManager(account,input.options.testnet,'0x456',input.options);
  await jwt.generateNewToken();
  rows.push({address:account.account.address,headers});
 }
 assert.equal(rows.length,2,'completed Ethereum source cases');
 process.stdout.write(JSON.stringify({node:process.version,arch:process.arch,v8:process.versions.v8,rows}));
})().catch(error=>{console.error(error);process.exitCode=1;});
