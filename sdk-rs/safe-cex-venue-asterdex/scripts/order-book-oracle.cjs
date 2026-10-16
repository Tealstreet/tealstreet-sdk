const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
let text = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_ORDER_BOOK_CONTROL === 'ascending-bids') {
  const original = '.sort((a, b) => b.price - a.price)';
  if (!text.includes(original)) throw Error('order book source control anchor missing');
  text = text.replace(original, '.sort((a, b) => a.price - b.price)');
}
const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const printer = ts.createPrinter();
const memberNode = cls.members.find(n => n.name?.getText(ast) === 'fetchOrderBook');
if (!memberNode) throw Error('Missing AsterDEXExchange.fetchOrderBook');
const member = printer.printNode(ts.EmitHint.Unspecified, memberNode, ast);
const compiled = ts.transpileModule(`const LogSeverity = { Error: 'error' };
class Oracle {
  constructor(apiVersion, depth) {
    this.apiVersion = apiVersion;
    this.depth = depth;
    this.logs = [];
    this.publicAPI = { get: async (path, options) => {
      this.request = [path, options];
      return { data: this.depth };
    }};
  }
  getFapiPath(endpoint) { return '/fapi/' + this.apiVersion + '/' + endpoint; }
  log(...args) { this.logs.push(args); }
  ${member}
}
module.exports = { Oracle };`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'module', compiled)(loaded.exports, loaded);
const { Oracle } = loaded.exports;
const cases = [
  { apiVersion: 'v1', symbol: 'BTCUSDT', limit: 20, depth: { bids: [['10', '2'], ['12', '1'], ['11', '3']], asks: [['15', '2'], ['13', '1'], ['14', '3']] } },
  { apiVersion: 'v3', symbol: 'ETHUSDT', limit: 5, depth: { bids: [['1.5tail', '2'], ['1.5', '3']], asks: [['-0', '2'], ['0', '1']] } },
  { apiVersion: 'v1', symbol: 'EMPTY', limit: 100, depth: { bids: [], asks: [] } },
  { apiVersion: 'v3', symbol: 'INF', limit: 1000, depth: { bids: [['Infinity', '1'], ['10', '1']], asks: [['-Infinity', '1'], ['10', '1']] } },
];
(async () => {
  const output = [];
  for (const input of cases) {
    const oracle = new Oracle(input.apiVersion, input.depth);
    const result = await oracle.fetchOrderBook(input.symbol, input.limit);
    output.push({
      input,
      request: oracle.request,
      result,
      negativeZeroPrices: {
        bids: result.bids.map(level => Object.is(level.price, -0)),
        asks: result.asks.map(level => Object.is(level.price, -0)),
      },
    });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { process.stderr.write(error.stack || String(error)); process.exitCode = 1; });
