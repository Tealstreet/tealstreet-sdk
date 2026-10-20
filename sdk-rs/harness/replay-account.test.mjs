import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { NONE, ORDER_COLUMNS, POSITION_COLUMNS, orderEnums, positionEnums } from '../hosts/wasm/js/wire.mjs';

const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');

test('Rust account comparison excludes scalar placeholders and retains independent leverage rows', () => {
  const accountSource = source.slice(source.indexOf('function rustAccount('), source.indexOf('\nasync function runTs()'));
  const account = (venue, settings) => vm.runInNewContext(`${accountSource}\nrustAccount`, {
    venue, values: { venue: venue.rust }, NONE,
  })({
    markets: { count: 7, symbol: (index) => `MARKET${index}` },
    balance: { used: 1, free: 9, total: 10, upnl: 0 },
    settings, fills: [],
  });
  const settings = {
    count: 7,
    market: [0, 1, 2, 3, 4, 5, 6],
    leverage: [0, NaN, -Infinity, undefined, -2, 5, 20],
    marginType: (index) => index % 2 ? 'isolated' : 'cross',
  };
  assert.deepEqual(structuredClone(account({ rust: 'breakoutprop' }, settings).settings), [
    { symbol: 'MARKET5', marginType: 'isolated', leverage: 5 },
    { symbol: 'MARKET6', marginType: 'cross', leverage: 20 },
  ]);
  assert.deepEqual(structuredClone(account({ rust: 'mexc', independentLeverage: true }, {
    count: 2, market: [0, 1], leverage: [0, NaN],
    buyLeverage: [5, 10], sellLeverage: [15, 20], marginType: () => 'cross',
  }).settings), [
    { symbol: 'MARKET0', marginType: 'cross', buyLeverage: 5, sellLeverage: 15 },
    { symbol: 'MARKET1', marginType: 'cross', buyLeverage: 10, sellLeverage: 20 },
  ]);
});

const accountReader = () => {
  const start = source.indexOf('function rustAccount(');
  const end = source.indexOf('\nasync function runTs()', start);
  assert.ok(start >= 0 && end > start);
  return vm.runInNewContext(`${source.slice(start, end)}\nrustAccount`, {
    venue: { rust: 'grvt' }, values: { venue: 'grvt' },
    NONE, ORDER_COLUMNS, POSITION_COLUMNS, orderEnums, positionEnums,
  });
};
const balance = { used: 1, free: 9, total: 10, upnl: 0 };
const table = (columns, market, symbols) => ({
  count: market.length, market, enums: market.map(() => 0),
  id: (index) => `row-${index}`, symbol: (index) => symbols[index],
  ...Object.fromEntries(columns.map((column) => [column, market.map(() => 1)])),
});

test('Rust account keeps canonical market symbols and stable identities for orphan table rows', () => {
  const lookups = [];
  const markets = { count: 1, symbol(index) {
    lookups.push(index);
    assert.equal(index, 0, 'an invalid native row must never reach the catalogue accessor');
    return 'LIVE';
  } };
  const indices = [0, NONE, 9];
  const symbols = ['ignored-tail-for-live-row', 'DELISTED', 'OUTSIDE-CATALOGUE'];
  const result = accountReader()({ markets, balance, fills: [],
    orders: table(ORDER_COLUMNS, indices, symbols),
    positions: table(POSITION_COLUMNS, indices, symbols),
    settings: { count: 3, market: indices, leverage: [5, 10, 20],
      symbol: (index) => symbols[index], marginType: () => 'cross' },
  });
  for (const rows of [result.orders, result.positions, result.settings])
    assert.deepEqual(Array.from(rows, (row) => row.symbol), ['LIVE', 'DELISTED', 'OUTSIDE-CATALOGUE']);
  assert.deepEqual(lookups, [0, 0, 0]);
  assert.equal(result.orders[1].id, 'row-1');
  assert.equal(result.positions[2].contracts, 1);
  assert.equal(result.settings[2].leverage, 20);
});

test('Rust account rejects unresolved invalid market indices without inventing an identity', () => {
  const lookups = [];
  const markets = { count: 1, symbol: (index) => { lookups.push(index); return 'wrong'; } };
  for (const index of [NONE, 1, -1, 0.5, NaN]) {
    assert.throws(() => accountReader()({ markets, balance, fills: [],
      orders: table(ORDER_COLUMNS, [index], [undefined]),
    }), /missing stable market identity/i);
  }
  assert.deepEqual(lookups, []);
});

test('actual frame fill capture preserves arrival symbols after market reorder and delisting', () => {
  const start = source.indexOf('      const executed = frame.fills();');
  const end = source.indexOf('\n    },', start);
  assert.ok(start >= 0 && end > start, 'exercise the real frame callback fill capture');
  const fills = [];
  const context = vm.createContext({ fills, venue: {}, NONE,
    markets: { count: 2, symbol: (index) => ['A', 'B'][index] },
  });
  const capture = vm.runInContext(`(frame) => {${source.slice(start, end)}\n}`, context);
  capture({ fills: () => ({ count: 3, market: [0, 0, NONE], side: [0, 1, 0],
    rows: [10, 2, 11, 3, 12, 4], orderId: (index) => `fill-${index}`,
    symbol: (index) => ['A', undefined, 'DELISTED'][index],
  }) });
  context.markets = { count: 2, symbol: (index) => ['B', 'A'][index] };
  const result = accountReader()({ markets: context.markets, balance, fills });
  assert.deepEqual(structuredClone(result.fills), [
    { id: 'fill-0', symbol: 'A', side: 'buy', amount: 2 },
    { id: 'fill-1', symbol: 'A', side: 'sell', amount: 3 },
    { id: 'fill-2', symbol: 'DELISTED', side: 'buy', amount: 4 },
  ]);
  assert.deepEqual(fills.map((fill) => fill.market), [0, 0, NONE], 'native indices remain available for legacy reads');
});

test('ordinary TS venue fetch always uses the tape and preserves Request and response metadata', async () => {
  const fetchSource = source.slice(source.indexOf('  globalThis.WebSocket = TapeWebSocket;'),
    source.indexOf('  // Header-capable Node sockets'));
  const calls = [];
  let networkCalls = 0;
  const context = {
    venue: { rust: 'grvt' }, values: { venue: 'grvt' }, TapeWebSocket: class {}, Request, Response, Headers,
    globalThis: { fetch: () => { networkCalls++; throw new Error('network must remain unused'); } },
    tape: { respond: (...args) => {
      calls.push(args);
      return calls.length === 1
        ? { status: 201, body: '{"from":"tape"}', headers: 'content-type: application/json\nx-replay: exact:with:colons' }
        : { status: 204, body: 'discarded for a bodyless status', headers: 'x-replay: empty' };
    } },
  };
  vm.runInNewContext(fetchSource, context);
  const request = new Request('https://edge.grvt.io/full/v1/account?scope=positions', {
    method: 'POST', body: '{"account":"caller"}',
    headers: { 'Content-Type': 'application/json', 'X-Auth': 'fixture-signature' },
  });
  const response = await context.globalThis.fetch(request);
  assert.equal(response.status, 201);
  assert.equal(response.headers.get('x-replay'), 'exact:with:colons');
  assert.equal(response.headers.get('content-type'), 'application/json');
  assert.deepEqual(await response.json(), { from: 'tape' });
  assert.equal(request.bodyUsed, false, 'reading a Request for replay must clone its body');
  const empty = await context.globalThis.fetch(request, {
    method: 'PUT', body: 'override-body', headers: { 'X-Override': 'explicit' },
  });
  assert.equal(empty.status, 204);
  assert.equal(empty.body, null);
  assert.equal(empty.headers.get('x-replay'), 'empty');
  assert.equal(await empty.text(), '');
  assert.deepEqual(structuredClone(calls), [
    ['POST', request.url, '{"account":"caller"}', {
      'content-type': 'application/json', 'x-auth': 'fixture-signature',
    }],
    ['PUT', request.url, 'override-body', { 'x-override': 'explicit' }],
  ]);
  assert.equal(networkCalls, 0);
});


test('actual market projection preserves omitted, empty and Unicode pretty symbols', () => {
  const start = source.indexOf('const marketFields = ');
  const end = source.indexOf('\nasync function runRust()', start);
  assert.ok(start >= 0 && end > start);
  const project = vm.runInNewContext(`${source.slice(start, end)}\nmarketFields`, { venue: {} });
  const market = { id: 'native', symbol: 'BTCUSD', base: 'BTC', quote: 'USD', active: true,
    precision: { amount: 0.001, price: 0.1 }, limits: { amount: { min: 0.001, max: 100 } },
  };
  for (const prettySymbol of [undefined, '', '比特/美元 🪙']) {
    const actual = structuredClone(project({ ...market, prettySymbol }));
    assert.deepEqual(actual, { ...market, isInverse: false, isSpot: false, isExpiring: false,
      contractSize: null, ...(prettySymbol !== undefined ? { prettySymbol } : {}),
    });
    assert.equal(Object.hasOwn(actual, 'prettySymbol'), prettySymbol !== undefined);
  }
});

test('actual ticker capture retains finite and zero midpoint and omits only its NaN sentinel', () => {
  const start = source.indexOf('      const tickerRows = [];');
  const end = source.indexOf('      const bookRows = {};', start);
  assert.ok(start >= 0 && end > start);
  const rows = vm.runInNewContext(`(() => {${source.slice(start, end)}\nreturn tickerRows; })()`, {
    NONE, markets: { symbol: (index) => ['LIVE0', 'LIVE1'][index] },
    tickers: { count: 3, market: [0, NONE, 1], identity: () => ({ symbol: 'DELISTED' }),
      columns: { bid: [100, 0, 300], midPrice: [101.25, 0, NaN], volume: [0, 10, 20], index: [NaN, 2, 3] },
    },
  });
  assert.deepEqual(structuredClone(rows), [
    { symbol: 'LIVE0', bid: 100, midPrice: 101.25, volume: 0, index: NaN },
    { symbol: 'DELISTED', bid: 0, midPrice: 0, volume: 10, index: 2 },
    { symbol: 'LIVE1', bid: 300, volume: 20, index: 3 },
  ]);
  assert.equal(Object.hasOwn(rows[2], 'midPrice'), false);
  assert.equal(Object.hasOwn(rows[0], 'index'), true, 'other numeric fields keep their original NaN representation');
});
