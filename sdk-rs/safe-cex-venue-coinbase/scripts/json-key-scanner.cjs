const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH??'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/coinbase/coinbase.starbase-public-frames.ts');
const output={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(file,'utf8')+'\nexport { JsonDuplicateKeyScanner };',{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports:output,require:()=>({})});
const cases=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../tests/fixtures/json-key-scanner.json'),'utf8'));
process.stdout.write(JSON.stringify(cases.map(({label,units})=>{
 const scanner=new output.JsonDuplicateKeyScanner(String.fromCharCode(...units));
 return {label,first:scanner.scan(),second:scanner.scan()};
})));
