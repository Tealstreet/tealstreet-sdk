const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);

const root = path.resolve(__dirname, '../../..');
const sourcePath = 'packages/safe-cex/src/exchanges/bybit/bybitinverse.exchange.ts';
const source = ts.createSourceFile(sourcePath, fs.readFileSync(path.join(root, sourcePath), 'utf8'), 99, true);
assert.equal(source.parseDiagnostics.length, 0);
const cls = source.statements.find(node => ts.isClassDeclaration(node) && node.name?.text === 'BybitInverseExchange');
assert.ok(cls);
const members = ['formatBalance', 'fetchOrdersHelper', 'fetchMarkets'].map(name => {
  const found = cls.members.filter(node => node.name?.getText(source) === name);
  assert.equal(found.length, 1, name);
  return found[0].getText(source);
});
const compile = text => ts.transpileModule(text, {
  compilerOptions: { target: 7, module: 1, useDefineForClassFields: true },
}).outputText;
const compiled = compile(`export class BybitInverseExchange {\n${members.join('\n')}\n}`);
const numberModule = { exports: {} };
vm.runInNewContext(compile(fs.readFileSync(path.join(root, 'packages/math/src/isNotNumber.ts'), 'utf8')), numberModule);
const math = {
  exports: {},
  require(name) {
    assert.equal(name, '@tealstreet/math/src/isNotNumber');
    return numberModule.exports;
  },
};
vm.runInNewContext(compile(fs.readFileSync(path.join(root, 'packages/safe-cex/src/utils/safe-math.ts'), 'utf8')), math);

const inputs = [
  { id: 'balance-length-string-zero', method: 'formatBalance', length: '0', iterator: 'bound-target-values' },
  { id: 'orders-length-string-zero', method: 'fetchOrdersHelper', length: '0', iterator: 'bound-target-values' },
  { id: 'markets-then-getter-interleave', method: 'fetchMarkets', iterable: 'second-next-requires-first-then-getter' },
  { id: 'markets-all-capture-before-map', method: 'fetchMarkets', mutation: 'map-replaces-original-all-and-global-Promise' },
  { id: 'markets-noncallable-all-map-throws', method: 'fetchMarkets', selectedAll: 17, map: 'throws-original-primary' },
];
const pinnedIds = [
  'balance-length-string-zero',
  'orders-length-string-zero',
  'markets-then-getter-interleave',
  'markets-all-capture-before-map',
  'markets-noncallable-all-map-throws',
];
assert.equal(inputs.length, 5, 'declared source-only boundary floor');
assert.deepEqual(inputs.map(input => input.id), pinnedIds, 'independent declared input IDs');

function fresh() {
  const trace = [];
  const context = vm.createContext({
    exports: {},
    LogSeverity: { Warning: 1 },
    ENDPOINTS: { UNFILLED_ORDERS: 'orders' },
    v(data, key) {
      assert.ok(Object.hasOwn(data, key), 'fixture uses exact own response keys');
      return data[key];
    },
    subtract: math.exports.subtract,
    observe: event => trace.push(event),
  });
  vm.runInContext(compiled, context);
  return { context, owner: new context.exports.BybitInverseExchange(), trace };
}

function stringZeroLength(rows, trace) {
  return new Proxy(rows, {
    get(target, key, receiver) {
      if (key === 'length') {
        trace.push('length:get');
        return '0';
      }
      if (key === Symbol.iterator) {
        trace.push('iterator:get');
        return target[Symbol.iterator].bind(target);
      }
      return Reflect.get(target, key, receiver);
    },
  });
}

const scenarios = {
  async 'balance-length-string-zero'() {
    const { owner, trace } = fresh();
    const first = {
      totalEquity: '100', totalMaintenanceMargin: '1', totalInitialMargin: '2',
      totalAvailableBalance: '97', coin: [],
    };
    const old = { old: true };
    owner.store = { balance: old, positions: [] };
    owner.log = () => trace.push('warning');
    const result = owner.formatBalance(stringZeroLength([first], trace));
    assert.equal(result.info, first, 'source original balance info identity');
    assert.notEqual(result, old, 'source bypasses empty-balance fallback');
    assert.equal(result.total, 100);
    assert.deepEqual(trace, ['length:get', 'iterator:get']);
    return { outcome: 'fulfilled', total: result.total, originalInfo: result.info === first, oldFallback: result === old, trace };
  },
  async 'orders-length-string-zero'() {
    const { owner, trace } = fresh();
    const row = { id: 'row-one' }, mapped = { id: 'mapped-one' };
    owner.xhr = { get: async (endpoint, options) => {
      trace.push('xhr:get');
      assert.equal(endpoint, 'orders');
      assert.deepEqual(Object.keys(options.params), ['category', 'settleCoin', 'limit', 'cursor']);
      assert.equal(options.params.category, 'inverse');
      assert.equal(options.params.settleCoin, undefined);
      assert.equal(options.params.limit, 50);
      assert.equal(options.params.cursor, '');
      return { data: { retMsg: 'OK', result: { list: stringZeroLength([row], trace), nextPageCursor: '' } } };
    } };
    owner.reinitOnThrow = request => { trace.push('reinit:call'); return request; };
    owner.mapOrder = function (value) {
      assert.equal(this, owner, 'source bound mapOrder receiver');
      assert.equal(value, row);
      trace.push('map:row-one');
      return [mapped];
    };
    const result = await owner.fetchOrdersHelper('inverse');
    assert.equal(result.length, 1);
    assert.equal(result[0], mapped, 'source original mapped-row identity');
    assert.deepEqual(trace, ['xhr:get', 'reinit:call', 'length:get', 'iterator:get', 'map:row-one']);
    return { outcome: 'fulfilled', count: result.length, originalMapped: result[0] === mapped, trace };
  },
  async 'markets-then-getter-interleave'() {
    const { owner, trace } = fresh();
    const market = { symbol: 'BTCUSD' };
    let visitedThen = false, step = 0;
    const thenable = { get then() {
      trace.push('then:get');
      visitedThen = true;
      return function (resolve) {
        assert.equal(this, thenable, 'source thenable receiver');
        trace.push('then:call');
        resolve([market]);
      };
    } };
    const requests = { [Symbol.iterator]() {
      trace.push('iterator:get');
      return { next() {
        trace.push(`next:${step}`);
        if (step++ === 0) return { done: false, value: thenable };
        assert.equal(visitedThen, true, 'source then getter precedes second iterator step');
        return { done: true };
      } };
    } };
    owner.supportedCategories = { map() { trace.push('map:call'); return requests; } };
    owner.fetchMarketsHelper = () => [];
    const result = await owner.fetchMarkets();
    assert.equal(result.length, 1);
    assert.equal(result[0], market, 'source original market identity');
    assert.deepEqual(trace, ['map:call', 'iterator:get', 'next:0', 'then:get', 'next:1', 'then:call']);
    return { outcome: 'fulfilled', count: result.length, originalMarket: result[0] === market, trace };
  },
  async 'markets-all-capture-before-map'() {
    const { context, owner, trace } = fresh();
    vm.runInContext(`
      const originalPromise = Promise, originalAll = Promise.all;
      Object.defineProperty(Promise, 'all', { configurable: true, get() {
        observe('all:get');
        return function (iterable) {
          if (this !== originalPromise) throw new Error('lost original Promise receiver');
          observe('all:call:original-receiver');
          return Reflect.apply(originalAll, this, [iterable]);
        };
      } });
    `, context);
    const originalPromise = vm.runInContext('originalPromise', context);
    const market = { symbol: 'BTCUSD' };
    const categories = { get map() {
      trace.push('map:get');
      return function (callback) {
        assert.equal(this, categories, 'source map receiver');
        assert.equal(typeof callback, 'function');
        trace.push('map:call');
        vm.runInContext(`
          Object.defineProperty(originalPromise, 'all', { configurable: true,
            value() { throw new Error('replacement all must not run'); } });
          globalThis.Promise = function ReplacementPromise() { throw new Error('replacement Promise must not run'); };
        `, context);
        return [originalPromise.resolve([market])];
      };
    } };
    Object.defineProperty(owner, 'supportedCategories', { get() { trace.push('categories:get'); return categories; } });
    const helper = function () {};
    Object.defineProperty(helper, 'bind', { get() { trace.push('bind:get'); return Function.prototype.bind; } });
    Object.defineProperty(owner, 'fetchMarketsHelper', { get() { trace.push('helper:get'); return helper; } });
    const result = await owner.fetchMarkets();
    assert.equal(result.length, 1);
    assert.equal(result[0], market, 'source original market after Promise replacement');
    assert.deepEqual(trace, ['all:get', 'categories:get', 'map:get', 'helper:get', 'bind:get', 'map:call', 'all:call:original-receiver']);
    return { outcome: 'fulfilled', count: result.length, originalMarket: result[0] === market, originalPromiseReceiver: true, trace };
  },
  async 'markets-noncallable-all-map-throws'() {
    const { context, owner, trace } = fresh();
    vm.runInContext(`Object.defineProperty(Promise, 'all', { configurable: true,
      get() { observe('all:get'); return 17; } });`, context);
    const primary = new Error('original-map-primary');
    const categories = { get map() {
      trace.push('map:get');
      return function () { trace.push('map:call'); throw primary; };
    } };
    Object.defineProperty(owner, 'supportedCategories', { get() { trace.push('categories:get'); return categories; } });
    owner.fetchMarketsHelper = () => [];
    let caught;
    try { await owner.fetchMarkets(); } catch (error) { caught = error; }
    assert.equal(caught, primary, 'source argument evaluation error precedes noncallable all error');
    assert.deepEqual(trace, ['all:get', 'categories:get', 'map:get', 'map:call']);
    return { outcome: 'rejected', originalPrimary: caught === primary, trace };
  },
};
assert.deepEqual(Object.keys(scenarios), pinnedIds, 'independent scenario inventory');

(async () => {
  const cases = [];
  for (const input of inputs) cases.push({ ...input, source: await scenarios[input.id]() });
  assert.equal(cases.length, 5, 'executed source-only boundary floor');
  assert.deepEqual(cases.map(value => value.id), pinnedIds, 'executed input inventory');
  assert.equal(cases.filter(value => value.source.outcome === 'fulfilled').length, 4);
  assert.equal(cases.filter(value => value.source.outcome === 'rejected').length, 1);
  const counts = {};
  for (const value of cases) counts[value.method] = (counts[value.method] || 0) + 1;
  assert.deepEqual(counts, { formatBalance: 1, fetchOrdersHelper: 1, fetchMarkets: 3 }, 'independent method floors');
  process.stdout.write(JSON.stringify({ scope: 'source-only review reproducer; no native execution', declaredCases: 5, cases }) + '\n');
})().catch(error => { console.error(error); process.exitCode = 1; });
