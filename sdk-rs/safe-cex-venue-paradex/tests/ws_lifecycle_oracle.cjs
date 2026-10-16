const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH), root = path.resolve(__dirname, '../../..');
const compiled = new Map();
function execute(code, globals = {}) {
  const module = { exports: {} };
  let out = compiled.get(code);
  if (!out) { out = ts.transpileModule(code, { reportDiagnostics: true, compilerOptions: { module: 1, target: 9 } }); compiled.set(code, out); }
  assert.equal(out.diagnostics.length, 0, 'ws_transpile');
  vm.runInNewContext(out.outputText, { module, exports: module.exports, ...globals });
  return module.exports;
}
const logSource = fs.readFileSync(path.join(root, 'packages/logging/src/LogSeverity.ts'), 'utf8');
const { LogSeverity } = execute(logSource);
const classes = {};
for (const venue of ['public', 'private']) {
  const file = `packages/safe-cex/src/exchanges/paradex/paradex.ws-${venue}.ts`;
  const source = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), 'utf8'), 99, true);
  assert.equal(source.parseDiagnostics.length, 0, 'ws_source_parse');
  const cls = source.statements.find(n => ts.isClassDeclaration(n) && n.name.text === `Paradex${venue === 'public' ? 'Public' : 'Private'}WebSocket`);
  const names = ['connectAndSubscribe', 'ping', 'dispose', ...(venue === 'public' ? ['onOpen', 'hasActiveSubscriptionInterest'] : ['throttledFetchOrders', 'subscribeToPrivateChannels'])];
  const members = names.map(name => { const found = cls.members.filter(n => n.name?.getText(source) === name); assert.equal(found.length, 1, 'ws_member_identity'); return found[0].getText(source); }).join('\n');
  classes[venue] = globals => execute(`export class Harness extends Base { ${members} }`, { LogSeverity, ...globals }).Harness;
}
function bits(value) { const b = Buffer.alloc(8); b.writeDoubleBE(value); return Number.isNaN(value) ? '7ff8000000000000' : b.toString('hex'); }
const units = text => Array.from({ length: text.length }, (_, i) => text.charCodeAt(i));
function run(venue, op, input) {
  const trace = [], error = {}, sends = [], idReads = [], idWrites = [], retained = {}, timers = [], listeners = [];
  let base = false, socket = null, ping = input.mask & 1 ? { name: 'ping' } : undefined, orders = input.mask & 2 ? { name: 'orders' } : undefined;
  let disposedValue = input.disposed ?? false;
  let pingReads = 0, ordersReads = 0, connectedReads = 0, callback, repeat = false, sentCleared = false, pendingCleared = false;
  const retry = new Set(), poll = new Map();
  const touch = name => { trace.push(name); if (input.failure === name) throw error; };
  class Base { dispose() { touch('base'); base = true; } }
  const Harness = classes[venue]({ Base, setTimeout(cb, delay) { touch('schedule'); assert.equal(delay, op === 'throttle' ? 1000 : 30000, 'ws_source_delay'); if (op === 'throttle') assert.equal(cb, retained.fetch, 'ws_source_fetch_identity'); callback = cb; const handle = { name: 'new' }; timers.push(handle); return handle; }, clearTimeout(handle) { touch(`clear-${handle?.name}`); if (input.layout === 2 && handle?.name === 'r0') retry.add({ name: 'r2' }); if (input.layout === 2 && handle?.name === 'p0') poll.set('p2', { name: 'p2' }); } });
  const h = new Harness();
  Object.defineProperty(h, 'isDisposed', { get() { touch('disposed'); return disposedValue; } });
  Object.defineProperty(h, 'isConnected', { get() { touch(`connected${connectedReads}`); return connectedReads++ === 0 ? input.connected : input.lateConnected; } });
  Object.defineProperty(h, 'connectUrl', { get() { touch('url'); return 'url-\ud800'; } });
  Object.defineProperty(h, 'ws', { get() { touch('ws-read'); return socket; }, set(value) { touch('write-socket'); socket = value; } });
  const newSocket = { addEventListener(event, handler) { touch(`add-${event}`); listeners.push([event, handler === retained[event], this === newSocket]); }, close() { touch('close'); } };
  h.createWebsocket = url => { touch('create'); assert.deepEqual(units(url), units('url-\ud800')); return newSocket; };
  for (const [name, event] of [['onOpen', 'open'], ['onMessage', 'message'], ['onClose', 'close']]) {
    retained[event] = h[name] ?? function() {};
    if (op === 'connect') h[name] = retained[event];
  }
  Object.defineProperty(h, 'pingInterval', { get() { touch(`ping${pingReads++}`); return ping; }, set(value) { touch('write-ping'); ping = value; } });
  Object.defineProperty(h, 'ordersThrottleTimeout', { get() { touch(`orders${ordersReads++}`); return orders; }, set(value) { touch('write-orders'); orders = value; } });
  h.subscriptionRetryTimeouts = { forEach(cb) { retry.forEach(cb); }, clear() { touch('retry-clear'); retry.clear(); } };
  h.ohlcvPollingTimeouts = { forEach(cb) { poll.forEach(cb); }, clear() { touch('poll-clear'); poll.clear(); } };
  if (input.layout) { retry.add({ name: 'r0' }); poll.set('p0', { name: 'p0' }); }
  if (input.layout === 2) { retry.add({ name: 'r1' }); poll.set('p1', { name: 'p1' }); }
  h.send = message => { touch(op === 'channels' ? `send${sends.length}` : 'send'); sends.push(JSON.parse(message)); };
  h.sentTradeSymbols = { clear() { touch('clear-sent'); sentCleared = true; } };
  h.pendingSubscribeRequests = { clear() { touch('clear-pending'); pendingCleared = true; } };
  h.subscribeToChannels = () => touch('subscribe');
  h.parent = { log(text, severity) { touch(severity === LogSeverity.Info ? 'log-info' : 'log-warning'); assert.equal(text, severity === LogSeverity.Info ? 'Paradex public WebSocket connected' : 'WebSocket disconnected in onOpen, closing connection'); } };
  if (op === 'open') { socket = input.hasSocket ? newSocket : undefined; h.ping = () => touch('ping-call'); }
  if (op === 'throttle') { const fetch = () => {}; retained.fetch = fetch; Object.defineProperty(h, 'fetchThrottledOrders', { get() { touch('fetch-get'); return fetch; } }); }
  let id = input.id, reads = 0, writes = 0;
  Object.defineProperty(h, 'messageId', { get() { touch(`id-read${reads++}`); idReads.push(bits(id)); return id; }, set(value) { touch(`id-write${writes++}`); idWrites.push(bits(value)); id = value; } });
  let outcome;
  try { const method = { connect: 'connectAndSubscribe', ping: 'ping', dispose: 'dispose', throttle: 'throttledFetchOrders', open: 'onOpen', channels: 'subscribeToPrivateChannels' }[op]; outcome = { ok: true, undefined: h[method]() === undefined }; } catch (e) { outcome = { ok: false, same: e === error }; }
  const before = { trace: trace.slice(), outcome, timer: ping?.name ?? null, orders: orders?.name ?? null };
  let deferred = null;
  if (op === 'ping' && callback && outcome.ok) {
    h.ping = () => { touch('repeat'); repeat = true; };
    disposedValue = input.lateDisposed;
    try { deferred = { ok: true, undefined: callback() === undefined }; } catch (e) { deferred = { ok: false, same: e === error }; }
  }
  return { venue, op, input: { ...input, id: input.id === undefined ? null : bits(input.id) }, before, deferred, trace, sends, listeners, idReads, idWrites, id: input.id === undefined ? null : bits(id), base, socket: socket === newSocket, ping: ping?.name ?? null, orders: orders?.name ?? null, retry: retry.size, poll: poll.size, repeat, sentCleared, pendingCleared };
}
function interest(channel, mask, failure) {
  const trace = [], error = {}, h = new (classes.public({ Base: class {} }))();
  const touch = name => { trace.push(name); if (name === failure) throw error; };
  h.desiredTradeSymbols = { has(symbol) { touch('desired'); return Boolean(mask & 1) && ['X', '\ud800'].includes(symbol); } };
  const markets = mask & 2 ? [{ symbol: 'X' }, { symbol: '\ud800' }] : [];
  h.parent = { store: { get markets() { touch('market'); return markets; } } };
  Object.defineProperty(markets, 'some', { value(cb) { return Array.prototype.some.call(this, cb); } });
  h.orderbookCallbacks = { get(symbol) { touch('book'); return mask & 4 && symbol === 'X' ? { size: 2 } : undefined; } };
  let outcome;
  try { outcome = { ok: true, value: h.hasActiveSubscriptionInterest(channel) }; } catch (e) { outcome = { ok: false, same: e === error }; }
  return { channel: units(channel), mask, failure, trace, outcome };
}
const cases = [];
for (const venue of ['public', 'private']) {
  for (const disposed of [false, true]) for (const failure of ['none', 'disposed', 'url', 'create', 'write-socket', 'add-open', 'add-message', 'add-close']) cases.push(run(venue, 'connect', { disposed, failure }));
  for (const connected of [false, true]) for (const lateDisposed of [false, true]) for (const lateConnected of [false, true]) for (const failure of ['none', 'connected0', 'send', 'schedule', 'write-ping', 'disposed', 'connected1', 'repeat']) cases.push(run(venue, 'ping', { connected, lateDisposed, lateConnected, failure }));
}
for (let mask = 0; mask < 2; mask++) for (let layout = 0; layout < 3; layout++) for (const failure of ['none', 'ping0', 'ping1', 'clear-ping', 'write-ping', 'clear-r0', 'clear-r1', 'retry-clear', 'clear-p0', 'clear-p1', 'poll-clear', 'base']) cases.push(run('public', 'dispose', { mask, layout, failure }));
for (let mask = 0; mask < 4; mask++) for (const failure of ['none', 'ping0', 'ping1', 'clear-ping', 'write-ping', 'orders0', 'orders1', 'clear-orders', 'write-orders', 'base']) cases.push(run('private', 'dispose', { mask, failure }));
for (const mask of [0, 2]) for (const failure of ['none', 'orders0', 'orders1', 'clear-orders', 'fetch-get', 'schedule', 'write-orders']) cases.push(run('private', 'throttle', { mask, failure }));
for (const disposed of [false, true]) for (const connected of [false, true]) for (const hasSocket of [false, true]) for (const failure of ['none', 'disposed', 'log-info', 'ping-call', 'clear-sent', 'clear-pending', 'connected0', 'log-warning', 'ws-read', 'close', 'subscribe']) cases.push(run('public', 'open', { disposed, connected, hasSocket, failure }));
for (const id of [0, -0, 9007199254740991, 9007199254740992, NaN, Infinity]) for (const failure of ['none', 'id-read0', 'id-write0', 'send0', 'id-read1', 'id-write1', 'send1', 'id-read2', 'id-write2', 'send2']) cases.push(run('private', 'channels', { id, failure }));
const interests = [];
for (const channel of ['markets_summary', 'trades.X', 'trades.', 'trades.X.extra', 'bbo.X', 'bbo.', 'order_book.X', 'order_book.', 'unknown', 'trades.\ud800']) for (let mask = 0; mask < 8; mask++) for (const failure of ['none', 'market', 'desired', 'unused', 'book']) interests.push(interest(channel, mask, failure));
process.stdout.write(JSON.stringify({ cases, interests }));
