// Real TS API/signing vs native WASM commands; documented V5 product payloads.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHmac } from 'node:crypto';
import { createRequire } from 'node:module';
import { BybitExchange } from '../../packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts';
import { DefaultStore } from '../../packages/safe-cex/src/store/store.base.ts';
import { FrameApplier } from '../../packages/safe-cex/src/store/frameApplier.ts';
import { Frame } from '../hosts/wasm/js/wire.mjs';
const require = createRequire(import.meta.url);
const { WasmEngine } = require('../hosts/wasm/pkg/node/safe_cex_wasm.js');
const KEY = 'product-fixture-key', SECRET = 'product-fixture-secret';
const EXPIRY = 1798185600000;
const CALL = 'BTC-25DEC26-100000-C-USDT', PUT = 'BTC-25DEC26-100000-P-USDT', ETH_PUT = 'ETH-25DEC26-3000-P-USDT';
const ok = (list, category) => ({ retCode: 0, retMsg: 'OK', result: { list, ...(category ? { category } : {}), nextPageCursor: '' } });
const instrument = (symbol, category) => ({
  symbol, status: 'Trading', contractType: category === 'inverse' ? 'InversePerpetual' : 'LinearPerpetual',
  baseCoin: 'BTC', quoteCoin: category === 'inverse' ? 'USD' : 'USDT', settleCoin: category === 'inverse' ? 'BTC' : 'USDT',
  deliveryTime: '0', launchTime: '1585526400000', symbolType: '',
  leverageFilter: { minLeverage: '1', maxLeverage: '100' },
  priceFilter: { minPrice: '0.1', maxPrice: '10000000', tickSize: '0.1' },
  lotSizeFilter: { qtyStep: category === 'inverse' ? '1' : '0.001', minOrderQty: category === 'inverse' ? '1' : '0.001', maxOrderQty: '10000', maxMktOrderQty: '10000' },
});
const option = (symbol, optionsType) => ({
  symbol, status: 'Trading', baseCoin: symbol.split('-')[0], quoteCoin: 'USDT', settleCoin: 'USDT', optionsType,
  deliveryTime: String(EXPIRY), priceFilter: { tickSize: '5', minPrice: '5' },
  lotSizeFilter: { qtyStep: '0.01', minOrderQty: '0.01', maxOrderQty: '100' },
});
const CATALOG = { linear: [instrument('BTCUSDT', 'linear')], inverse: [instrument('BTCUSD', 'inverse')], option: [option(CALL, 'Call'), option(PUT, 'Put'), option(ETH_PUT, 'Put')] };
const quote = (symbol) => ({ symbol, lastPrice: '1000', bid1Price: '1000', ask1Price: '1010', markPrice: '1005',
  indexPrice: '99000', volume24h: '2', turnover24h: '2000', price24hPcnt: '0', fundingRate: '0', nextFundingTime: '0',
  markIv: '0.5', bid1Iv: '0.49', ask1Iv: '0.51', underlyingPrice: '99000', delta: symbol === CALL ? '0.4' : '-0.6', gamma: '0.01', vega: '12', theta: '-8' });
const GREEKS = [{ baseCoin: 'BTC', totalDelta: '0.2', totalGamma: '0.03', totalVega: '9', totalTheta: '-7' }];

function validate(request) {
  const headers = Object.fromEntries((Array.isArray(request.headers) ? request.headers : Object.entries(request.headers ?? {})).map(([k, v]) => [k.toLowerCase(), String(v)]));
  const url = new URL(request.url), signed = request.method !== 'GET' || !url.pathname.startsWith('/v5/market/');
  if (!signed && !headers['x-bapi-sign']) return;
  const timestamp = headers['x-bapi-timestamp'], window = headers['x-bapi-recv-window'];
  assert.match(timestamp ?? '', /^\d+$/); assert.match(window ?? '', /^\d+$/);
  assert.equal(headers['x-bapi-api-key'], KEY);
  assert(Number(window) > 0 && Number(window) <= 60000);
  assert(Number(timestamp) >= Date.now() - Number(window) && Number(timestamp) < Date.now() + 1000, 'native signing timestamp lies inside its receive window');
  const payload = request.method === 'GET' ? url.search.slice(1) : request.body;
  const expected = createHmac('sha256', SECRET).update(timestamp + KEY + window + payload).digest('hex');
  assert.equal(headers['x-bapi-sign'], expected, `raw ${request.method} ${url.pathname} signature`);
}
function response(request) {
  const url = new URL(request.url), category = url.searchParams.get('category') ?? 'linear';
  const allowed = {
    '/v5/market/instruments-info': ['category', 'status', 'baseCoin', 'limit', 'cursor'],
    '/v5/market/option-base-coins': [], '/v5/market/tickers': ['category', 'baseCoin', 'symbol'],
    '/v5/account/info': [], '/v5/user/query-api': [], '/v5/account/fee-rate': ['category'],
    '/v5/account/wallet-balance': ['accountType'], '/v5/asset/coin-greeks': [],
    '/v5/position/list': ['category', 'settleCoin', 'limit', 'cursor', 'symbol'],
    '/v5/order/realtime': ['category', 'settleCoin', 'limit', 'cursor', 'symbol', 'orderId', 'orderLinkId', 'openOnly', 'orderFilter'],
    '/v5/order/create': [],
  }[url.pathname];
  assert(allowed, `Unscripted native path ${url.pathname}`);
  for (const key of url.searchParams.keys()) {
    if (key === 'requestId' && request.implementation === 'ts') {
      // The production TS interceptor signs its nanoid cache-buster too.
      assert.match(url.searchParams.get(key), /^[A-Za-z0-9_-]{21}$/);
    } else assert(allowed.includes(key), `Unexpected native query key ${key}`);
  }
  if (allowed.includes('category')) {
    assert(url.searchParams.has('category'), 'native category is required');
    assert(['linear', 'inverse', 'option'].includes(category));
  }
  if (url.pathname === '/v5/market/instruments-info') {
    const baseCoin = url.searchParams.get('baseCoin');
    if (category === 'option') assert(['All', 'BTC', 'ETH'].includes(baseCoin), 'option discovery needs All or a listed native underlying');
    else assert(baseCoin === null || baseCoin === 'BTC');
    const rows = CATALOG[category].filter((row) => baseCoin === null || baseCoin === 'All' || row.baseCoin === baseCoin);
    return ok(url.searchParams.get('status') === 'PreLaunch' ? [] : rows, category);
  }
  if (url.pathname === '/v5/market/option-base-coins') return ok([{ baseCoin: 'BTC' }, { baseCoin: 'ETH' }]);
  if (url.pathname === '/v5/market/tickers') {
    const baseCoin = url.searchParams.get('baseCoin'), symbol = url.searchParams.get('symbol');
    if (category === 'option') assert(baseCoin || symbol, 'option quotes need a native underlying or symbol');
    if (baseCoin !== null) assert(CATALOG[category].some((row) => row.baseCoin === baseCoin), 'unknown option underlying');
    if (symbol !== null) assert(CATALOG[category].some((row) => row.symbol === symbol), 'unknown native quote symbol');
    const rows = CATALOG[category].filter((row) => (baseCoin === null || row.baseCoin === baseCoin) && (symbol === null || row.symbol === symbol));
    assert(rows.length > 0, 'native baseCoin/symbol must select matching rows');
    return ok(rows.map((row) => quote(row.symbol)), category);
  }
  if (url.pathname === '/v5/account/info') return { retCode: 0, retMsg: 'OK', result: { marginMode: 'REGULAR_MARGIN', unifiedMarginStatus: 6 } };
  if (url.pathname === '/v5/user/query-api') return { retCode: 0, retMsg: 'OK', result: { readOnly: 0, unified: 1, uta: 1 } };
  if (url.pathname === '/v5/account/fee-rate') return ok([]);
  if (url.pathname === '/v5/account/wallet-balance') return ok([{ accountType: 'UNIFIED', totalEquity: '1000', totalInitialMargin: '0', totalAvailableBalance: '1000', totalPerpUPL: '0', totalWalletBalance: '1000', coin: [] }]);
  if (url.pathname === '/v5/position/list' || url.pathname === '/v5/order/realtime') return ok([], category);
  if (url.pathname === '/v5/asset/coin-greeks') return ok(GREEKS);
  if (url.pathname === '/v5/order/create') {
    const body = JSON.parse(request.body);
    const allowed = ['category', 'symbol', 'side', 'orderType', 'qty', 'price', 'orderIv', 'timeInForce', 'reduceOnly', 'orderLinkId', 'positionIdx', 'closeOnTrigger'];
    for (const key of Object.keys(body)) assert(allowed.includes(key), `Unexpected native order key ${key}`);
    assert(['linear', 'inverse', 'option'].includes(body.category));
    assert(CATALOG[body.category].some((row) => row.symbol === body.symbol));
    assert.equal(typeof body.qty, 'string'); assert(Number(body.qty) > 0);
    assert.equal(typeof body.orderLinkId, 'string'); assert(body.orderLinkId.length > 0);
    if (body.category === 'option' && body.orderIv !== undefined) assert(!Object.hasOwn(body, 'price'), 'native IV orders cannot also price by limit');
    return { retCode: 0, retMsg: 'OK', result: { orderId: `accepted-${body.orderLinkId}`, orderLinkId: body.orderLinkId } };
  }
  assert.fail(`Unscripted native API ${request.method} ${url}`);
}
function market(m) {
  return { id: m.id, symbol: m.symbol, base: m.base, quote: m.quote, active: m.active,
    contractSize: m.contractSize ?? null, isInverse: Boolean(m.isInverse), isExpiring: Boolean(m.isExpiring),
    precision: m.precision, amount: m.limits.amount, ...(m.option ? { option: m.option } : {}) };
}
function chain(rows) { return rows.map((row) => ({ market: market(row.market), quote: row.quote })); }
class Native {
  constructor(options) {
    this.now = Date.now(); this.resources = 0; this.requests = []; this.sockets = []; this.sent = []; this.results = new Map(); this.queue = [];
    this.engine = new WasmEngine('bybit', JSON.stringify({ key: KEY, secret: SECRET, ...options }));
    this.collect(this.engine.start(this.now)); this.run();
  }
  collect(json) { if (json) this.queue.push(...JSON.parse(json)); }
  run() {
    for (let count = 0; this.queue.length; count++) {
      assert(count < 500, 'native finite action sequence');
      const a = this.queue.shift();
      // Managed requests reach the fake venue only after the engine's own claim.
      if (a.op === 'managedHttp') {
        if (a.phase === 'prepare') {
          this.collect(this.engine.httpPrepared(a.preparation, String(++this.resources), undefined, this.now));
        } else if (a.phase === 'bind') {
          const claim = JSON.parse(this.engine.httpClaim(a.permit, this.now));
          if (claim.status === 'released') this.queue.unshift({ ...a.packet, op: 'http' });
          this.collect(this.engine.prepareDispatch(this.now));
        }
        continue;
      }
      if (a.op === 'http') {
        validate(a); this.requests.push(a); const body = JSON.stringify(response(a));
        this.collect(this.engine.httpStart(a.request, 200, '', this.now));
        this.collect(this.engine.httpChunk(a.request, new TextEncoder().encode(body), this.now));
        this.collect(this.engine.httpEnd(a.request, this.now));
      } else if (a.op === 'wsConnect') {
        this.sockets.push(a); this.collect(this.engine.wsOpen(a.socket, this.now));
        if (a.url.endsWith('/private')) {
          this.collect(this.engine.wsMessage(a.socket, '{"op":"auth","success":true}', this.now));
          this.collect(this.engine.wsMessage(a.socket, '{"op":"subscribe","success":true}', this.now));
        }
      } else if (a.op === 'wsSend') this.sent.push(a);
      else if (a.op === 'done') this.results.set(a.command, a);
    }
  }
  command(command) {
    this.now = Date.now();
    const id = 100 + this.results.size;
    this.collect(this.engine.command(JSON.stringify({ command: id, ...command }), this.now)); this.run();
    const result = this.results.get(id); assert(result, `native result for ${JSON.stringify(command)}`);
    if (result.error) throw new Error(result.error);
    return JSON.parse(result.result);
  }
  store() {
    const bytes = this.engine.snapshotFrame(this.now).slice();
    const store = new DefaultStore(); new FrameApplier(store).apply(new Frame(bytes.buffer), false); return store;
  }
  free() { this.engine.free(); }
}
async function ts(options) {
  const store = new DefaultStore(), requests = [];
  const exchange = new BybitExchange('bybit', { key: KEY, secret: SECRET, ...options }, store);
  let fixtureError;
  exchange.xhr.defaults.adapter = async (config) => {
    try {
    const url = exchange.xhr.getUri(config);
    const body = typeof config.data === 'string' ? config.data : config.data == null ? undefined : JSON.stringify(config.data);
    const request = { implementation: 'ts', method: config.method.toUpperCase(), url: url.startsWith('http') ? url : 'https://api.bybit.com' + url, body, headers: config.headers.toJSON?.() ?? config.headers };
    validate(request); requests.push(request);
    return { status: 200, statusText: 'OK', headers: {}, config, data: response(request) };
    } catch (error) { fixtureError = error; throw error; }
  };
  let markets;
  try { markets = await exchange.fetchMarkets(); } catch (error) { throw fixtureError ?? error; }
  store.update({ markets });
  return { exchange, store, requests };
}
test('Bybit independent raw fixture validator rejects signature, clock, economic and category alias mutations', () => {
  const sign = (body, timestamp = Date.now()) => {
    const request = { method: 'POST', url: 'https://api.bybit.com/v5/order/create', body: JSON.stringify(body), headers: {
      'X-BAPI-API-KEY': KEY, 'X-BAPI-TIMESTAMP': String(timestamp), 'X-BAPI-RECV-WINDOW': '5000',
    } };
    request.headers['X-BAPI-SIGN'] = createHmac('sha256', SECRET).update(String(timestamp) + KEY + '5000' + request.body).digest('hex');
    return request;
  };
  const body = { category: 'option', symbol: CALL, side: 'Buy', orderType: 'Limit', qty: '0.02', price: '1000', orderLinkId: 'mutation-proof' };
  const valid = sign(body); assert.doesNotThrow(() => { validate(valid); response(valid); });
  for (const changed of [
    { ...valid, headers: { ...valid.headers, 'X-BAPI-SIGN': '00'.repeat(32) } },
    { ...valid, body: JSON.stringify({ ...body, qty: '0.03' }) },
    { ...valid, body: JSON.stringify({ ...body, category: 'linear' }) },
    sign(body, Date.now() - 6000), sign(body, Date.now() + 2000),
  ]) assert.throws(() => validate(changed));
  assert.throws(() => response(sign({ ...body, category: 'linear' })));
  assert.throws(() => response(sign({ ...body, product: 'option' })), /Unexpected native order key/);
  assert.throws(() => response({ method: 'GET', url: 'https://api.bybit.com/v5/market/instruments-info?product=option' }), /Unexpected native query key/);
  assert.throws(() => response({ method: 'GET', url: 'https://api.bybit.com/v5/market/instruments-info?category=wrong' }));
});
test('Bybit native fixture respects All, underlying and exact symbol selectors across BTC and ETH', () => {
  const get = (path) => response({ method: 'GET', url: 'https://api.bybit.com' + path }).result.list;
  const base = '/v5/market/instruments-info?category=option&baseCoin=';
  assert.equal(get(base + 'All').length, 3);
  assert.deepEqual(get(base + 'BTC').map((row) => row.symbol), [CALL, PUT]);
  assert.deepEqual(get(base + 'ETH').map((row) => row.symbol), [ETH_PUT]);
  assert.deepEqual(get('/v5/market/tickers?category=option&baseCoin=ETH').map((row) => row.symbol), [ETH_PUT]);
  assert.deepEqual(get('/v5/market/tickers?category=option&symbol=' + PUT).map((row) => row.symbol), [PUT]);
  for (const path of [base + 'USDT', '/v5/market/tickers?category=option', '/v5/market/tickers?category=option&baseCoin=USDT',
    '/v5/market/tickers?category=option&symbol=UNKNOWN', '/v5/market/tickers?category=option&baseCoin=ETH&symbol=' + CALL])
    assert.throws(() => get(path));
});
for (const inverse of [false, true]) for (const options of [false, true]) {
  test(`actual Bybit TS/native combined products inverse=${inverse} options=${options}`, async () => {
    const profile = { enableInverseMarkets: inverse, enableOptionMarkets: options };
    const native = new Native(profile), reference = await ts(profile);
    try {
      const nativeStore = native.store();
      assert.deepEqual(nativeStore.markets.map(market).sort((a, b) => a.id.localeCompare(b.id)), reference.store.markets.map(market).sort((a, b) => a.id.localeCompare(b.id)));
      assert.equal(nativeStore.markets.length, 1 + Number(inverse) + 3 * Number(options));
      for (const value of [native.requests, reference.requests]) {
        const discovered = new Set(value.filter((r) => new URL(r.url).pathname === '/v5/market/instruments-info').map((r) => new URL(r.url).searchParams.get('category')));
        assert.deepEqual([...discovered].sort(), ['linear', ...(inverse ? ['inverse'] : []), ...(options ? ['option'] : [])].sort());
      }
      await reference.exchange.fetchOrders(); await reference.exchange.fetchPositions();
      for (const value of [native.requests, reference.requests]) {
        const categories = new Set(value.filter((r) => new URL(r.url).pathname === '/v5/order/realtime').map((r) => new URL(r.url).searchParams.get('category')));
        assert.deepEqual([...categories].sort(), ['linear', ...(inverse ? ['inverse'] : []), ...(options ? ['option'] : [])].sort());
      }
      if (!options) {
        assert.throws(() => native.command({ op: 'fetchOptionChain', query: { underlying: 'BTC' } }), /Options are not enabled/);
        await assert.rejects(reference.exchange.fetchOptionChain({ underlying: 'BTC' }), /Options are not enabled/);
        return;
      }
      const query = { underlying: 'BTC', expiry: EXPIRY, right: 'call', strikeMin: 99999, strikeMax: 100001 };
      assert.deepEqual(chain(native.command({ op: 'fetchOptionChain', query })), chain(await reference.exchange.fetchOptionChain(query)));
      assert.deepEqual(native.command({ op: 'fetchOptionExpiries', underlying: 'BTC' }), await reference.exchange.fetchOptionExpiries('BTC'));
      assert.deepEqual(native.command({ op: 'fetchAccountGreeks' }), await reference.exchange.fetchAccountGreeks());
      assert.deepEqual(native.command({ op: 'fetchOptionExpiries', underlying: 'ETH' }), await reference.exchange.fetchOptionExpiries('ETH'));
      assert.deepEqual(native.command({ op: 'fetchOptionExpiries', underlying: 'SOL' }), []);
      assert.deepEqual(chain(native.command({ op: 'fetchOptionChain', query: { underlying: 'ETH', right: 'put' } })), chain(await reference.exchange.fetchOptionChain({ underlying: 'ETH', right: 'put' })));
      assert(nativeStore.marketsBySymbol[ETH_PUT] && nativeStore.tickersBySymbol[ETH_PUT].mark > 0, 'discovery and quote seeding include a second listed underlying');
      const optionSocket = native.sockets.find((socket) => socket.url.endsWith('/public/option'));
      assert(optionSocket, 'Rust explicitly connects native option public feed');
      assert(nativeStore.tickersBySymbol[CALL].mark > 0, 'Rust seeds actual option quotes');
      native.collect(native.engine.command(JSON.stringify({ op: 'subscribeOrderBook', symbol: CALL }), native.now)); native.run();
      assert(native.sent.some((a) => a.socket === optionSocket.socket && a.text.includes(`orderbook.25.${CALL}`)));
      native.collect(native.engine.wsMessage(optionSocket.socket, JSON.stringify({ topic: `orderbook.25.${CALL}`, type: 'snapshot',
        data: { s: CALL, b: [['1000', '2']], a: [['1010', '3']], u: 1, seq: 1 }, ts: native.now }), native.now)); native.run();
      const bookFrame = native.engine.snapshotFrame(native.now).slice();
      const book = [...new Frame(bookFrame.buffer).books()].find((row) => row.market === nativeStore.markets.findIndex((m) => m.id === CALL));
      assert(book); assert.deepEqual([...book.bids.slice(0, 2)], [1000, 2]); assert.deepEqual([...book.asks.slice(0, 2)], [1010, 3]);
      const genericTsTickers = await reference.exchange.fetchTickers();
      assert(!genericTsTickers.some((row) => row.symbol === CALL), 'TS generic ticker API still omits option seeds; parity scope remains explicit');
      // TS has no generic option ticker/socket parity; dedicated native book data
      // remains visible and is tested independently without fabricating TS rows.
      const payloads = [];
      for (const [index, iv] of [undefined, 0.45].entries()) {
        const order = { symbol: CALL, type: 'limit', side: 'buy', amount: 0.019, price: 1001, clientOrderId: `combined-option-${index}`,
          ...(iv === undefined ? {} : { impliedVolatility: iv }) };
        const expected = [`accepted-${order.clientOrderId}`];
        assert.deepEqual(native.command({ op: 'placeOrders', orders: [order] }), expected);
        assert.deepEqual(await reference.exchange.placeOrders([order]), expected);
        const written = [native.requests, reference.requests].map((rows) => JSON.parse(rows.filter((r) => new URL(r.url).pathname === '/v5/order/create').at(-1).body));
        assert.deepEqual(written[0], written[1]);
        assert.equal(written[0].category, 'option'); assert.equal(written[0].qty, '0.02'); assert.equal(written[0].orderLinkId, order.clientOrderId);
        if (iv === undefined) assert.equal(written[0].price, '1000'); else { assert.equal(written[0].orderIv, '0.45'); assert(!Object.hasOwn(written[0], 'price')); }
        payloads.push(written[0]);
      }
      const invalid = { symbol: CALL, type: 'limit', side: 'buy', amount: 0.02, price: 1000, stopLoss: 500 };
      const before = [native.requests.length, reference.requests.length];
      assert.throws(() => native.command({ op: 'placeOrders', orders: [{ ...invalid, symbol: 'BTCUSDT', stopLoss: undefined }, invalid] }));
      await assert.rejects(reference.exchange.placeOrders([{ ...invalid, symbol: 'BTCUSDT', stopLoss: undefined }, invalid]));
      assert.deepEqual([native.requests.length, reference.requests.length], before, 'invalid mixed batch writes zero');
      assert.equal(payloads.length, 2);
    } finally { native.free(); reference.exchange.dispose(); }
  });
}

test('native option position identity and Greeks survive frames without positionIdx and retire on close', () => {
  const native = new Native({ enableOptionMarkets: true });
  try {
    const socket = native.sockets.find((row) => row.url.endsWith('/private'));
    assert(socket);
    const row = { category: 'option', symbol: CALL, side: 'Buy', size: '0.2', avgPrice: '1000', positionValue: '200',
      unrealisedPnl: '3', delta: '0.4', gamma: '0.01', vega: '12', theta: '-8' };
    const push = (position) => {
      native.collect(native.engine.wsMessage(socket.socket, JSON.stringify({ topic: 'position', data: [position] }), native.now));
      native.run();
    };
    push({ ...row, delta: undefined, gamma: undefined, vega: undefined, theta: undefined });
    const beforeGreeks = native.store().positions.find((position) => position.symbol === CALL);
    assert(beforeGreeks);
    push(row);
    const position = native.store().positions.find((position) => position.symbol === CALL);
    assert(position);
    assert.equal(position.id, `${CALL}:long`);
    assert.equal(position.id, beforeGreeks.id, 'receiving the first Greeks cannot change an existing position identity');
    assert.deepEqual(position.option?.greeks, { delta: 0.4, gamma: 0.01, vega: 12, theta: -8, units: 'Black-Scholes per 1 unit of the underlying, in the premium asset: vega per 1 vol point, theta per calendar day' });
    push({ ...row, size: '0' });
    assert.equal(native.store().positions.length, 0);
    const bytes = native.engine.snapshotFrame(native.now).slice();
    assert.equal(new Frame(bytes.buffer).positionGreeks()?.count ?? 0, 0);
  } finally { native.free(); }
});
