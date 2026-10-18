// Execute the actual credential normalizer with separately labelled live-binding
// admission profiles. Existing credential oracle inputs remain unchanged.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts');
const source=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['COINBASE_KEY_NAME_PATTERN','COINBASE_PRIVATE_KEY_PATTERN','COINBASE_ED25519_PREFIX','getCoinbaseJsonSecret','parseCoinbaseCredentialJson','base64ToBytes','normalizeCoinbaseApiCredentials'];
const selected=source.statements.filter(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>names.includes(d.name.getText(source))));
assert.equal(selected.length,names.length);
const exportsObject={};
vm.runInNewContext(ts.transpileModule(selected.map(n=>n.getText(source)).join('\n'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:exportsObject,atob});
const profiles=[
 ['valid','organizations/org/apiKeys/id'],
 ['org-slash','organizations/org/child/apiKeys/id'],
 ['org-double-slash','organizations/org//child/apiKeys/id'],
 ['key-slash','organizations/org/apiKeys/id/child'],
 ['empty-org','organizations//apiKeys/id'],
 ['unicode','organizations/雪/apiKeys/clé'],
 ['org-space','organizations/org child/apiKeys/id'],
 ['json-org-slash',JSON.stringify({name:'organizations/org/child/apiKeys/id'})],
];
const rows=profiles.map(([id,key])=>{try{const result=exportsObject.normalizeCoinbaseApiCredentials(key,Buffer.alloc(32).toString('base64'));return {id,key,keyName:result.keyName};}catch(error){return {id,key,error:error.message};}});
assert.equal(rows.length,8);process.stdout.write(JSON.stringify(rows));
