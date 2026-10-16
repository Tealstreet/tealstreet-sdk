const assert = require('node:assert/strict');
const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { root, ts, load } = require('./types-oracle.cjs');
const types = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const constants = load(path.join(root, 'packages/safe-cex/src/exchanges/weex/weex.types.ts'));
function sourceClass(file, name) {
  const tree = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
  const node = tree.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === name);
  assert.ok(node, name);
  const arrows = node.members.filter(n => ts.isPropertyDeclaration(n) && n.initializer && ts.isArrowFunction(n.initializer)).map(n => n.name.getText(tree));
  const prototype = node.members.filter(ts.isMethodDeclaration).map(n => n.name.getText(tree));
  const compiled = ts.transpileModule(node.getText(tree), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }, reportDiagnostics: true });
  assert.equal(compiled.diagnostics.filter(d => d.category === ts.DiagnosticCategory.Error).length, 0);
  return { compiled: compiled.outputText, arrows, prototype };
}
const names = ['WeexExchange', 'WeexPublicWebsocket', 'WeexPrivateWebsocket'];
const files = ['weex.exchange.ts', 'weex.ws-public.ts', 'weex.ws-private.ts'];
const declarations = files.map((file, i) => sourceClass(path.join(root, 'packages/safe-cex/src/exchanges/weex', file), names[i]));
assert.deepEqual(declarations.map(d => d.arrows.length), [45, 20, 14], 'classes::source_arrow_floor');
const client = path.join(root, 'packages/safe-cex/src/exchanges/clientOrderId.ts');
const clientTree = ts.createSourceFile(client, fs.readFileSync(client, 'utf8'), ts.ScriptTarget.Latest, true);
const rule = clientTree.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(clientTree) === 'patternClientOrderIdRule'));
const scope = { exports: {}, ...types, ...constants, Map, Set, performance: { now: () => 42 }, uuid: () => 'fixture', createAPI: () => ({ transport: true }) };
class ExchangeBase {
  constructor(name, options, store) { this.name = name; this.options = options; this.store = store; }
  createXhrWithSecondaryKey(options, factory) { assert.strictEqual(options, this.options); assert.strictEqual(factory, scope.createAPI); return factory(); }
  on(event, handler) { this.registered = handler; }
  off(event, handler) { assert.strictEqual(handler, this.registered); }
}
class SocketBase {
  constructor(parent) { this.parent = parent; this.store = parent.store; this.isDisposed = false; this.pingAt = 0; }
  sendWithLog(value) { this.sent ??= []; this.sent.push(JSON.parse(value)); }
  dispose() { this.isDisposed = true; }
}
scope.BaseExchange = ExchangeBase; scope.BaseWebSocket = SocketBase;
vm.runInNewContext(ts.transpileModule(rule.getText(clientTree), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText, scope);
for (const declaration of declarations) vm.runInNewContext(declaration.compiled, scope);
const store = { markets: [{ symbol: 'BTC' }, { symbol: 'ETH' }] };
const exchange = new scope.exports.WeexExchange('WEEX', {}, store);
const parent = { store, activeTickerSymbols: [], on(event, handler) { this.handler = handler; }, off(event, handler) { assert.strictEqual(handler, this.handler); } };
const pub = new scope.exports.WeexPublicWebsocket(parent), priv = new scope.exports.WeexPrivateWebsocket(parent);
const instances = [exchange, pub, priv];
const collectionFields = [['symbolSettings'], ['messageHandlers', 'topics', 'tickerTopics', 'subscriptionOperationTimestamps', 'orderbooks'], []];
const collections = instances.map((instance, i) => collectionFields[i].map(name => {
  const value = instance[name];
  const kind = value instanceof Map ? 'Map' : value instanceof Set ? 'Set' : Array.isArray(value) ? 'Array' : 'Object';
  return [name, kind, kind === 'Map' || kind === 'Set' ? value.size : kind === 'Array' ? value.length : Object.keys(value).length];
}));
const ownNames = instances.map(o => Object.keys(o).filter(k => k !== 'registered'));
assert.deepEqual(ownNames.map(a => a.length), [56, 32, 24], 'classes::source_own_floor');
const isAlgo = exchange.isAlgoOrder;
assert.equal(isAlgo({ id: 'a__b' }), true);
const retained = [pub.ping, priv.ping];
retained.forEach(ping => { ping.call({ subscriptionId: 99 }); ping(); });
assert.equal(pub.subscriptionId, 3); assert.equal(priv.subscriptionId, 3);
assert.equal(pub.pingAt, 42); assert.equal(priv.pingAt, 42);
process.stdout.write(JSON.stringify({
  classes: instances.map((instance, i) => ({ name: names[i], names: ownNames[i], arrows: declarations[i].arrows, prototype: declarations[i].prototype, collections: collections[i] })),
  algo: isAlgo({ id: 'a__b' }),
  sockets: [pub, priv].map(s => ({ id: s.subscriptionId, pingAt: s.pingAt, sent: s.sent })),
}));
