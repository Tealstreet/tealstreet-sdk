const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/backpack');
function read(file) {
  const filename = path.join(root, file);
  return ts.createSourceFile(filename, fs.readFileSync(filename, 'utf8'), ts.ScriptTarget.Latest, true);
}
const source = read('backpack.ws-public.ts');
const classes = source.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === 'BackpackPublicWebSocket');
assert.equal(classes.length, 1, 'exact maintained Backpack public class');
const fields = classes[0].members.filter(n => ts.isPropertyDeclaration(n) && n.name?.getText(source) === 'connectAndSubscribe');
assert.equal(fields.length, 1, 'exact maintained connectAndSubscribe member');
assert(ts.isArrowFunction(fields[0].initializer));
const types = read('backpack.types.ts');
const urls = types.statements.flatMap(n => ts.isVariableStatement(n) ? [...n.declarationList.declarations] : []).filter(n => n.name.getText(types) === 'WSS_URLS');
assert.equal(urls.length, 1, 'exact maintained URL declaration');
const emitted = ts.transpileModule(`const WSS_URLS=${urls[0].initializer.getText(types)}; (function(){return ${fields[0].initializer.getText(source)};})`, {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText;
const factory = require('node:vm').runInNewContext(emitted);
for (const testnet of [false, true]) {
  const listeners = [];
  const socket = {addEventListener: (name, callback) => listeners.push([name, callback])};
  const receiver = {
    parent: {options: {testnet}}, isDisposed: false,
    onOpen() {}, onMessage() {}, onClose() {},
    createWebsocket(url) { assert.equal(url, 'wss://ws.backpack.exchange'); return socket; },
  };
  factory.call(receiver)();
  assert.equal(receiver.ws, socket, 'maintained call synchronously installs original wrapper');
  assert.deepEqual(listeners.map(([name]) => name), ['open', 'message', 'close']);
  assert.equal(listeners[0][1], receiver.onOpen);
  assert.equal(listeners[1][1], receiver.onMessage);
  assert.equal(listeners[2][1], receiver.onClose);
}
process.stdout.write('synchronous-wrapper-installed\n');
