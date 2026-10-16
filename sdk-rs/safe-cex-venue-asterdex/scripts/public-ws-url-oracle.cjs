const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const exchangeFile = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-public.ts');
const typesFile = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.types.ts');
let exchangeText = fs.readFileSync(exchangeFile, 'utf8');
if (process.env.ASTERDEX_PUBLIC_URL_CONTROL === 'wrong-query-key') {
  const original = '?requestId=${nanoid()}';
  if (!exchangeText.includes(original)) throw Error('public connection URL control target disappeared');
  exchangeText = exchangeText.replace(original, '?request=${nanoid()}');
}
const exchangeAst = ts.createSourceFile(exchangeFile, exchangeText, ts.ScriptTarget.Latest, true);
const cls = exchangeAst.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXPublicWebsocket');
const getter = cls.members.find(n => ts.isGetAccessorDeclaration(n) && n.name.getText(exchangeAst) === 'connectionURL');
if (!getter) throw Error('Missing public connectionURL getter');
const typesText = fs.readFileSync(typesFile, 'utf8');
const typesAst = ts.createSourceFile(typesFile, typesText, ts.ScriptTarget.Latest, true);
const endpointStatement = typesAst.statements.find(n => ts.isVariableStatement(n) &&
  n.declarationList.declarations.some(d => d.name.getText(typesAst) === 'ASTERDEX_ENDPOINTS'));
const endpointDecl = endpointStatement?.declarationList.declarations.find(d => d.name.getText(typesAst) === 'ASTERDEX_ENDPOINTS');
if (!endpointDecl) throw Error('Missing ASTERDEX_ENDPOINTS declaration');
const printer = ts.createPrinter();
const endpoints = printer.printNode(ts.EmitHint.Expression, endpointDecl.initializer, typesAst);
const source = `const ASTERDEX_ENDPOINTS = ${endpoints};
class Oracle {
  parent;
  constructor(testnet) { this.parent = { options: { testnet } }; }
  ${printer.printNode(ts.EmitHint.Unspecified, getter, exchangeAst)}
}
export { Oracle };`;
const compiled = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
let nextId = '';
new Function('exports', 'nanoid', compiled)(loaded.exports, () => nextId);
const { Oracle } = loaded.exports;
const cases = [];
for (const testnet of [false, true]) {
  for (const requestId of ['Aster-1', 'id_with_underscore']) {
    nextId = requestId;
    const oracle = new Oracle(testnet);
    cases.push({ testnet, requestId, url: oracle.connectionURL });
  }
}
process.stdout.write(JSON.stringify(cases));
