const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const root = path.resolve(__dirname, '../../..');
const compilerOptions = { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS };
const read = relative => fs.readFileSync(path.join(root, 'packages/safe-cex/src', relative), 'utf8');
function moduleFrom(relative, dependencies = {}) {
  const exports = {};
  vm.runInNewContext(ts.transpileModule(read(relative), { compilerOptions }).outputText,
    { exports, require(name) { assert(Object.hasOwn(dependencies, name), `unmapped import ${relative}: ${name}`); return dependencies[name]; }, structuredClone });
  return exports;
}
const { cleanFloat, createIncrementalFills } = moduleFrom('utils/incrementalFills.ts');
const { clone } = moduleFrom('utils/clone.ts');
const { DefaultStore } = moduleFrom('store/store.base.ts', {
  'lodash/uniqBy': { default: require('lodash/uniqBy') }, '../types': { OrderStatus: { Closed: 'closed', Canceled: 'canceled' } },
  '../utils/clone': { clone }, '../utils/virtualClock': { virtualClock: { getCurrentTimeMs: () => 1000 } },
});
function subject(side) {
  const filename = `exchanges/bybit/bybitinverse.ws-${side}.ts`;
  const ast = ts.createSourceFile(filename, read(filename), ts.ScriptTarget.Latest, true);
  assert.equal(ast.parseDiagnostics.length, 0);
  const className = side === 'private' ? 'BybitInversePrivateWebsocket' : 'BybitInversePublicWebsocket';
  const cls = ast.statements.find(n => ts.isClassDeclaration(n) && n.name?.text === className);
  const names = side === 'private' ? ['incrementalFills', 'onMessage', 'handleOrderTopic', 'handlePositionTopic'] : ['onMessage'];
  const members = names.map(name => { const matches = cls.members.filter(n => n.name?.getText(ast) === name); assert.equal(matches.length, 1, `${className}.${name}`); return matches[0].getText(ast); });
  const trace = [];
  const globals = { exports: {}, cleanFloat, createIncrementalFills, jsonParse() { trace.push('parse'); return globals.parsed; },
    v: (value, key) => value[key], resolveBybitExecution(value) { trace.push('execution'); return value.execution; },
    OrderStatus: { Closed: 'closed', Canceled: 'canceled' }, virtualClock: { getCurrentTimeMs() { trace.push('clock'); return 1234; } } };
  vm.createContext(globals);
  vm.runInContext(ts.transpileModule(`export class Subject {${members.join('\n')}}`, { compilerOptions }).outputText, globals);
  const owner = new globals.exports.Subject();
  owner.isDisposed = false; owner.authRetries = 0; owner.parent = {};
  owner.trackLastReceived = () => trace.push('track'); owner.directFallback = { onMessage: () => trace.push('fallback') };
  owner.emitWsDiagnosticReceive = () => trace.push('diagnostic'); owner.recordMessageHandlerStats = topic => trace.push(`stats:${topic}`);
  owner.emitAndLogError = message => trace.push(`error:${message}`); owner.handlePongEvent = () => trace.push('pong');
  return { owner, globals, trace };
}
const mapped = id => ({ id, side: 'buy', symbol: 'BTCUSD', status: 'open' });
const raw = status => ({ orderStatus: status, price: '12', qty: '10', cumExecValue: '0.25', execution: { amount: 5, price: 19 } });
function orderHost(c, orders) {
  const { owner, trace } = c;
  owner.parent.mapOrder = () => { trace.push('map-order'); return orders; };
  owner.parent.markOrderSeen = id => trace.push(`seen:${id}`);
  owner.incrementalFills = () => { trace.push('incremental'); return { qty: 0.3, value: 0.1 }; };
  owner.emitter = { emit: (_, fill) => { trace.push('fill'); c.fill = fill; } };
  owner.store = { addOrUpdateOrders(rows) { trace.push('store-orders'); assert.equal(rows, orders); } };
}
const result = {};
{
  const a = subject('private').owner.incrementalFills, b = subject('private').owner.incrementalFills;
  assert.notEqual(a, b); result.factory = [a('a', 4).qty, a('a', 7).qty, b('a', 7).qty];
  assert.deepEqual(result.factory, [4, 3, 7], 'source independent factory state');
}
{
  const c = subject('private'), { owner, globals, trace } = c;
  globals.parsed = { topic: 'other', op: 'auth', success: false, ret_msg: 'denied' }; owner.authRetries = 3;
  owner.sendReconnectSequence = sync => { assert.equal(sync, true); assert.equal(owner.authRetries, 4); trace.push('reconnect'); return new Promise(() => {}); };
  owner.onMessage({ data: 'wire' }); result.privateRetry = [...trace];
  owner.onMessage({ data: 'wire' }); assert.equal(trace.filter(v => v === 'reconnect').length, 1, 'source bounded retry');
}
{
  const c = subject('private'), orders = [mapped('one'), mapped('two')]; orderHost(c, orders);
  const batch = [raw('Filled')], emit = c.owner.emitter.emit;
  c.owner.emitter.emit = (...args) => { batch.push(raw('New')); emit(...args); };
  c.owner.handleOrderTopic(batch);
  result.filled = { price: c.fill.price, amount: c.fill.amount, status: orders.map(o => o.status), trace: c.trace };
  assert.equal(c.fill.price, 3); assert.equal(c.trace.filter(v => v === 'map-order').length, 1, 'source forEach captured length');
}
{
  const c = subject('private'), orders = [mapped('one')]; orderHost(c, orders);
  const store = new DefaultStore(), original = { name: 'SubscriberError' }; let later = 0;
  store.subscribe(() => { throw original; }); store.subscribe(() => later++);
  c.owner.store.addOrUpdateOrders = rows => {
    c.trace.push('store-orders');
    store.addOrUpdateOrders(rows.map(o => ({ ...o, type: 'limit', price: 20, amount: 5, filled: 5, remaining: 0, reduceOnly: false, lastUpdate: 1 })));
  };
  let failure; try { c.owner.handleOrderTopic([raw('Filled'), raw('Filled')]); } catch (e) { failure = e; }
  assert.equal(failure, original); assert.equal(store.orders.length, 1); assert.equal(later, 0);
  result.storeFailure = { same: failure === original, committed: store.orders.length, later, status: store.orders[0].status, trace: c.trace };
}
{
  const c = subject('private'), { owner, globals, trace } = c, original = owner.parent;
  const positions = [{ id: 'p1' }, { id: 'p2' }], replacement = { lastWebsocketPositionsUpdate: 9 }; let aliases;
  owner.parent.mapPosition = p => { trace.push('map-position'); return p; };
  owner.store = { updatePositions(updates) { trace.push('store-positions'); aliases = updates.length === 2 && updates.every(([a, b], i) => a === positions[i] && b === positions[i]); } };
  globals.virtualClock.getCurrentTimeMs = () => { trace.push('clock'); owner.parent = replacement; return -0; };
  owner.handlePositionTopic(positions);
  assert(aliases); assert(Object.is(original.lastWebsocketPositionsUpdate, -0));
  result.positions = { aliases, oldNegativeZero: Object.is(original.lastWebsocketPositionsUpdate, -0), replacement: replacement.lastWebsocketPositionsUpdate, trace };
}
{
  const c = subject('public'), { owner, globals, trace } = c, prefix = [], key = 'tickers.BTC\ud800';
  globals.parsed = {}; const json = globals.parsed;
  const exact = [function (frame) { assert.equal(this, exact); assert.equal(frame, json); frame.touched = true; trace.push('exact'); prefix.push(function (frame) { assert.equal(this, prefix); assert.equal(frame, json); trace.push('prefix-appended'); }); }];
  prefix.push(function (frame) { assert.equal(this, prefix); assert.equal(frame.touched, true); trace.push('prefix'); });
  owner.messageHandlers = Object.assign(Object.create({ tickers: prefix }), { [key]: exact });
  owner.recordMessageHandlerStats = topic => { assert.equal(topic.charCodeAt(topic.length - 1), 0xd800); trace.push('stats-utf16'); owner.messageHandlers.tickers = []; };
  owner.onMessage({ data: `{"topic":"${key}"}` }); result.publicLive = trace;
  assert.equal(prefix.length, 2); assert.equal(exact.length, 1);
}

{
  const out = [];
  for (const mode of ['primitive-result', 'primitive-iterator']) {
    const c = subject('private'); let closed = 0, readDone = 0;
    c.globals.readDone = () => { readDone++; };
    vm.runInContext("Object.defineProperty(Number.prototype, 'done', { get() { readDone(); return true; }, configurable: true })", c.globals);
    const orders = { [Symbol.iterator]: () => mode === 'primitive-iterator' ? 1 : { next: () => 1, return() { closed++; return {}; } } };
    orderHost(c, orders); let failure;
    try { c.owner.handleOrderTopic([raw('New')]); } catch (e) { failure = e; }
    assert.equal(failure?.name, 'TypeError', `source ${mode} rejects`); assert.equal(readDone, 0); assert.equal(closed, 0);
    assert.deepEqual(c.trace, ['map-order']); out.push({ mode, typeError: true, readDone, closed });
  }
  result.primitiveIterators = out;
}
{
  const c = subject('private'), original = {}, secondary = {}; let closed = 0;
  const orders = { [Symbol.iterator]: () => ({ next: () => ({ value: { id: 'x' }, done: false }), return() { closed++; throw secondary; } }) };
  orderHost(c, orders); c.owner.parent.markOrderSeen = () => { throw original; }; let failure;
  try { c.owner.handleOrderTopic([raw('New')]); } catch (e) { failure = e; }
  assert.equal(failure, original); assert.equal(closed, 1);
  result.abruptIterator = { same: failure === original, closed };
}
{
  const c = subject('public'), values = [], original = {};
  const handler = function (value) { assert.equal(this, c.owner.messageHandlers.pong); values.push(value); };
  c.owner.messageHandlers = { pong: [handler, handler] }; c.owner.onMessage({ data: 'pong' });
  assert.equal(values.length, 2); assert.notEqual(values[0], values[1]); let later = 0, failure;
  c.owner.messageHandlers.pong = [() => { throw original; }, () => later++];
  try { c.owner.onMessage({ data: 'pong' }); } catch (e) { failure = e; }
  assert.equal(failure, original); assert.equal(later, 0);
  result.pong = { fresh: values[0] !== values[1], same: failure === original, later };
}
assert.deepEqual(Object.keys(result), ['factory','privateRetry','filled','storeFailure','positions','publicLive','primitiveIterators','abruptIterator','pong'], 'nine explicit source cases: iteration stopped at pong');
process.stdout.write(JSON.stringify(result));
