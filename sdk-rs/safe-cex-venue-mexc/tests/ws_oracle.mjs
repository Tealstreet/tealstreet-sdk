import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
if (!process.env.SAFE_CEX_TYPESCRIPT_PATH) throw Error('SAFE_CEX_TYPESCRIPT_PATH must name the isolated compiler');
const require = createRequire(path.join(path.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH), '../mexc-ws.cjs'));
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const sourcePath = fileURLToPath(new URL('../../../packages/safe-cex/src/exchanges/mexc/mexc.ws-public.ts', import.meta.url));
let source = fs.readFileSync(sourcePath, 'utf8');
if (process.env.MEXC_WS_TS_CONTROL === 'subscription-param') {
  const before = 'JSON.stringify(subscription)';
  assert(source.includes(before), 'missing TypeScript subscription mutation anchor');
  source = source.replace(before, 'JSON.stringify({method: subscription.method})');
}
const ast = ts.createSourceFile(sourcePath, source, ts.ScriptTarget.Latest, true);
const statement = ast.statements.find(node => ts.isVariableStatement(node)
  && node.declarationList.declarations.some(declaration => declaration.name.getText(ast) === 'subscriptionKey'));
assert(statement, 'subscriptionKey declaration not found');
const js = ts.transpileModule(`${statement.getText(ast)}\nexports.subscriptionKey = subscriptionKey;`, {
  compilerOptions: {target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS},
}).outputText;
const module = {exports: {}};
vm.runInNewContext(`(function(exports){${js}\n})(module.exports)`, {module});
const cases = [
  {method:'sub.ticker', param:{symbol:'BTC_USDT'}},
  {method:'sub.depth.full', param:{symbol:'ETH_USDT',limit:20}},
  {method:'sub.kline', param:{symbol:'X_Y',interval:'Min60'}},
  {method:'method', param:null},
  {method:'unicode', param:{'é':'雪',nested:[true,0,'x']}},
];
const output = cases.map(input => ({input, output: module.exports.subscriptionKey(input)}));
process.stdout.write(JSON.stringify(output));
