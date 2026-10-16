const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts');
let text = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_PRIVATE_OPEN_CONTROL === 'wrong-retry-reset') {
  const original = 'this.connectRetryCount = 0;';
  if (!text.includes(original)) throw Error('private onOpen control source missing');
  text = text.replace(original, 'this.connectRetryCount = 1;');
}
const configFile = path.join(root, 'packages/safe-cex/src/config/config.ts');
const configText = fs.readFileSync(configFile, 'utf8');
const configAst = ts.createSourceFile(configFile, configText, ts.ScriptTarget.Latest, true);
const configDecl = configAst.statements
  .filter(ts.isVariableStatement)
  .flatMap(statement => [...statement.declarationList.declarations])
  .find(declaration => declaration.name.getText(configAst) === 'ENABLE_RECONNECTING_WEBSOCKET');
if (!configDecl || !configDecl.initializer || ![ts.SyntaxKind.TrueKeyword, ts.SyntaxKind.FalseKeyword].includes(configDecl.initializer.kind)) {
  throw Error('ENABLE_RECONNECTING_WEBSOCKET must be a literal boolean');
}
const enabled = configDecl.initializer.kind === ts.SyntaxKind.TrueKeyword;
const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXPrivateWebsocket');
const onOpen = cls.members.find(n => n.name?.getText(ast) === 'onOpen');
if (!onOpen) throw Error('Missing AsterDEXPrivateWebsocket.onOpen');
const printer = ts.createPrinter();
const member = printer.printNode(ts.EmitHint.Unspecified, onOpen, ast);
const compiled = ts.transpileModule(`const ENABLE_RECONNECTING_WEBSOCKET = ${enabled};
const LogSeverity = { Info: 'info' };
class AsterDEXPrivateWebsocket {
  isDisposed = false; connectRetryCount = 7; parent;
  ping = () => {};
  ${member}
}
module.exports = { Oracle: AsterDEXPrivateWebsocket };`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} };
new Function('exports', 'module', compiled)(loaded.exports, loaded);
const { Oracle } = loaded.exports;
const cases = [];
for (const disposed of [false, true]) {
  const trace = [];
  const instance = new Oracle();
  instance.isDisposed = disposed;
  Object.defineProperty(instance, 'connectRetryCount', {
    get() { return this._retryCount; },
    set(value) { this._retryCount = value; trace.push(['retry', value]); },
  });
  instance._retryCount = 7;
  instance.parent = {
    log: (message, severity) => trace.push(['log', message, severity]),
    fetchAndSetPrivateSnapshot: retries => trace.push(['snapshot', retries]),
  };
  instance.ping = () => trace.push(['ping']);
  instance.onOpen();
  cases.push({ disposed, enabled, trace });
}
process.stdout.write(JSON.stringify(cases));
