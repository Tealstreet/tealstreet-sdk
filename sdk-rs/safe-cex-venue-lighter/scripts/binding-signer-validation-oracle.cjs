// Live TransactionClient validation precedence; no HTTP or cryptography.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/client/transaction-client.ts');
const source = fs.readFileSync(file, 'utf8');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
assert.equal(tree.parseDiagnostics.length, 0);
const compiled = ts.transpileModule(source, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}, reportDiagnostics: true});
assert.equal(compiled.diagnostics.length, 0);
const context = {exports: {}, Date: {now: () => 1700000000000}};
vm.runInNewContext(compiled.outputText, context, {filename: file});
const client = new context.exports.TransactionClient({chainId: 304, accountIndex: 42n, apiKeyIndex: 3, privateKey: '', crypto: {}, httpClient: {}});
const order = {orderId: 1n, marketId: 1, isBuy: true, price: 100n, amount: 1n};
const inputs = [
 ['leverage-before-mode', 'leverage', {marketId: 1, leverage: -1, marginMode: 7}],
 ['market-before-mode', 'leverage', {marketId: 255, leverage: 10, marginMode: 7}],
 ['mode', 'leverage', {marketId: 1, leverage: 10, marginMode: 7}],
 ['leverage-string', 'leverage', {marketId: 1, leverage: '10', marginMode: 0}],
 ['empty-before-grouping', 'grouped', {orders: [], groupingType: 7}],
 ['market-before-grouping', 'grouped', {orders: [{...order, marketId: 255}], groupingType: 7}],
 ['duplicate-before-grouping', 'grouped', {orders: [order, order], groupingType: 7}],
 ['same-market-before-grouping', 'grouped', {orders: [order, {...order, marketId: 2}], groupingType: 7}],
 ['grouping', 'grouped', {orders: [order], groupingType: 7}],
];
const cases = inputs.map(([label, kind, params]) => {
 try {
  client[kind === 'leverage' ? 'buildUpdateLeverageData' : 'validateCreateGroupedOrdersParams'](params, 7n);
  assert.fail(`${label}: expected source validation to throw`);
 } catch (error) {
  assert.equal(error.name, 'Error');
  return {label, kind, params, error: {name: error.name, message: error.message}};
 }
});
assert.equal(cases.length, 9);
process.stdout.write(JSON.stringify(cases, (_, value) => typeof value === 'bigint' ? {$bigint: value.toString()} : value));
