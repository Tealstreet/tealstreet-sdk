const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
let text = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_OHLCV_CONTROL === 'wrong-open-field') {
  const original = 'open: parseFloat(candle[1])';
  if (!text.includes(original)) throw Error('fetchOHLCV source control anchor missing');
  text = text.replace(original, 'open: parseFloat(candle[2])');
}
const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
if (!cls) throw Error('Missing AsterDEXExchange');
const printer = ts.createPrinter();
const memberText = name => {
  const member = cls.members.find(n => n.name?.getText(ast) === name);
  if (!member) throw Error(`Missing AsterDEXExchange.${name}`);
  return printer.printNode(ts.EmitHint.Unspecified, member, ast);
};
const compiled = ts.transpileModule(`
const LogSeverity = { Error: 'error' };
class Oracle {
  constructor(apiVersion, rows) {
    this.apiVersion = apiVersion;
    this.rows = rows;
    this.logs = [];
    this.publicAPI = { get: async (requestPath, options) => {
      this.request = [requestPath, options];
      return { data: this.rows };
    }};
  }
  getFapiPath(endpoint) { return '/fapi/' + this.apiVersion + '/' + endpoint; }
  log(...args) { this.logs.push(args); }
  ${memberText('fetchOHLCV')}
  ${memberText('mapTimeframe')}
}
module.exports = { Oracle };
`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'module', compiled)(loaded.exports, loaded);
const { Oracle } = loaded.exports;
const rows = [
  [1700000000000, '1.25', '2.50', '0.75', '2.0', '10.5', 1700000059999, '25', 17, '4', '9', '0'],
  [-0, '-0', '12tail', '', 'Infinity', '-Infinity', 0, '0', 0, '0', '0', '0'],
  [9007199254740992, 'NaN', '0x10', '1e400', '-1.25tail', '\uFEFF3.75x', 0, '0', 0, '0', '0', '0'],
];
const cases = [
  { apiVersion: 'v1', interval: '1m', symbol: 'BTCUSDT', limit: 20, from: 1700000000000, rows },
  { apiVersion: 'v3', interval: '1h', symbol: 'ETHUSDT', limit: 0, from: 0, rows: [rows[1]] },
  { apiVersion: 'v1', interval: '1w', symbol: 'EMPTY', rows: [] },
  { apiVersion: 'v3', interval: '12h', symbol: 'LIMIT', limit: -2, from: -1, rows: [rows[2]] },
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
    const oracle = new Oracle(input.apiVersion, input.rows);
    const result = await oracle.fetchOHLCV({
      interval: input.interval,
      symbol: input.symbol,
      limit: input.limit,
      from: input.from,
    });
    output.push({
      captureArchitecture: `${process.platform}-${process.arch}`,
      captureNodeMajor: Number(process.versions.node.split('.')[0]),
      input,
      request: oracle.request,
      result,
      rowTimestampBits: input.rows.map(row => bits(row[0])),
      bits: result.map(candle => [candle.timestamp, candle.open, candle.high, candle.low, candle.close, candle.volume].map(bits)),
    });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { process.stderr.write(error.stack || String(error)); process.exitCode = 1; });
