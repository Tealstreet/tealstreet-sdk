import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const lodash = require('lodash');
const root = new URL('../../../packages/safe-cex/src/', import.meta.url);
function ast(file) {
  return ts.createSourceFile(file, fs.readFileSync(new URL(file, root), 'utf8'), ts.ScriptTarget.Latest, true);
}
function body(file) {
  const source = ast(file);
  return source.statements.filter(node => !ts.isImportDeclaration(node))
    .map(node => node.getText(source)).join('\n').replace(/\bexport /g, '');
}
function fixture() {
  const f = {requests: [], holds: new Map(), loops: [], errors: [], now: 0};
  class Clock extends Date {static now() {return f.now;}}
  class Base {
    constructor(name, options, store) {Object.assign(this, {name, options, store, isDisposed: false});}
    get isPublicDataOnly() {return !!this.options.publicDataOnly;}
    async initExternalPublicData() {return false;}
    getTickInterval() {return undefined;}
    emitAndLogError(message) {f.errors.push(message);}
    dispose() {this.isDisposed = true; this.store.reset();}
  }
  class Socket {
    constructor(parent) {this.parent = parent;}
    cleanupMessageHandlerStats(topic) {assert.equal(this.messageHandlers[topic].length, 0);}
    dispose() {assert.equal(this.parent.isDisposed, true);}
  }
  function createAPI() {
    async function request(method, url, config = {}) {
      for (const [key, value] of Object.entries(config.params || {})) url = url.replace(`{${key}}`, value);
      f.requests.push({method, url});
      const hold = f.holds.get(url);
      if (hold) {f.holds.delete(url); await hold.promise;}
      if (f.failures?.has(url)) throw Error('cancel refused ' + url.split('/').at(-1));
      let data;
      if (url === '/login') data = {sessionToken: 'token', timeout: '1:00:00'};
      else if (url === '/users/k') data = {userDetails: [{accounts: [{account: 'default:A', accountStatus: 'FULL_TRADING'}]}]};
      else if (url === 'instruments/query') data = {instruments: [{symbol: 'BTCUSD', priceIncrement: .1, pipSize: .01, firstCurrency: 'BTC', currency: 'USD'}]};
      else if (url === 'accounts/default:A/instruments/query') data = {instrumentDetails: [{symbol: 'BTCUSD', minOrderSize: .01, minOrderSizeIncrement: .01, maxOrderSize: {value: 100}, marginRate: {tiers: [{value: .05}]}}]};
      else if (url === 'accounts/default:A/portfolio') data = {portfolios: [{balances: [{currency: 'USD', value: 50.25}], positions: [], orders: []}]};
      else if (url === 'marketdata') data = {events: [{symbol: 'BTCUSD', bid: 99, ask: 101}]};
      else {assert.equal(method, 'DELETE'); assert.ok(url.startsWith('accounts/default:A/orders/')); data = {};}
      return {data};
    }
    return {get: (url, config) => request('GET', url, config),
      post: (url, _body, config) => request('POST', url, config),
      delete: (url, config) => request('DELETE', url, config)};
  }
  const context = vm.createContext({Date: Clock, Error, structuredClone,
    BaseExchange: Base, BaseWebSocket: Socket, createAPI, rateLimit: client => client,
    chunk: lodash.chunk, uniqBy: lodash.uniqBy, omitBy: lodash.omitBy,
    loop: callback => f.loops.push(callback), virtualClock: {getCurrentTimeMs: () => f.now}});
  function run(code) {
    vm.runInContext(ts.transpileModule(code, {compilerOptions: {target: ts.ScriptTarget.ES2022}}).outputText, context);
  }
  const types = ast('types.ts');
  const enums = types.statements.filter(node => ts.isEnumDeclaration(node));
  const seconds = types.statements.find(node => ts.isVariableStatement(node) &&
    node.declarationList.declarations.some(declaration => declaration.name.getText(types) === 'timeframeToSeconds'));
  assert.ok(seconds);
  run(enums.map(node => node.getText(types).replace('export ', '')).join('\n') + '\n' + seconds.getText(types).replace('export ', ''));
  for (const file of ['utils/clone.ts', 'store/store.base.ts', 'utils/createPositionId.ts', 'utils/inverseObj.ts',
    'exchanges/breakoutprop/breakoutprop.types.ts', 'exchanges/breakoutprop/breakoutprop.ws-public.ts',
    'exchanges/breakoutprop/breakoutprop.exchange.ts']) run(body(file));
  run('globalThis.ExchangeClass = BreakoutpropExchange; globalThis.StoreClass = DefaultStore;');
  f.store = new context.StoreClass();
  f.exchange = new context.ExchangeClass('breakoutprop', {key: 'k', secret: 's'}, f.store);
  f.flush = async () => {for (let i = 0; i < 256; i++) await Promise.resolve();};
  f.hold = url => {
    let release;
    const promise = new Promise(resolve => {release = resolve;});
    f.holds.set(url, {promise});
    return release;
  };
  return f;
}
function deleteIds(f) {return f.requests.filter(row => row.method === 'DELETE').map(row => row.url.split('/').at(-1));}
const startup = fixture();
assert.equal(Object.keys(startup.exchange.publicWebsocket.messageHandlers).length, 0);
const releaseLogin = startup.hold('/login');
const deliveries = [];
const unsubscribe = startup.exchange.listenOrderBook('BTCUSD', book => deliveries.push(book));
void startup.exchange.start();
await startup.flush();
assert.equal(startup.requests.length, 1);
assert.equal(startup.store.loaded.markets, false);
assert.equal(deliveries.length, 0);
releaseLogin();
await startup.flush();
assert.equal(startup.store.balance.total, 50.25);
assert.equal(startup.store.markets.length, 1);
assert.equal(startup.store.tickers.length, 1);
assert.deepEqual(Object.values(startup.store.loaded), [true, true, true, true, true]);
assert.equal(deliveries.length, 1);
const firstBook = deliveries[0];
const startupRequests = startup.requests.slice();
await startup.loops.shift()();
assert.equal(deliveries.length, 2);
assert.equal(firstBook, deliveries[1]);
unsubscribe();
await startup.loops.shift()();
assert.equal(deliveries.length, 2);
startup.exchange.dispose();
const beforeDispose = startup.requests.length;
await startup.loops.shift()();
assert.equal(startup.requests.length, beforeDispose);
assert.equal(deliveries.length, 2);
assert.deepEqual(startup.errors, []);

const disposed = fixture();
const releaseDisposed = disposed.hold('/login');
let lateCallbacks = 0;
disposed.exchange.listenOrderBook('BTCUSD', () => lateCallbacks++);
const disposedStart = disposed.exchange.start();
await disposed.flush();
disposed.exchange.dispose();
releaseDisposed();
await disposedStart;
assert.equal(lateCallbacks, 0);
assert.ok(Object.values(disposed.store.loaded).every(value => value === false));
assert.equal(disposed.exchange.sessionTokenRequest, null);

const cancellations = [];
for (const failureId of [null, 0, 9]) {
  const reject = failureId !== null;
  const f = fixture();
  f.exchange.sessionToken = 'cached'; f.exchange.sessionTokenExpiresAt = 3_600_000;
  f.exchange.accountCode = 'default:A';
  const release = f.hold(`accounts/default:A/orders/${failureId ?? 0}`);
  const siblingId = failureId === 0 ? 9 : 0;
  const releaseSibling = reject ? f.hold(`accounts/default:A/orders/${siblingId}`) : () => {};
  if (reject) f.failures = new Set([failureId, siblingId].map(id => `accounts/default:A/orders/${id}`));
  const orders = Array.from({length: 21}, (_, id) => ({id: String(id)}));
  const pending = f.exchange.cancelOrders(orders);
  let failure;
  pending.catch(error => {failure = error.message;});
  await f.flush();
  assert.equal(deleteIds(f).length, 10);
  orders[20].id = 'mutated';
  release();
  await f.flush();
  const ids = deleteIds(f);
  assert.equal(ids.length, reject ? 10 : 21);
  if (reject) {
    assert.equal(failure, `cancel refused ${failureId}`);
    assert.equal(f.errors.length, 1);
    releaseSibling();
    await f.flush();
    assert.equal(f.errors.length, 2);
    assert.equal(deleteIds(f).length, 10);
  }
  else assert.equal(ids.at(-1), 'mutated');
  cancellations.push({reject, failureId, rejection: failure, ids, errors: f.errors});
  f.exchange.dispose();
}
console.log(JSON.stringify({startup: {requests: startupRequests, callbacks: 2, balance: 50.25}, cancellations, disposed: {callbacks: lateCallbacks, loaded: disposed.store.loaded, requests: disposed.requests}}));
