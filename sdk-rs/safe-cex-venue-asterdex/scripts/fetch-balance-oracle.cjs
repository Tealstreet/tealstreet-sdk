const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
let text = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_BALANCE_CONTROL === 'nullish-fallback') {
  const original = "account.totalWalletBalance || '0'";
  if (!text.includes(original)) throw Error('fetchBalance source control anchor missing');
  text = text.replace(original, "account.totalWalletBalance ?? '0'");
}
const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const printer = ts.createPrinter();
const memberNode = cls.members.find(n => n.name?.getText(ast) === 'fetchBalance');
if (!memberNode) throw Error('Missing AsterDEXExchange#fetchBalance');
const member = printer.printNode(ts.EmitHint.Unspecified, memberNode, ast);
const compiled = ts.transpileModule(`const LogSeverity = { Error: 'error' };
class Oracle {
  constructor(apiVersion, account) {
    this.apiVersion = apiVersion;
    this.account = account;
    this.logs = [];
    this.primaryAPI = { get: async path => {
      this.request = path;
      return { data: this.account };
    }};
  }
  getAccountEndpoint() { return this.apiVersion === 'v3' ? '/fapi/v3/account' : '/fapi/v4/account'; }
  log(...args) { this.logs.push(args); }
  ${member}
}
module.exports = { Oracle };`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'module', compiled)(loaded.exports, loaded);
const { Oracle } = loaded.exports;
const cases = [
  { apiVersion: 'v1', account: { totalWalletBalance: '123.4', availableBalance: '100', totalUnrealizedProfit: '0.25', totalInitialMargin: '10', totalOpenOrderInitialMargin: '2' } },
  { apiVersion: 'v3', account: { totalWalletBalance: '', availableBalance: '', totalUnrealizedProfit: '', totalInitialMargin: '', totalOpenOrderInitialMargin: '' } },
  { apiVersion: 'v1', account: { totalWalletBalance: '0', availableBalance: ' 4.5tail', totalUnrealizedProfit: '-0', totalInitialMargin: 'Infinity', totalOpenOrderInitialMargin: '2' } },
  { apiVersion: 'v3', account: { totalWalletBalance: 'NaN', availableBalance: '-Infinity', totalUnrealizedProfit: '1e400', totalInitialMargin: '1e308', totalOpenOrderInitialMargin: '1e308' } },
];
const bits = value => {
  if (Number.isNaN(value)) return '7ff8000000000000';
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, value, false);
  return view.getBigUint64(0, false).toString(16).padStart(16, '0');
};
(async () => {
  const output = [];
  for (const input of cases) {
    const oracle = new Oracle(input.apiVersion, input.account);
    const result = await oracle.fetchBalance();
    output.push({
      captureArchitecture: `${process.platform}-${process.arch}`,
      captureNodeMajor: Number(process.versions.node.split('.')[0]),
      input,
      request: oracle.request,
      result,
      bits: Object.fromEntries(Object.entries(result).map(([key, value]) => [key, bits(value)])),
    });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { process.stderr.write(error.stack || String(error)); process.exitCode = 1; });
