const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {load, root} = require('./tables.cjs');
const types = load(path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoinspot.types.ts'));
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const parsing = load(path.join(root, 'packages/json-parse/src/jsonParse.ts'));
const file = path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoinspot.ws-private.ts');
const source = fs.readFileSync(file, 'utf8');
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
assert.equal(ast.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === 'KucoinSpotPrivateWebsocket').length, 1);
const code = ts.transpileModule(source, {compilerOptions: {
  target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.CommonJS, useDefineForClassFields: true,
}}).outputText;
function deferred() {
  let resolve;
  const promise = new Promise(r => {resolve = r;});
  return {promise, resolve};
}
async function settle() {for (let i = 0; i < 8; i++) await Promise.resolve();}
function fixture() {
  const trace = [], frames = [], listeners = [], timers = [], updates = [], postUrls = [];
  const balance = deferred(), post = deferred();
  const counts = {orders: 0, balance: 0};
  const clock = {now: () => 41};
  let clear = id => trace.push(`clear:${id}`);
  class BaseWebSocket {
    constructor(parent) {this.parent = parent; this.isDisposed = false;}
    get store() {return this.parent.store;}
    dispose() {trace.push('super.dispose'); this.isDisposed = true; return 97;}
  }
  const module = {exports: {}};
  new Function('require', 'module', 'exports', 'Date', 'performance', 'setTimeout', 'clearTimeout', code)(name => {
    switch (name) {
      case '@tealstreet/json-parse/src/jsonParse': return parsing;
      case '../../config/config': return {ENABLE_RECONNECTING_WEBSOCKET: true};
      case '../../types': return enums;
      case '../base.ws': return {BaseWebSocket};
      case './kucoinspot.types': return types;
      default: throw Error(`unexpected class dependency ${name}`);
    }
  }, module, module.exports, clock, {now: () => 79}, (callback, delay) => {
    assert.equal(delay, 250); timers.push(callback); return timers.length;
  }, id => clear(id));
  const parent = {
    store: {loaded: {orders: 43}, update: update => updates.push(update)},
    xhr: {post: url => {postUrls.push(url); return post.promise;}},
    fetchBalance: () => {counts.balance++; return balance.promise;},
    fetchAndSetOrders: () => {counts.orders++;},
    fetchAndSetPrivateSnapshot: n => trace.push(`snapshot:${n}`),
    emitAndLogError: message => trace.push(`error:${message}`),
  };
  const Class = module.exports.KucoinSpotPrivateWebsocket;
  const r = new Class(parent);
  const fields = Object.keys(r).slice(2);
  r.sendWithLog = value => frames.push(JSON.parse(value));
  r.createWebsocket = url => {
    trace.push(`socket:${url}`);
    return {addEventListener: (event, callback) => listeners.push([event, callback])};
  };
  r.onClose = () => {};
  for (const key of ['trackLastReceived', 'emitWsDiagnosticReceive', 'handlePongEvent', 'recordMessageHandlerStats', 'retryFailedConnect']) {
    r[key] = (...args) => trace.push(['handlePongEvent', 'recordMessageHandlerStats', 'retryFailedConnect'].includes(key) ? `${key}:${args[0]}` : key);
  }
  return {Class, r, fields, parent, trace, frames, listeners, timers, updates, postUrls, counts, post, balance, setClear: f => {clear = f;}};
}
async function run(kind) {
  const f = fixture(), {r} = f;
  switch (kind) {
    case 'layout': {
      const other = new f.Class(f.parent);
      const names = ['connectAndSubscribe', 'onOpen', 'ping', 'onMessage', 'handleOrderTopic', 'subscribe', 'refreshOrdersSoon', 'refreshBalanceSoon'];
      for (const name of names) assert.notEqual(r[name], other[name]);
      return {fields: f.fields, arrows: names.map(n => [r[n].name, r[n].length, r[n].constructor.name === 'AsyncFunction']), prototype: Object.getOwnPropertyNames(f.Class.prototype).filter(n => n !== 'constructor'), prototypeMeta: [f.Class.prototype.dispose.name, f.Class.prototype.dispose.length, f.Class.prototype.dispose.constructor.name === 'AsyncFunction'], fresh: true, interval: r.pingInterval};
    }
    case 'ingress': {
      r.connectAndSubscribe();
      assert.deepEqual(f.postUrls, ['/api/v1/bullet-private']);
      assert.equal(r.token, null);
      f.post.resolve({data: {code: '200000', data: {token: 'token-23', instanceServers: [{endpoint: 'wss://socket.example', pingInterval: 61}]}}});
      await settle();
      for (const [event, field] of [['open', 'onOpen'], ['message', 'onMessage'], ['close', 'onClose']]) assert.equal(f.listeners.find(row => row[0] === event)[1], r[field]);
      r.onOpen();
      let liveSend = 0, pingAt;
      Object.defineProperty(r, 'pingAt', {configurable: true, get: () => pingAt, set: value => {pingAt = value; r.sendWithLog = () => {liveSend++;};}});
      r.ping.call({isDisposed: true});
      assert.equal(pingAt, 79);
      r.isDisposed = true;
      r.onMessage({get data() {r.isDisposed = false; return '{"type":"pong"}';}});
      for (const topic of ['/spotMarket/tradeOrdersV2-extra', '/account/balance-extra']) r.onMessage({data: JSON.stringify({topic, data: {}})});
      return {url: r.socketEndpoint, interval: r.pingInterval, frames: f.frames, listeners: f.listeners.map(row => row[0]), liveSend, timers: [r.orderRefreshTimeout, r.balanceRefreshTimeout], trace: f.trace};
    }
    case 'timers': {
      for (const key of ['refreshOrdersSoon', 'refreshBalanceSoon']) {r[key](); r[key]();}
      assert.equal(f.timers.length, 2);
      f.timers[0](); f.timers[1]();
      assert.equal(r.orderRefreshTimeout, undefined); assert.equal(r.balanceRefreshTimeout, undefined);
      assert.deepEqual(f.counts, {orders: 1, balance: 1}); assert.equal(f.updates.length, 0);
      const balance = {used: 17, free: 29}; f.balance.resolve(balance); await settle();
      assert.equal(f.updates.length, 1); assert.equal(f.updates[0].balance, balance);
      const disposed = fixture(); disposed.r.refreshBalanceSoon(); disposed.timers[0](); disposed.r.isDisposed = true; disposed.balance.resolve(97); await settle(); assert.equal(disposed.updates.length, 0);
      return {timers: f.timers.length, fetches: [f.counts.orders, f.counts.balance], cleared: true, retained: true, update: f.updates[0], disposedUpdates: disposed.updates.length};
    }
    case 'dispose': {
      const other = new f.Class(f.parent); let reads = 0, timer = 101;
      Object.defineProperty(other, 'orderRefreshTimeout', {configurable: true, get() {if (timer === undefined) return undefined; reads++; return reads === 1 ? 101 : 107;}, set(v) {timer = v;}});
      other.balanceRefreshTimeout = 103;
      const result = f.Class.prototype.dispose.call(other);
      assert.equal(result, undefined); assert.equal(other.isDisposed, true); assert.equal(r.isDisposed, false); assert.equal(timer, undefined); assert.equal(other.balanceRefreshTimeout, undefined);
      const replacement = new f.Class(f.parent);
      Object.setPrototypeOf(f.Class.prototype, {dispose() {f.trace.push('replacement.dispose'); this.isDisposed = true; return 73;}});
      assert.equal(f.Class.prototype.dispose.call(replacement), undefined); assert.equal(replacement.isDisposed, true);
      const failure = new Error('clear-failure'); f.setClear(() => {throw failure;}); r.orderRefreshTimeout = 109;
      let caught; try {f.Class.prototype.dispose.call(r);} catch (e) {caught = e;}
      assert.equal(caught, failure); assert.equal(r.orderRefreshTimeout, 109);
      return {trace: f.trace, reads, disposed: other.isDisposed, originalUntouched: !r.isDisposed, resultUndefined: result === undefined, throwIdentity: caught === failure, throwTimer: r.orderRefreshTimeout};
    }
    default: throw Error(`unknown case ${kind}`);
  }
}
run(process.argv[2]).then(value => process.stdout.write(JSON.stringify(value))).catch(error => {console.error(error); process.exitCode = 1;});
