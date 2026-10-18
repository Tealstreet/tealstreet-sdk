// Execute the source listener and its actual order-book imports on binding frames.
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const root = path.resolve(__dirname, '../../../packages/safe-cex/src');
function source(file) {
  const name = path.join(root, file);
  return ts.createSourceFile(name, fs.readFileSync(name, 'utf8'), ts.ScriptTarget.Latest, true);
}
const socket = source('exchanges/coinbase/coinbase.ws-public.ts');
const owner = socket.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'CoinbasePublicWebsocket');
const field = owner.members.find(n => ts.isPropertyDeclaration(n) && n.name.getText(socket) === 'listenOrderBook');
assert(field?.initializer);
const helpers = ['utils/orderbook.ts', 'utils/updateOrderBook.ts'].map(file => {
  const ast = source(file);
  return ast.statements.filter(n => !ts.isImportDeclaration(n)).map(n => n.getText(ast)).join('\n');
});
const utils = source('exchanges/coinbase/coinbase.utils.ts');
for (const name of ['COINBASE_USDC_ALIAS_EXCEPTIONS', 'normalizeCoinbaseUsdEquivalentProductId']) {
  const declaration = utils.statements.filter(ts.isVariableStatement).flatMap(n => [...n.declarationList.declarations]).find(n => n.name.getText(utils) === name);
  assert(declaration?.initializer);
  helpers.push(`const ${name} = ${declaration.initializer.getText(utils)};`);
}
const exportsObject = {};
vm.runInNewContext(ts.transpileModule(`${helpers.join('\n')}\nexport class Listener {listenOrderBook = ${field.initializer.getText(socket)};}`, {
  compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
}).outputText, {exports: exportsObject, nanoid: () => 'source-inner', console});
const listener = new exportsObject.Listener();
Object.assign(listener, {isConnected: true, orderBookInnerState: {}, messageHandlers: {}, topics: {}, send() {}});
const frames = JSON.parse(fs.readFileSync(0, 'utf8'));
const captures = [];
listener.listenOrderBook('BTC-USDC', book => captures.push(JSON.parse(JSON.stringify(book))));
for (const frame of frames) for (const event of frame.events) listener.messageHandlers['level2.BTC-USD'][0](event);
assert.equal(captures.length, frames.length);
process.stdout.write(JSON.stringify(captures));
