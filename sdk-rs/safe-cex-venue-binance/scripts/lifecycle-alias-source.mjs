import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = new URL('../../../', import.meta.url);
const source = file => ts.createSourceFile(file, fs.readFileSync(new URL(file, root), 'utf8'),
  ts.ScriptTarget.Latest, true);
const compile = code => ts.transpileModule(code, { compilerOptions: {
  module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022,
} }).outputText;
const context = vm.createContext({});
function load(code) {
  context.exports = {};
  vm.runInContext(compile(code), context);
  const exported = context.exports;
  Object.assign(context, exported);
  return exported;
}
for (const file of ['packages/safe-cex/src/exchanges/income-paging.ts',
  'packages/safe-cex/src/utils/incomeCategoryFromType.ts']) {
  load(fs.readFileSync(new URL(file, root), 'utf8'));
}
function constant(tree, name) {
  const declaration = tree.statements.find(node => ts.isVariableStatement(node)
    && node.declarationList.declarations.some(item => item.name.getText(tree) === name));
  assert.ok(declaration, name);
  return `export ${declaration.getText(tree).replace(/^export\s+/, '')}`;
}
const exchange = source('packages/safe-cex/src/exchanges/binance/binance.exchange.ts');
const types = source('packages/safe-cex/src/exchanges/binance/binance.types.ts');
load(constant(types, 'ENDPOINTS'));
for (const name of ['INCOME_HISTORY_MAX_WINDOW_MS', 'INCOME_HISTORY_PAGE_LIMIT', 'INCOME_HISTORY_CHUNK_DELAY_MS']) {
  load(constant(exchange, name));
}
const subject = exchange.statements.find(node => ts.isClassDeclaration(node)
  && node.name?.text === 'BinanceExchange');
const methods = ['_fetchIncomeHistoryPage', 'fetchIncomeHistoryRange'].map(name => {
  const method = subject.members.find(node => node.name?.getText(exchange) === name);
  assert.ok(method, name);
  return method.getText(exchange);
});
const { Subject } = load(`exports.Subject = class { ${methods.join('\n')} };`);
const profiles = [];
for (const income of ['-0', 'NaN', 'Infinity', '-Infinity']) {
  const native = new Subject();
  const requests = [];
  native.isPublicDataOnly = false;
  native.xhr = { get: async (path, config) => {
    requests.push({ path, ...config.params });
    return { data: [{ symbol: 'BTCUSDT', incomeType: 'FUNDING_FEE', income,
      asset: 'USDT', time: 101, tranId: 31, tradeId: '41' }] };
  } };
  native.log = () => { throw new Error('complete source range must not warn'); };
  const rows = await native.fetchIncomeHistoryRange(100, 102, 'FUNDING_FEE');
  assert.equal(rows.length, 1);
  assert.equal(rows[0].tranId, 31);
  assert.equal(requests.length, 3);
  const bytes = new DataView(new ArrayBuffer(8));
  bytes.setFloat64(0, rows[0].income, false);
  profiles.push({ income, incomeBits: Number.isNaN(rows[0].income) ? 'NaN'
    : bytes.getBigUint64(0, false).toString(16).padStart(16, '0'),
    transaction: String(rows[0].tranId), requests });
}
assert.equal(profiles[0].incomeBits, '8000000000000000');
process.stdout.write(JSON.stringify(profiles));
