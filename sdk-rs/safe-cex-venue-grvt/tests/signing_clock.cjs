const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const {bits}=require('../../safe-cex-num/tests/support/number-bits.cjs');
const compiler=process.env.SAFE_CEX_TYPESCRIPT_PATH;
assert(compiler&&path.isAbsolute(compiler),'absolute isolated compiler required');
const ts=require(compiler);
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/grvt/grvt.signing.ts');
const tree=ts.createSourceFile(file,fs.readFileSync(file,'utf8'),ts.ScriptTarget.Latest,true);
const names=['generateNonce','generateExpiration'];
const functions=names.map(name=>{const fn=tree.statements.find(s=>ts.isFunctionDeclaration(s)&&s.name.text===name);assert(fn,'actual '+name);return fn.getText(tree);});
let source=functions.join('\n');
if(process.env.GRVT_CLOCK_CONTROL==='truncated') {
 assert(source.includes('(expirationMs * 1_000_000).toString()'));
 source=source.replace('(expirationMs * 1_000_000).toString()','Math.trunc(expirationMs * 1_000_000).toString()');
}
let now=0,calls=0;
const context={exports:{},Date:{now:()=>{calls++;return now;}}};
vm.runInNewContext(ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,context);
const values=[0,-0,1,-1,0.1,-0.1,1e-12,-1e-12,1e-300,-1e-300,1e300,-1e300,1.25,-1.25,8640000000000000,1700000000123,Number.MAX_VALUE,-Number.MAX_VALUE,Infinity,-Infinity,NaN];
const rows=[];
for(now of values){
 calls=0;const nonce=context.exports.generateNonce();const nonceCalls=calls;
 for(const hours of [undefined,...values]){
  calls=0;const expiration=context.exports.generateExpiration(hours);
  rows.push({now:bits(now),hours:hours===undefined?null:bits(hours),nonce:bits(nonce),nonceCalls,expiration,calls});
 }
}
process.stdout.write(JSON.stringify(rows));
