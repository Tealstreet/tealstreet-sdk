// Actual maintained wrapper and installed axios-retry/is-retry-allowed policy.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/utils/retryReadsOnly.ts');
const moduleExports={};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText,{exports:moduleExports,require});
const inputs=JSON.parse(fs.readFileSync(0,'utf8'));
process.stdout.write(JSON.stringify(inputs.map(({code,method})=>moduleExports.networkErrorOnRead()({config:{method},code}))));
