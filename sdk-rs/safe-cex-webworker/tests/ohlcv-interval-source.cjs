const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const intervals = ['1M','1s','5s','15s','30s','1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w','3h','5h','7h','8h','9h','10h','11h','24h','2d','3d'];
const typePath = path.join(root, 'packages/safe-cex/src/types.ts');
const ast = ts.createSourceFile(typePath, fs.readFileSync(typePath, 'utf8'), ts.ScriptTarget.Latest, true);
const aliases = new Map(ast.statements.filter(ts.isTypeAliasDeclaration).map(n => [n.name.text, n.type]));
function literals(node) {
  if (ts.isUnionTypeNode(node)) return node.types.flatMap(literals);
  if (ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal)) return [node.literal.text];
  assert(ts.isTypeReferenceNode(node) && aliases.has(node.typeName.getText(ast)), 'OHLCV source closed interval alias');
  return literals(aliases.get(node.typeName.getText(ast)));
}
assert.deepEqual(literals(aliases.get('AnyTimeframe')).sort(), [...intervals].sort(), 'OHLCV exact 27 source interval literals');
class BaseHandler {
  constructor(exchange, ctx) { this._exchange = exchange; this._ctx = ctx; }
  bufferUntilMarketsLoaded() { return false; }
}
const modules = new Map();
function load(name) {
  if (name === './BaseHandler') return { BaseHandler };
  if (name === 'lodash/throttle') return require(name);
  assert(['CallMethodHandler', 'OHLCVSubscriptionsHandler', './ThrottleManager'].includes(name), `unexpected source import ${name}`);
  if (modules.has(name)) return modules.get(name);
  const file = path.join(root, 'packages/safe-cex/src/webworker/handlers', name.replace('./','') + '.ts');
  const source = fs.readFileSync(file, 'utf8');
  const js = ts.transpileModule(source, { compilerOptions: { target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.CommonJS, esModuleInterop:true } }).outputText;
  const module = { exports:{} };
  vm.runInNewContext(js, { module, exports:module.exports, require:load, console, setTimeout, clearTimeout });
  modules.set(name, module.exports);
  return module.exports;
}
(async () => {
  const { CallMethodHandler } = load('CallMethodHandler');
  const { OHLCVSubscriptionsHandler } = load('OHLCVSubscriptionsHandler');
  const replies = [], posts = [], reads = [], subscriptions = [];
  const exchange = {
    name:'bybit', options:{}, store:{ markets:[{symbol:'BTC'}] },
    async fetchOHLCV(options) { reads.push(options); return []; },
    listenOHLCV(options, callback) {
      subscriptions.push(options);
      callback({timestamp:123,open:1,high:3,low:0.5,close:2,volume:4});
      return () => {};
    },
  };
  const reader = new CallMethodHandler(exchange, { postMessage:reply => replies.push(reply) });
  const listener = new OHLCVSubscriptionsHandler(exchange, { postMessage:post => posts.push(post) });
  for (const [i, interval] of intervals.entries()) {
    const options = {symbol:'BTC',interval,from:123,to:456,limit:7};
    await reader.handleCallMethod(`interval-${i}`, 'fetchOHLCV', [options]);
    assert.equal(replies.at(-1).data.success, true, `source fetch ${interval}`);
    assert.strictEqual(reads.at(-1), options);
    const data = {symbol:'BTC',interval,subId:`s${i}`};
    listener.handle({op:'subscribeOHLCV',data});
    assert.strictEqual(subscriptions.at(-1), data);
    assert.equal(posts.at(-1).data.interval, interval);
    assert.equal(posts.at(-1).data.subId, `s${i}`);
    assert.equal(posts.at(-1).data.candle.volume, 4);
    listener.handle({op:'unsubscribeOHLCV',data:{subId:`s${i}`}});
  }
  assert.equal(reads.length,27); assert.equal(replies.length,27);
  assert.equal(subscriptions.length,27); assert.equal(posts.length,27);
  console.log('OHLCV exact 27 source literals; actual fetch/subscription forwarding and delivery PASS');
})().catch(error => { console.error(error); process.exitCode = 1; });
