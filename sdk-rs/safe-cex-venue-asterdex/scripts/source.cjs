const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const tsPath = process.env.SAFE_CEX_TYPESCRIPT_PATH || path.join(root, 'sdk-rs/target/asterdex-oracle-deps/node_modules/typescript');
const ts = require(tsPath);
const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts');
let source = fs.readFileSync(file, 'utf8');
// Test-only, in-memory corruption: the child comparison must reject it by name.
if (process.env.ASTERDEX_ORACLE_CONTROL === 'endpoint') {
  const original = "https://fapi.asterdex.com";
  if (!source.includes(original)) throw Error('Endpoint control source disappeared');
  source = source.replace(original, 'https://asterdex-control.invalid');
}
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const compiled = ts.transpileModule(source + '\nexport { trimNonEmpty, getAsterDEXRealizedPnlKey };', {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
const loaded = {exports: {}};
new Function('exports', 'module', compiled)(loaded.exports, loaded);
module.exports = {ts, ast, source, values: loaded.exports, root};
