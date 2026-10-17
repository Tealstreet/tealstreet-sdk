const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH),BigNumber=require('bignumber.js');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/paradex/paradex.sdk.ts');
const ast=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const owners=ast.statements.filter(n=>ts.isClassDeclaration(n)&&n.name?.text==='ParadexRequestSigner');assert.equal(owners.length,1);
const methods=owners[0].members.filter(n=>n.name?.getText(ast)==='toChainQuantum');assert.equal(methods.length,1);
const code=ts.transpileModule(`class Signer { ${methods[0].getText(ast)} }; Signer`,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;
const Signer=vm.runInNewContext(code,{BigNumber,Error});
const rows=[];
for(const field of ['size','price'])for(const value of ['Infinity','-Infinity','NaN','1.234567891']){
 const id=field+':'+value;try{rows.push({id,value:new Signer().toChainQuantum(value,field)});}catch(error){rows.push({id,name:error.name,message:error.message});}
}
for(const field of ['size','price']){const value='1e400',id=field+':'+value;try{rows.push({id,value:new Signer().toChainQuantum(value,field)});}catch(error){rows.push({id,name:error.name,message:error.message});}}
assert.equal(rows.length,10);process.stdout.write(JSON.stringify(rows));
