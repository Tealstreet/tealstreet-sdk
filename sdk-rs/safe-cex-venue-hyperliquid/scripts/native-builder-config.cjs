const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.constants.ts');
const moduleExports = {};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(file,'utf8'), {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText, {exports:moduleExports});
if (process.argv.includes('--defaults')) {
  const inputs = [undefined,null,{}, {testnet:true}, {testnet:true,hyperliquidBuilder:null}];
  process.stdout.write(JSON.stringify(inputs.map(options => {
    const result = moduleExports.resolveHyperliquidBuilder(options);
    return {result,fresh:result!==moduleExports.resolveHyperliquidBuilder(options)};
  })));
  process.exit(0);
}
const address = '0x1111111111111111111111111111111111111111';
const inputs = [{address,feeTenthsBps:'10'},{feeTenthsBps:7},false,17,'raw'];
const rows = inputs.map(builder => {
  let reads = 0;
  const options = {hyperliquidBuilder:builder,get testnet(){reads++;throw Error('unused testnet');}};
  const result = moduleExports.resolveHyperliquidBuilder(options);
  return {same:result===builder,testnetReads:reads,result};
});
process.stdout.write(JSON.stringify(rows));
