const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
let text = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_LEVERAGE_BRACKETS_CONTROL === 'last-bracket') {
  const original = 'item.brackets[0].initialLeverage';
  if (!text.includes(original)) throw Error('leverage bracket control source missing');
  text = text.replace(original, 'item.brackets[item.brackets.length - 1].initialLeverage');
}
const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const method = cls.members.find(n => n.name?.getText(ast) === 'fetchLeverageBrackets');
if (!method) throw Error('Missing AsterDEXExchange#fetchLeverageBrackets');
const printer = ts.createPrinter();
const member = printer.printNode(ts.EmitHint.Unspecified, method, ast);
const compiled = ts.transpileModule(`const LogSeverity = { Info: 'info', Warning: 'warning' };
class AsterDEXExchange {
  constructor(apiVersion, response, error, trace) {
    this.apiVersion = apiVersion;
    this.primaryAPI = { get: async path => {
      trace.push(['request', path]);
      if (error) throw new Error(error);
      return { data: response };
    }};
    this.log = (message, severity) => trace.push(['log', message, severity]);
  }
  getFapiPath(endpoint) { return '/fapi/' + this.apiVersion + '/' + endpoint; }
  ${member}
}
module.exports = { Oracle: AsterDEXExchange };`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'module', compiled)(loaded.exports, loaded);
const { Oracle } = loaded.exports;
const cases = [
  { apiVersion: 'v1', response: [{ symbol: 'BTCUSDT', brackets: [{ initialLeverage: 125 }, { initialLeverage: 1 }] }, { symbol: 'ETHUSDT', brackets: [{ initialLeverage: 100 }] }] },
  { apiVersion: 'v3', response: { symbol: 'SOLUSDT', brackets: [{ initialLeverage: 50 }] } },
  { apiVersion: 'v1', response: [{ symbol: 'EMPTY', brackets: [] }, { symbol: 'MISSING' }] },
  { apiVersion: 'v3', response: [{ symbol: 'BTCUSDT', brackets: [{ initialLeverage: 125 }] }, { symbol: 'ETHUSDT', brackets: [{ initialLeverage: 25 }] }, { symbol: 'BTCUSDT', brackets: [{ initialLeverage: 75 }] }] },
  { apiVersion: 'v1', response: [null] },
  { apiVersion: 'v3', response: [{ symbol: 'BAD', brackets: [null] }] },
  { apiVersion: 'v1', error: 'offline' },
  // Equal JSON contents are distinct object identities at the source Map boundary.
  { apiVersion: 'v3', response: [
    { symbol: { market: 'BTCUSDT' }, brackets: [{ initialLeverage: 125 }] },
    { symbol: { market: 'BTCUSDT' }, brackets: [{ initialLeverage: 75 }] },
  ] },
  { apiVersion: 'v3', response: [
    { symbol: ['BTCUSDT'], brackets: [{ initialLeverage: 125 }] },
    { symbol: ['BTCUSDT'], brackets: [{ initialLeverage: 75 }] },
  ] },
  { apiVersion: 'v3', response: [
    { brackets: [{ initialLeverage: 125 }] },
    { symbol: null, brackets: [{ initialLeverage: 100 }] },
    { brackets: [{ initialLeverage: 75 }] },
    { symbol: null, brackets: [{ initialLeverage: 50 }] },
  ] },
];
(async () => {
  const output = [];
  for (const input of cases) {
    const trace = [];
    const instance = new Oracle(input.apiVersion, input.response, input.error, trace);
    const map = await instance.fetchLeverageBrackets();
    output.push({
      input,
      apiVersion: input.apiVersion,
      result: [...map.entries()],
      trace,
    });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { process.stderr.write(error.stack || String(error)); process.exitCode = 1; });
