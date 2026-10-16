const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const exchangeFile = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
const typesFile = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts');
let exchangeText = fs.readFileSync(exchangeFile, 'utf8');
if (process.env.ASTERDEX_HISTORICAL_FILLS_CONTROL === 'wrong-side') {
  const original = "trade.side === 'BUY'";
  if (!exchangeText.includes(original)) throw Error('fetchHistoricalFills source control anchor missing');
  exchangeText = exchangeText.replace(original, "trade.side !== 'BUY'");
}
const exchangeAst = ts.createSourceFile(exchangeFile, exchangeText, ts.ScriptTarget.Latest, true);
const exchangeClass = exchangeAst.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const fetchNode = exchangeClass?.members.find(n => n.name?.getText(exchangeAst) === 'fetchHistoricalFills');
if (!fetchNode) throw Error('Missing AsterDEXExchange.fetchHistoricalFills');
const printer = ts.createPrinter();
const fetchMethod = printer.printNode(ts.EmitHint.Unspecified, fetchNode, exchangeAst);

const typesAst = ts.createSourceFile(typesFile, fs.readFileSync(typesFile, 'utf8'), ts.ScriptTarget.Latest, true);
const requiredVariables = ['ASTERDEX_DEFAULT_POSITION_SIDE', 'getAsterDEXRealizedPnlKey', 'normalizeAsterDEXRealizedPnlDeltas'];
const declarations = new Map();
for (const statement of typesAst.statements) {
  if (!ts.isVariableStatement(statement)) continue;
  for (const declaration of statement.declarationList.declarations) {
    if (ts.isIdentifier(declaration.name) && requiredVariables.includes(declaration.name.text)) {
      declarations.set(declaration.name.text, printer.printNode(ts.EmitHint.Unspecified, statement, typesAst));
    }
  }
}
for (const name of requiredVariables) if (!declarations.has(name)) throw Error(`Missing actual TypeScript helper ${name}`);
const compiled = ts.transpileModule(`
const LogSeverity = { Error: 'error' };
const OrderSide = { Buy: 'buy', Sell: 'sell' };
const PositionSide = { Long: 'long', Short: 'short' };
${requiredVariables.map(name => declarations.get(name)).join('\n')}
class Oracle {
  constructor(apiVersion, response, publicDataOnly = false, reject = false) {
    this.apiVersion = apiVersion;
    this.response = response;
    this.isPublicDataOnly = publicDataOnly;
    this.reject = reject;
    this.logs = [];
    this.primaryAPI = { get: async (requestPath, options) => {
      this.request = [requestPath, options];
      if (this.reject) throw new Error('oracle transport rejection');
      return { data: this.response };
    }};
  }
  getFapiPath(endpoint) { return '/fapi/' + this.apiVersion + '/' + endpoint; }
  log(...args) { this.logs.push(args); }
  ${fetchMethod}
}
module.exports = { Oracle };
`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'module', compiled)(loaded.exports, loaded);
const { Oracle } = loaded.exports;

const trades = [
  { symbol: 'BTCUSDT', id: 2, orderId: 9, price: '10.5tail', qty: '2', quoteQty: '21', realizedPnl: '5', side: 'BUY', positionSide: 'LONG', buyer: true, maker: false, commission: '0.1', commissionAsset: 'USDT', time: 3000 },
  { symbol: 'BTCUSDT', id: 1, orderId: 9, price: '12', qty: '1', realizedPnl: '2', side: 'SELL', positionSide: 'LONG', buyer: true, maker: false, commission: '', time: 1000 },
  { symbol: 'BTCUSDT', id: 3, orderId: 10, price: 'NaN', qty: '1', realizedPnl: '4', side: 'BUY', positionSide: 'SHORT', time: 4000 },
  { symbol: 'BTCUSDT', id: 4, orderId: 11, price: '1', qty: '0', realizedPnl: '6', side: 'BUY', positionSide: 'SHORT', time: 5000 },
  { symbol: 'ETHUSDT', id: 5, orderId: 12, price: '-1.25tail', qty: '3.5x', realizedPnl: 'Infinity', side: 'BUY', positionSide: 'BOTH', maker: true, commission: 'Infinity', commissionAsset: 'ETH', time: 6000 },
  { symbol: 'NEGUSDT', id: -0, orderId: -0, price: '1', qty: '1', side: 'SELL', positionSide: 'other', time: -0 },
  { symbol: 'INFUSDT', id: Infinity, orderId: Infinity, price: '2', qty: '1', side: 'BUY', positionSide: 'LONG', time: Infinity },
];
const cases = [
  { name: 'mapped and filtered trades', apiVersion: 'v1', opts: { symbol: 'BTCUSDT', orderId: 9, from: 1000, to: 9000, limit: 1500 }, response: trades },
  { name: 'falsy optional params and non-array response', apiVersion: 'v3', opts: { symbol: 'ETHUSDT', orderId: 0, from: 0, to: 0, limit: 0 }, response: null },
  { name: 'missing symbol', apiVersion: 'v1', opts: { symbol: '', limit: 10 }, response: trades },
  { name: 'public data only', apiVersion: 'v3', opts: { symbol: 'BTCUSDT' }, response: trades, publicDataOnly: true },
  { name: 'request failure', apiVersion: 'v1', opts: { symbol: 'BTCUSDT' }, response: trades, reject: true },
];
function bits(number) {
  if (Number.isNaN(number)) return '7ff8000000000000';
  const buffer = new ArrayBuffer(8);
  new DataView(buffer).setFloat64(0, number, false);
  return new DataView(buffer).getBigUint64(0, false).toString(16).padStart(16, '0');
}
(async () => {
  const output = [];
  for (const input of cases) {
    const oracle = new Oracle(input.apiVersion, input.response, input.publicDataOnly, input.reject);
    const result = await oracle.fetchHistoricalFills(input.opts);
    output.push({
      captureArchitecture: `${process.platform}-${process.arch}`,
      captureNodeMajor: Number(process.versions.node.split('.')[0]),
      input,
      request: oracle.request,
      logs: oracle.logs,
      result,
      compareMapper: !input.publicDataOnly && !input.reject && Boolean(input.opts.symbol),
      rowNumberBits: Array.isArray(input.response) ? input.response.map(trade => ({
        id: bits(trade.id), orderId: bits(trade.orderId), time: bits(trade.time),
      })) : [],
      bits: result.map(row => ({
        timestamp: bits(row.timestamp), price: bits(row.price), qty: bits(row.qty),
        commission: row.commission === undefined ? null : bits(row.commission),
        realizedPnl: row.realizedPnl === undefined ? null : bits(row.realizedPnl),
      })),
    });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { process.stderr.write(error.stack || String(error)); process.exitCode = 1; });
