const fs = require('node:fs');
const path = require('node:path');
const { ts, root } = require('./source.cjs');

const file = path.join(root, 'packages/safe-cex/src/exchanges/asterdex/asterdex.ws-private.ts');
let text = fs.readFileSync(file, 'utf8');
if (process.env.ASTERDEX_PRIVATE_PING_CONTROL === 'wrong-message') {
  const original = "method: 'LIST_SUBSCRIPTIONS'";
  if (!text.includes(original)) throw Error('private ping control source missing');
  text = text.replace(original, "method: 'LIST_ORDERS'");
}
const ast = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'AsterDEXPrivateWebsocket');
const ping = cls.members.find(n => n.name?.getText(ast) === 'ping');
if (!ping) throw Error('Missing AsterDEXPrivateWebsocket.ping');
const printer = ts.createPrinter();
const member = printer.printNode(ts.EmitHint.Unspecified, ping, ast);
const compiled = ts.transpileModule(`class AsterDEXPrivateWebsocket {
  ws; isDisposed = false; pingAt = 0;
  ${member}
}
module.exports = { Oracle: AsterDEXPrivateWebsocket };`, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
const loaded = { exports: {} }, trace = [];
new Function('exports', 'module', 'WebSocket', 'performance', compiled)(loaded.exports, loaded, { OPEN: 1 }, { now: () => 42 });
const { Oracle } = loaded.exports;
const cases = [];
for (const [disposed, connected] of [[false, true], [false, false], [true, true]]) {
  const instance = new Oracle();
  instance.isDisposed = disposed;
  instance.ws = connected ? { readyState: 1, send: message => trace.push(message) } : { readyState: 0, send: message => trace.push(message) };
  const before = trace.length;
  instance.ping();
  cases.push({ disposed, connected, sent: trace.length > before, message: trace[before] ?? null, pingAt: instance.pingAt });
}
process.stdout.write(JSON.stringify(cases));
