const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
let text = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_GET_ACCOUNT_CONTROL === 'nullish-fallback') {
  const original = 'account.accountAlias || this.options.key ||';
  if (!text.includes(original)) throw Error('getAccount source control anchor missing');
  text = text.replace(original, 'account.accountAlias ?? this.options.key ??');
}
const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const printer = ts.createPrinter();
const methodNode = cls.members.find(n => n.name?.getText(ast) === 'getAccount');
if (!methodNode) throw Error('Missing AsterDEXExchange.getAccount');
const member = printer.printNode(ts.EmitHint.Unspecified, methodNode, ast);
const compiled = ts.transpileModule(`class Oracle {
  constructor(apiVersion, key, account, error) {
    this.apiVersion = apiVersion;
    this.options = { key };
    this.logs = [];
    this.primaryAPI = { get: async path => {
      this.path = path;
      if (error) throw new Error(error);
      return { data: account };
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
  { apiVersion: 'v1', key: 'key-v1', account: { accountAlias: 'account-1' } },
  { apiVersion: 'v3', key: 'key-v3', account: { accountAlias: '' } },
  { apiVersion: 'v1', key: '', account: { accountAlias: '' } },
  { apiVersion: 'v3', account: { accountAlias: '' } },
  { apiVersion: 'v3', key: 'fallback', account: { accountAlias: 'alias' } },
  { apiVersion: 'v1', key: 'fallback', account: {} },
];
(async () => {
  const output = [];
  for (const input of cases) {
    const oracle = new Oracle(input.apiVersion, input.key, input.account, input.error);
    const result = await oracle.getAccount();
    output.push({ input, result, path: oracle.path, logs: oracle.logs });
  }
  process.stdout.write(JSON.stringify(output));
})().catch(error => { process.stderr.write(error.stack || String(error)); process.exitCode = 1; });
