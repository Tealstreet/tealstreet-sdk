const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const {load, root} = require('./tables.cjs');
const types = load(path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoin.types.ts'));
const enums = load(path.join(root, 'packages/safe-cex/src/types.ts'));
const parsing = load(path.join(root, 'packages/json-parse/src/jsonParse.ts'));
const file = path.join(root, 'packages/safe-cex/src/exchanges/kucoin/kucoin.ws-private.ts');
const source = fs.readFileSync(file, 'utf8');
const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
assert.equal(ast.statements.filter(n => ts.isClassDeclaration(n) && n.name?.text === 'KucoinPrivateWebsocket').length, 1);
const code = ts.transpileModule(source, {compilerOptions: {target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.CommonJS, useDefineForClassFields: true}}).outputText;
function deferred() {let resolve; const promise = new Promise(r => {resolve = r;}); return {promise, resolve};}
async function settle() {for (let i = 0; i < 8; i++) await Promise.resolve();}
function fixture() {
  const trace = [], frames = [], listeners = [], removed = [], timers = [], mapped = [], updates = [], fills = [], snapshotTimers = [], urls = [], parsed = [];
  const orders = [{id: 'parent-13', side: 'sell', symbol: 'BTC/USDT'}, {id: 'child-17', side: 'buy', symbol: 'ETH/USDT'}];
  const post = deferred(); let snapshots = 0, r;
  let clear = id => trace.push(`clear:${id}`);
  class BaseWebSocket {
    constructor(parent) {this.parent = parent; this.isDisposed = false;}
    get store() {return this.parent.store;}
    get emitter() {return this.parent.emitter;}
    dispose() {trace.push('super.dispose'); this.isDisposed = true; return 97;}
  }
  const module = {exports: {}};
  new Function('require', 'module', 'exports', 'Date', 'performance', 'setTimeout', 'clearTimeout', code)(name => {
    switch (name) {
      case '@tealstreet/json-parse/src/jsonParse': return {jsonParse: input => {const value = parsing.jsonParse(input); parsed.push(value); return value;}};
      case '../../config/config': return {ENABLE_RECONNECTING_WEBSOCKET: true};
      case '../../types': return enums;
      case '../base.ws': return {BaseWebSocket};
      case './kucoin.types': return types;
      default: throw Error(`unexpected class dependency ${name}`);
    }
  }, module, module.exports, {now: () => 41}, {now: () => 79}, (callback, delay) => {assert.equal(delay, 500); timers.push(callback); return timers.length;}, id => clear(id));
  const parent = {
    store: {marketsBySymbol: {'BTC/USDT': {contractSize: 7}}, updateOrderStatus: (order, status) => {updates.push([order, status]); if (updates.length === 1) orders[1] = {id: 'child-19', side: 'buy', symbol: 'ETH/USDT'};}},
    emitter: {emit: (event, value) => {assert.equal(event, 'fill'); fills.push(value);}},
    xhr: {post: url => {urls.push(url); return post.promise;}},
    mapOrder(value) {assert.equal(this, parent); mapped.push(value); return orders;},
    fetchAndSetPrivateSnapshot: (...args) => {assert.equal(args.length, 0); snapshots++; snapshotTimers.push(r.snapshotRefreshTimer); trace.push('snapshot');},
    emitAndLogError: message => {throw Error(`unexpected venue error ${message}`);},
  };
  const Class = module.exports.KucoinPrivateWebsocket; r = new Class(parent);
  const fields = Object.keys(r).slice(2);
  r.sendWithLog = value => frames.push(JSON.parse(value));
  const socket = {
    addEventListener(event, callback) {listeners.push([event, callback]);},
    removeEventListener(event, callback) {assert.equal(this, socket); removed.push([event, callback]);},
    close() {assert.equal(this, socket); assert.equal(r.ws, undefined); trace.push('close');},
  };
  r.createWebsocket = url => {trace.push(`socket:${url}`); return socket;}; r.onClose = () => {};
  for (const key of ['trackLastReceived', 'emitWsDiagnosticReceive', 'handlePongEvent', 'recordMessageHandlerStats', 'retryFailedConnect']) {
    r[key] = (...args) => {if (key === 'handlePongEvent') assert.equal(args.length, 0); trace.push(['recordMessageHandlerStats', 'retryFailedConnect'].includes(key) ? `${key}:${args[0]}` : key);};
  }
  async function resolveToken() {post.resolve({data: {code: '200000', data: {token: 'token-23', instanceServers: [{endpoint: 'wss://future.example'}]}}}); await settle();}
  return {Class, r, parent, fields, trace, frames, listeners, removed, timers, mapped, parsed, updates, fills, snapshotTimers, urls, orders, resolveToken, setClear: f => {clear = f;}, snapshots: () => snapshots};
}
async function run(kind) {
  const f = fixture(), {r} = f;
  switch (kind) {
    case 'layout': {
      const other = new f.Class(f.parent);
      const names = ['connectAndSubscribe', 'onOpen', 'ping', 'onMessage', 'refreshSnapshotSoon', 'reconnect', 'dispose', 'handleOrderTopic', 'handleAlgoOrdersTopic', 'subscribe'];
      for (const key of names) assert.notEqual(r[key], other[key]);
      return {fields: f.fields, arrows: names.map(n => [r[n].name, r[n].length, r[n].constructor.name === 'AsyncFunction']), prototype: Object.getOwnPropertyNames(f.Class.prototype).filter(n => n !== 'constructor'), fresh: true};
    }
    case 'ingress': {
      r.connectAndSubscribe(); assert.deepEqual(f.urls, ['/api/v1/bullet-private']); assert.equal(r.token, null); await f.resolveToken();
      for (const [event, field] of [['open', 'onOpen'], ['message', 'onMessage'], ['close', 'onClose']]) assert.equal(f.listeners.find(row => row[0] === event)[1], r[field]);
      r.onOpen.call({isDisposed: true}); assert.equal(f.frames.length, 7); assert.equal(r.pingAt, 79);
      const message = value => r.onMessage({data: JSON.stringify(value)});
      message({type: 'welcome'}); assert.equal(f.frames.length, 7); r.subscribed = false; message({type: 'welcome'}); message({type: 'pong'});
      message({topic: '/contractMarket/tradeOrders', data: {type: 'canceled', matchSize: '3', matchPrice: '11', price: '17'}});
      message({topic: '/contractMarket/advancedOrders', data: {type: 'cancel'}});
      assert.equal(f.mapped.length, 2); assert.equal(f.parsed.length, 5); assert.equal(f.mapped[0], f.parsed[3].data); assert.equal(f.mapped[1], f.parsed[4].data); assert.equal(f.fills.length, 1); assert.equal(f.updates.length, 3);
      for (const [index, orderIndex] of [[0, 0], [1, 1], [2, 0]]) assert.equal(f.updates[index][0], f.orders[orderIndex]);
      message({topic: '/contractAccount/wallet'}); message({topic: '/contract/positionAll'}); assert.equal(f.timers.length, 2); f.timers[1]();
      assert.equal(r.snapshotRefreshTimer, undefined); assert.deepEqual(f.snapshotTimers, [undefined, undefined]);
      return {url: r.socketEndpoint, frames: f.frames, fill: f.fills[0], statuses: f.updates.map(([order, status]) => [order.id, status]), snapshots: f.snapshots(), trace: f.trace};
    }
    case 'reconnect': {
      r.ws = r.createWebsocket('previous'); r.token = 'previous-token'; r.socketEndpoint = 'previous-endpoint'; r.subscribed = true;
      let completed = false, completion; r.reconnect().then(value => {completed = true; completion = value;}); assert.equal(f.removed.length, 3);
      for (const [index, event, field] of [[0, 'open', 'onOpen'], [1, 'message', 'onMessage'], [2, 'close', 'onClose']]) {assert.equal(f.removed[index][0], event); assert.equal(f.removed[index][1], r[field]);}
      assert.equal(r.ws, undefined); assert.equal(r.token, null); assert.equal(r.socketEndpoint, null); assert.equal(r.subscribed, false); assert.equal(f.urls.length, 1); await settle(); assert.equal(completed, false); await f.resolveToken(); assert.equal(completed, true); assert.equal(completion, undefined); assert.equal(f.listeners.length, 3); assert.equal(r.token, 'token-23');
      const other = new f.Class(f.parent);
      Object.setPrototypeOf(f.Class.prototype, {dispose() {f.trace.push('replacement.dispose'); this.isDisposed = true; return 73;}});
      r.snapshotRefreshTimer = 31; const result = r.dispose.call(other); assert.equal(result, undefined); assert.equal(r.isDisposed, true); assert.equal(other.isDisposed, false); assert.equal(r.snapshotRefreshTimer, undefined);
      r.refreshSnapshotSoon(); assert.equal(f.timers.length, 1); f.timers[0](); assert.equal(r.snapshotRefreshTimer, undefined); assert.equal(f.snapshots(), 0);
      const failure = new Error('clear-failure'); f.setClear(() => {throw failure;}); r.snapshotRefreshTimer = 37; let caught; try {r.dispose();} catch (error) {caught = error;}
      assert.equal(caught, failure); assert.equal(r.snapshotRefreshTimer, 37);
      return {removed: f.removed.map(row => row[0]), retained: true, disposed: r.isDisposed, otherDisposed: other.isDisposed, throwIdentity: caught === failure, throwTimer: r.snapshotRefreshTimer, snapshots: f.snapshots(), trace: f.trace};
    }
    default: throw Error(`unknown case ${kind}`);
  }
}
run(process.argv[2]).then(value => process.stdout.write(JSON.stringify(value))).catch(error => {console.error(error); process.exitCode = 1;});
