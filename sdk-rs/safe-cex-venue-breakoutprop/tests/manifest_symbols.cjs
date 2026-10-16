// Scoped source-identity census using the maintained extractor, not a regex copy.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH,'SAFE_CEX_TYPESCRIPT_PATH required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root=path.resolve(__dirname,'../../..');
const filename=path.join(root,'sdk-rs/parity/extract-ts/src/extract.ts');
const mod={exports:{}};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(filename,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText,{exports:mod.exports,module:mod,require:n=>n==='typescript'?ts:require(n)});
const dir=path.join(root,'packages/safe-cex/src/exchanges/breakoutprop');
const files=fs.readdirSync(dir).filter(f=>f.endsWith('.ts')).map(f=>path.join(dir,f));
// Identity extraction only: no claim of package semantic typechecking.
const program=ts.createProgram(files,{target:ts.ScriptTarget.ES2022,noResolve:true});
assert.equal(program.getSyntacticDiagnostics().length,0,'venue syntax');
const checker=program.getTypeChecker();
console.log(JSON.stringify(files.map(file=>({file:path.relative(root,file),symbols:mod.exports.extractFile(program.getSourceFile(file),checker,path.join(root,'packages/safe-cex'))}))));
