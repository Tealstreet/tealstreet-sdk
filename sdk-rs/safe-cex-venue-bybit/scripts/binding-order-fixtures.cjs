// Run maintained order-write members; only their transport and store dependencies are scripted.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../..');
const compilerOptions = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS };
const context = vm.createContext({
  chunk: (items, size) => Array.from({ length: Math.ceil(items.length / size) }, (_, i) => items.slice(i * size, (i + 1) * size)),
  forEachSeries: async (items, fn) => { for (let i = 0; i < items.length; i++) await fn(items[i], i); },
  sleep: async () => {},
});
function evaluate(source) {
  vm.runInContext(ts.transpileModule(source, { compilerOptions }).outputText, context);
}
function source(file) {
  return ts.createSourceFile(file, fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8'), ts.ScriptTarget.Latest, true);
}
const types = source('types.ts');
for (const name of ['OrderType', 'OrderSide']) {
  const node = types.statements.find(node => ts.isEnumDeclaration(node) && node.name.text === name);
  assert(node, `missing enum ${name}`);
  evaluate(node.getText(types).replace(/^export\s+/, ''));
}
const adapter = source('exchanges/bybit/bybit.exchange.ts');
const wire = source('exchanges/bybit/bybit.types.ts');
for (const [ast, names] of [[wire, ['ENDPOINTS']], [adapter, ['BYBIT_POSITION_TPSL_MODES', 'isBybitPositionTpslMode', 'isBybitOrderAlreadyGoneCancelMessage']]]) {
  const declarations = ast.statements.filter(ts.isVariableStatement).flatMap(node => [...node.declarationList.declarations]);
  for (const name of names) {
    const node = declarations.find(node => node.name.getText(ast) === name);
    assert(node, `missing declaration ${name}`);
    evaluate(`const ${node.getText(ast)};`);
  }
}
const klass = adapter.statements.find(node => ts.isClassDeclaration(node) && node.name.text === 'BybitExchange');
const names = ['updateOrder', 'cancelOrders', 'cancelOrdersHelper'];
const members = names.map(name => {
  const member = klass.members.find(node => node.name?.getText(adapter) === name);
  assert(member, `missing member ${name}`);
  return member.getText(adapter);
});
evaluate(`globalThis.Subject = class { ${members.join('\n')} };`);
(async () => {
  const results = [];
  for (const input of JSON.parse(fs.readFileSync(0, 'utf8'))) {
    const subject = new context.Subject();
    const calls = [];
    subject.store = { orders: input.parents, tickersBySymbol: {} };
    subject.getCategoryForSymbol = () => 'linear';
    subject.getOrderPositionIdx = async () => 0;
    subject.applyLocalOrderUpdateAfterRestWrite = () => {};
    subject.markOrderSeen = () => {};
    subject.retireCancelledOrders = () => {};
    subject.emitAndLogError = () => {};
    subject.xhr = subject.unlimitedXHR = { post: async (url, body) => {
      calls.push({ url, body });
      return { data: { retCode: 0, retMsg: input.message, result: { orderId: 'amended' }, ...(input.extra ?? {}) } };
    } };
    let result;
    try {
      result = { ok: input.kind === 'update' ? await subject.updateOrder(input.options) : (await subject.cancelOrders(input.orders), null) };
    } catch (error) {
      result = { error: error.message };
    }
    results.push({ calls, result });
  }
  process.stdout.write(JSON.stringify(results));
})().catch(error => { console.error(error); process.exitCode = 1; });
