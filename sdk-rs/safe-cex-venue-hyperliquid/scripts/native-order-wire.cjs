// Execute maintained functions with native NaN/infinities before JSON capture.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.signing.ts');
const source = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const names = ['floatToWire', 'orderTypeToWire', 'orderRequestToOrderWire'];
const declarations = names.map(name => {
  const declaration = source.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  if (!declaration) throw new Error(`missing maintained function ${name}`);
  return declaration.getText(source);
}).join('\n');
const moduleExports = {};
vm.runInNewContext(ts.transpileModule(declarations, {compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText, {exports:moduleExports});
const cases = [NaN, Infinity, -Infinity, -0, 60000.125, 1e30];
const rows = cases.map(value => moduleExports.orderRequestToOrderWire({is_buy:true,limit_px:value,sz:value,reduce_only:false,order_type:{trigger:{isMarket:true,triggerPx:value,tpsl:'sl'}}}, 7));
process.stdout.write(JSON.stringify(rows));
