const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.exchange.ts');
let sourceText = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_MARGIN_CONTROL === 'accept-empty-symbol') {
  const original = "if (!symbol)";
  if (!sourceText.includes(original)) throw Error('changeMarginType source control target disappeared');
  sourceText = sourceText.replace(original, 'if (false)');
}
const ast = ts.createSourceFile(file, sourceText, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXExchange');
const printer = ts.createPrinter();
const member = name => {
  const node = cls.members.find(n => n.name?.getText(ast) === name);
  if (!node) throw Error(`Missing ${name}`);
  return printer.printNode(ts.EmitHint.Unspecified, node, ast);
};
const source = `class Oracle {
  apiVersion;
  logs = [];
  getFapiPath(endpoint) { return '/fapi/' + this.apiVersion + '/' + endpoint.replace(/^\\/+/, ''); }
  log(...args) { this.logs.push(args); }
  ${member('changeMarginType')}
}
export { Oracle };`;
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'LogSeverity', compiled)(loaded.exports, { Error: 'error' });
const { Oracle } = loaded.exports;
const result = async fn => {
  try { await fn(); return { resolved: true }; }
  catch (error) { return { error: { name: error.name, message: error.message } }; }
};
(async () => {
  const cases = [];
  for (const apiVersion of ['v1', 'v3']) {
    for (const symbol of [undefined, '', 'BTCUSDT']) {
      for (const isolated of [false, true]) {
        const oracle = new Oracle();
        oracle.apiVersion = apiVersion;
        let requestPath;
        let requestBody;
        oracle.primaryAPI = { post: async (url, body) => { requestPath = url; requestBody = body; } };
        cases.push({ apiVersion, symbol, isolated,
          ...await result(() => oracle.changeMarginType(isolated, symbol)),
          requestPath, requestBody, logs: oracle.logs });
      }
    }
  }
  process.stdout.write(JSON.stringify(cases));
})().catch(error => { console.error(error); process.exitCode = 1; });
