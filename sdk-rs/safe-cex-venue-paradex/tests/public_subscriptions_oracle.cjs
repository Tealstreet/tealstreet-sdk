const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH), root = path.resolve(__dirname, '../../..'), compiled = new Map();
function execute(code, globals = {}) {
  const module = { exports: {} }; let out = compiled.get(code);
  if (!out) { out = ts.transpileModule(code, { reportDiagnostics: true, compilerOptions: { module: 1, target: 9 } }); assert.equal(out.diagnostics.length, 0, 'subscriptions_transpile'); compiled.set(code, out); }
  vm.runInNewContext(out.outputText, { module, exports: module.exports, ...globals }); return module.exports;
}
const file = 'packages/safe-cex/src/exchanges/paradex/paradex.ws-public.ts';
const source = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), 99, true);
assert.equal(source.parseDiagnostics.length, 0, 'subscriptions_source_parse');
const cls = source.statements.find(n => ts.isClassDeclaration(n) && n.name.text === 'ParadexPublicWebSocket');
const names = ['sendJsonRpcSubscription', 'sendTradeSubscribe', 'subscribeToOrderBook', 'unsubscribeFromOrderBook', 'subscribeToTrades', 'unsubscribeFromTrades', 'cleanupOhlcvSubscription', 'subscribeToChannels'];
const members = names.map(name => { const nodes = cls.members.filter(n => n.name?.getText(source) === name); assert.equal(nodes.length, 1, 'subscriptions_member_identity'); return nodes[0].getText(source); }).join('\n');
const helper = source.statements.filter(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(source) === 'getOrderBookChannel'));
assert.equal(helper.length, 1, 'subscriptions_channel_identity');
const { getOrderBookChannel } = execute(`${helper[0].getText(source)}\nmodule.exports = { getOrderBookChannel };`);
const { LogSeverity } = execute(fs.readFileSync(path.join(root, 'packages/logging/src/LogSeverity.ts'), 'utf8'));
const units = text => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
function bits(value) { const b = Buffer.alloc(8); b.writeDoubleBE(value); return Number.isNaN(value) ? '7ff8000000000000' : b.toString('hex'); }
const numeric = value => value === undefined ? null : bits(value);
const symbol = mode => mode === 'empty' ? '' : mode === 'surrogate' ? 'X\ud800' : 'X';
const values = [undefined, 0, -0, NaN, 1, 2, -1, 0.5, Infinity, 9007199254740992];
function simple(op, input) {
  const trace = [], error = {}, requests = [], calls = [], pending = [], writes = [], deleted = [], logs = [];
  let id = input.id ?? 0, count = input.count, version = input.version, active = true, timer = input.hasTimer ? { marker: 'timer' } : undefined, desired = false, sent = input.sent ?? false;
  const fields = ['ohlcvCallbacks', 'ohlcvIntervals', 'ohlcvLastTradeAt', 'ohlcvLastRecoveryAt', 'lastCandleCache'];
  const retained = new Set(fields);
  const touch = name => { trace.push(name); if (name === input.failure) throw error; };
  const Harness = execute(`export class Harness {${members}}`, {
    LogSeverity,
    getOrderBookChannel(s) { touch('channel'); return getOrderBookChannel(s); },
    clearTimeout(value) { touch('timer-clear'); assert.equal(value, timer, 'subscriptions_source_timer_identity'); },
    JSON: { stringify(value) { requests.push({ id: bits(value.id), jsonrpc: value.jsonrpc, method: value.method, channel: units(value.params.channel), keys: Object.keys(value), paramsKeys: Object.keys(value.params) }); touch('stringify'); return JSON.stringify(value); } }
  }).Harness;
  const h = new Harness(), s = symbol(input.symbol ?? 'normal'), key = 'X\ud800:1m';
  Object.defineProperty(h, 'isConnected', { get() { touch('connected'); return input.connected; } });
  Object.defineProperty(h, 'messageId', { get() { touch('id-read'); return id; }, set(value) { touch('id-write'); id = value; } });
  h.pendingSubscribeRequests = { set(givenId, value) { touch('pending'); pending.push({ id: bits(givenId), channel: units(value.channel), attempt: bits(value.attempt) }); } };
  h.send = value => { touch('send'); const decoded = JSON.parse(value); calls.push({ ...decoded, params: { channel: units(decoded.params.channel) } }); };
  if (op !== 'rpc') h.sendJsonRpcSubscription = function(method, channel, attempt) { touch('rpc'); calls.push({ method, channel: units(channel), attempt: numeric(attempt), argc: arguments.length }); };
  h.desiredTradeSymbols = { add(given) { assert.equal(given, s); touch('desired-add'); desired = true; } };
  h.sentTradeSymbols = { has(given) { assert.equal(given, s); touch('sent-has'); return sent; }, add(given) { assert.equal(given, s); touch('sent-add'); sent = true; } };
  h.tradeSubscriptionCounts = {
    get(given) { assert.equal(given, s); touch('count-get'); return count; },
    set(given, value) { assert.equal(given, s); touch('count-set'); count = value; writes.push(bits(value)); },
    delete(given) { assert.equal(given, s); touch('count-delete'); count = undefined; }
  };
  h.sendTradeSubscribe = op === 'trade' ? h.sendTradeSubscribe : function(given) { assert.equal(given, s); touch('trade'); calls.push({ symbol: units(given), argc: arguments.length }); };
  h.ohlcvPollingActive = { set(given, value) { assert.equal(given, key); touch('active-set'); active = value; } };
  h.ohlcvSubscriptionVersions = { get(given) { assert.equal(given, key); touch('version-get'); return version; }, set(given, value) { assert.equal(given, key); touch('version-set'); version = value; } };
  h.ohlcvPollingTimeouts = { has(given) { assert.equal(given, key); touch('timer-has'); return !!timer; }, get(given) { assert.equal(given, key); touch('timer-get'); return timer; }, delete(given) { assert.equal(given, key); touch('timer-delete'); timer = undefined; } };
  for (const field of fields) h[field] = { delete(given) { assert.equal(given, key); touch(`${field}-delete`); retained.delete(field); deleted.push(field); } };
  let outcome;
  try {
    let returned;
    if (op === 'rpc') returned = h.sendJsonRpcSubscription(input.method, s, input.attempt);
    else if (op === 'trade') returned = h.sendTradeSubscribe(s, input.force, input.attempt);
    else if (op === 'book-sub') returned = h.subscribeToOrderBook(s);
    else if (op === 'book-unsub') returned = h.unsubscribeFromOrderBook(s);
    else if (op === 'count-sub') returned = h.subscribeToTrades(s);
    else if (op === 'count-unsub') returned = h.unsubscribeFromTrades(s);
    else if (op === 'cleanup') returned = h.cleanupOhlcvSubscription(key);
    outcome = { ok: true, undefined: returned === undefined };
  } catch (e) { outcome = { ok: false, same: e === error }; }
  return { op, input: { ...input, id: numeric(input.id), attempt: numeric(input.attempt), count: numeric(input.count), version: numeric(input.version), force: input.force ?? null }, symbol: units(s), key: units(key), trace, outcome, requests, calls, pending, id: bits(id), count: numeric(count), version: numeric(version), active, timer: !!timer, desired, sent, writes, deleted, retained: Array.from(retained) };
}
function broadcast(layout, failure) {
  const trace = [], error = {}, calls = [], logs = [], symbols = layout === 'empty' ? [] : ['X', 'Y'], books = new Map(layout === 'empty' ? [] : layout === 'normal' ? [['X', 2], ['Y', 0], ['X\ud800', 1]] : layout === 'grow' ? [['X', 1]] : [['X', 0], ['Y', 1]]);
  let symbolReads = 0, rpcCalls = 0, tradeCalls = 0, logCalls = 0, bookReads = 0, bookVisits = 0, bookCalls = 0;
  const touch = name => { trace.push(name); if (name === failure) throw error; };
  const marketSymbols = new WeakMap();
  function market(s) { const value = { get symbol() { touch(`market-symbol${symbolReads++}`); return s; } }; marketSymbols.set(value, s); return value; }
  const markets = symbols.map(market);
  const Harness = execute(`export class Harness {${members}}`, { LogSeverity }).Harness, h = new Harness();
  h.parent = { store: { get markets() { touch('markets'); return markets; } }, log(message, severity) { touch(`log${logCalls++}`); logs.push([units(message), severity]); } };
  // Count the two intrinsic calls independently of subscriber effects.
  let passes = 0;
  Object.defineProperty(markets, 'forEach', { configurable: true, value(cb) { touch(`visit${passes++}`); return Array.prototype.forEach.call(this, cb); } });
  h.sendJsonRpcSubscription = function(method, channel) {
    const index = rpcCalls++; touch(`rpc${index}`); calls.push({ kind: 'rpc', method, channel: units(channel), argc: arguments.length });
    if (index === 0 && layout === 'grow') markets.push(market('Z'));
    if (index === 0 && layout === 'replace') markets[1] = market('X\ud800');
  };
  h.sendTradeSubscribe = function(s, force) { const index = tradeCalls++; touch(`trade${index}`); calls.push({ kind: 'trade', symbol: units(s), force, argc: arguments.length }); if (index === 0 && layout === 'grow') markets.push(market('W')); };
  h.orderbookCallbacks = {
    get size() { touch(`book-size${bookReads++}`); return books.size; },
    forEach(cb) { touch('books-visit'); books.forEach((size, s) => cb({ get size() { touch(`book-callback${bookVisits++}`); return size; } }, s)); }
  };
  h.subscribeToOrderBook = s => { const index = bookCalls++; touch(`book-subscribe${index}`); calls.push({ kind: 'book', symbol: units(s) }); if (layout === 'grow' && index === 0) books.set('Z', 1); if (layout === 'replace' && index === 0) { books.delete('Y'); books.set('X', 1); } };
  let outcome; try { outcome = { ok: true, undefined: h.subscribeToChannels() === undefined }; } catch (e) { outcome = { ok: false, same: e === error }; }
  return { layout, failure, trace, outcome, calls, logs, markets: markets.map(m => units(marketSymbols.get(m))), books: Array.from(books, ([s, size]) => [units(s), size]) };
}
const cases = [];
for (const method of ['subscribe', 'unsubscribe']) for (const connected of [false, true]) for (const id of [0, -0, 9007199254740991, 9007199254740992, NaN, Infinity]) for (const attempt of [undefined, -1, NaN]) for (const failure of ['none', 'connected', 'id-read', 'id-write', 'pending', 'stringify', 'send']) cases.push(simple('rpc', { method, connected, id, attempt, failure, symbol: 'surrogate' }));
for (const s of ['empty', 'normal', 'surrogate']) for (const force of [undefined, false, true]) for (const sent of [false, true]) for (const attempt of [undefined, -1, NaN]) for (const failure of ['none', 'desired-add', 'sent-has', 'sent-add', 'rpc']) cases.push(simple('trade', { symbol: s, force, sent, attempt, failure }));
for (const op of ['book-sub', 'book-unsub']) for (const s of ['empty', 'normal', 'surrogate']) for (const connected of [false, true]) for (const failure of ['none', 'connected', 'channel', 'rpc']) cases.push(simple(op, { symbol: s, connected, failure }));
for (const op of ['count-sub', 'count-unsub']) for (const s of ['empty', 'normal', 'surrogate']) for (const count of values) for (const failure of ['none', 'count-get', 'count-set', op === 'count-sub' ? 'trade' : 'count-delete']) cases.push(simple(op, { symbol: s, count, failure }));
for (const hasTimer of [false, true]) for (const version of values) for (const failure of ['none', 'active-set', 'version-get', 'version-set', 'timer-has', 'timer-get', 'timer-clear', 'timer-delete', 'ohlcvCallbacks-delete', 'ohlcvIntervals-delete', 'ohlcvLastTradeAt-delete', 'ohlcvLastRecoveryAt-delete', 'lastCandleCache-delete']) cases.push(simple('cleanup', { hasTimer, version, failure }));
const broadcasts = [];
for (const layout of ['empty', 'normal', 'grow', 'replace']) for (const failure of ['none', 'markets', 'market-symbol0', 'rpc0', 'rpc1', 'trade0', 'trade1', 'log0', 'book-size0', 'book-callback0', 'book-subscribe0', 'book-size1', 'log1', 'visit1']) broadcasts.push(broadcast(layout, failure));
process.stdout.write(JSON.stringify({ cases, broadcasts }));
