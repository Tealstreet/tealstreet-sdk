const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const node = cls.members.find(n => n.name?.getText(ast) === 'fetchSymbolMaxLeverage');
if (!node) throw Error('Missing fetchSymbolMaxLeverage');
const printer = ts.createPrinter();
let method = printer.printNode(ts.EmitHint.Unspecified, node, ast);
if (process.env.ASTERDEX_LEVERAGE_CONTROL === 'nullish') {
  const original = 'return market.limits?.leverage?.max || 10;';
  if (!method.includes(original)) throw Error('fetchSymbolMaxLeverage source control anchor missing');
  method = method.replace(original, 'return market.limits?.leverage?.max ?? 10;');
}
const compiled = ts.transpileModule(`class Oracle { ${method} }
export { Oracle };`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'LogSeverity', compiled)(loaded.exports, { Error: 'Error' });
const { Oracle } = loaded.exports;
const encoded = value => Number.isNaN(value) ? { number: 'NaN' } : value === Infinity ? { number: 'Infinity' } : value === -Infinity ? { number: '-Infinity' } : Object.is(value, -0) ? { number: '-0' } : value;
const candidates = [0, -0, NaN, Infinity, -Infinity, -3.5, 2.75, undefined, null];
const cases = candidates.map((max, index) => ({
  symbol: 'BTC',
  markets: [{ symbol: 'BTC', limits: { leverage: max === undefined ? {} : { max } } }, { symbol: 'BTC', limits: { leverage: { max: 99 } } }],
  max: max === undefined ? { undefined: true } : max === null ? null : encoded(max),
  index,
}));
cases.push({ symbol: 'MISSING', markets: [{ symbol: 'BTC', limits: { leverage: { max: 5 } } }] });
const output = cases.map(c => {
  const oracle = new Oracle();
  oracle.fetchMarkets = async () => c.markets;
  const logs = [];
  oracle.log = (...args) => logs.push(args);
  const marketRows = c.markets.map(m => {
    const max = m.limits?.leverage?.max;
    return { symbol: m.symbol, max: max === undefined ? { undefined: true } : max === null ? null : encoded(max) };
  });
  try {
    const value = oracle.fetchSymbolMaxLeverage(c.symbol);
    if (!value || typeof value.then !== 'function') throw Error('fetchSymbolMaxLeverage did not return a Promise');
    return value.then(
      result => ({ symbol: c.symbol, marketRows, result: encoded(result), logs }),
      error => ({ symbol: c.symbol, marketRows, error: { name: error.name, message: error.message }, logs }),
    );
  } catch (error) {
    return Promise.resolve({ symbol: c.symbol, marketRows, error: { name: error.name, message: error.message }, logs });
  }
});
Promise.all(output).then(rows => process.stdout.write(JSON.stringify(rows))).catch(error => { console.error(error); process.exitCode = 1; });
