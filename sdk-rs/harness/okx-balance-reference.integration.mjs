import { test } from 'node:test';
import assert from 'node:assert/strict';
import { OKXExchange } from '../../packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts';
import { DefaultStore } from '../../packages/safe-cex/src/store/store.base.ts';
import { venueOf } from './venues.mjs';
const linear = { id: 'BTC-USDT-SWAP', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true, precision: { amount: 1, price: 0.1 },
  limits: { amount: { min: 1, max: 10000 }, leverage: { min: 1, max: 100 } }, contractSize: 0.01, isInverse: false, info: { ctValCcy: 'BTC', settleCcy: 'USDT' } };
const inverse = { ...linear, id: 'BTC-USD-SWAP', symbol: 'BTCUSD', quote: 'USD', contractSize: 100, isInverse: true, info: { ctValCcy: 'USD', settleCcy: 'BTC' } };
const raw = (instId, overrides) => ({ instId, pos: '5', posSide: 'net', avgPx: '39000', markPx: '40000', lever: '10', upl: '50', mgnMode: 'cross', imr: '200', mmr: '20', ccy: 'USDT', ...overrides });
const amounts = (balance) => ({ used: balance.used, upnl: balance.upnl, total: balance.total, free: balance.free });
test('real OKX partial position/account handlers retain full IMR, coin valuation and live order reserve through next formatter tick', async () => {
  const store = new DefaultStore();
  const exchange = new OKXExchange('okx', { key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-passphrase' }, store);
  exchange._accountPositionMode = 'oneway'; exchange._accountLevel = 2;
  store.update({ markets: [linear, inverse], tickers: [linear, inverse].map((m) => ({ id: m.id, symbol: m.symbol, mark: 40000, last: 40000 })) });
  const initial = [raw(linear.id), raw(inverse.id, { ccy: 'BTC', imr: '0.01', mmr: '0.001', upl: '0.001' })];
  const orders = [{ id: '77', symbol: linear.symbol, status: 'open', type: 'limit', side: 'buy', amount: 1, remaining: 0.01, filled: 0, price: 40000,
    reduceOnly: false, info: { lever: '10', ccy: 'USDT' } }];
  store.update({ positions: exchange.mapPositions(initial), orders, balance: { used: 600, upnl: 90, total: 9910, free: 9356, info: [{ ccy: 'USD', eq: '10000' }] } });
  venueOf('okx').prepareTs(exchange);
  try {
    exchange.privateWebsocket.handlePositionTopic({ data: [raw(inverse.id, { ccy: 'BTC', imr: '0.012', mmr: '0.0012', upl: '0.002' })] });
    assert.equal(store.positions.length, 2);
    assert.deepEqual(amounts(store.balance), { used: 680, upnl: 130, total: 9870, free: 9276 });
    exchange.privateWebsocket.handleAccountTopic({ data: [{ totalEq: '12000' }] });
    assert.deepEqual(amounts(store.balance), { used: 680, upnl: 130, total: 11870, free: 11276 });
    const tick = await exchange.formatBalanceUsedAvail(store.balance.info, store.positions, store.orders);
    assert.deepEqual(amounts(tick), amounts(store.balance), 'old cached raw REST balance cannot replace newer totalEq at next tick');
    // A new array alone has no freshness provenance. Complete the production
    // REST read, whose captured account revision can retire the older WS anchor.
    const priorGet = exchange.xhr.get;
    const reads = [];
    const positionRows = store.positions.map((position) => position.info);
    exchange.xhr.get = async (url, config) => {
      reads.push({ url, instType: config?.params?.instType });
      if (url === '/api/v5/account/account-position-risk')
        return { data: { data: [{ balData: [{ ccy: 'USD', eq: '12050' }] }] } };
      if (url === '/api/v5/account/positions') return { data: { data: positionRows } };
      throw new Error(`Unexpected fresh REST read ${url}`);
    };
    let fresh;
    try { ({ balance: fresh } = await exchange.fetchBalanceAndOpenPositions()); }
    finally { exchange.xhr.get = priorGet; }
    assert.deepEqual(reads, [
      { url: '/api/v5/account/account-position-risk', instType: 'SWAP' },
      { url: '/api/v5/account/positions', instType: 'SWAP' },
    ]);
    assert.deepEqual(amounts(fresh), { used: 680, upnl: 130, total: 11920, free: 11326 }, 'a completed fresh REST snapshot retains its actual newer equity');
    store.update({ balance: fresh });
    exchange.privateWebsocket.handlePositionTopic({ data: [raw(inverse.id, { ccy: 'BTC', pos: '0', imr: '0', mmr: '0', upl: '0' })] });
    assert.deepEqual(amounts(store.balance), { used: 200, upnl: 50, total: 12000, free: 11806 }, 'flat inverse removes only its own risk');
  } finally { exchange.dispose(); }
});

test('real OKX untraded native empty last/open seed clears unavailable scalars without hiding malformed nonempty data', () => {
  const store = new DefaultStore(); store.update({ markets: [linear] });
  const exchange = new OKXExchange('okx', { publicDataOnly: true }, store);
  venueOf('okx').prepareTs(exchange);
  try {
    const raw = { instId: linear.id, last: '', open24h: '', bidPx: '9', askPx: '11', volCcy24h: '2', vol24h: '20' };
    const [ticker] = exchange.parseTickers([raw]);
    assert.deepEqual({ last: ticker.last, mark: ticker.mark, index: ticker.index, percentage: ticker.percentage }, { last: 0, mark: 0, index: 0, percentage: 0 });
    assert.deepEqual({ bid: ticker.bid, ask: ticker.ask, volume: ticker.volume, quoteVolume: ticker.quoteVolume }, { bid: 9, ask: 11, volume: 2, quoteVolume: 20 });
    assert.equal(raw.last, '', 'native metadata not mutated');
    store.update({ tickers: [ticker] });
    exchange.publicWebsocket.handleTickerEvents({ data: [raw] });
    assert.equal(store.tickersBySymbol[linear.symbol].last, 0, 'empty native stream quote stays unavailable after initial seed');
    exchange.publicWebsocket.handleTickerEvents({ data: [{ ...raw, last: '12', open24h: '10' }] });
    assert.equal(store.tickersBySymbol[linear.symbol].last, 12);
    assert.equal(store.tickersBySymbol[linear.symbol].percentage, 20);
    exchange.publicWebsocket.handleTickerEvents({ data: [raw] });
    assert.equal(store.tickersBySymbol[linear.symbol].last, 12, 'partial empty stream retains a proven last trade');
    assert.equal(store.tickersBySymbol[linear.symbol].percentage, 20);
    exchange.publicWebsocket.handleTickerEvents({ data: [{ ...raw, last: '13', open24h: '' }] });
    assert.equal(store.tickersBySymbol[linear.symbol].last, 13);
    assert.equal(store.tickersBySymbol[linear.symbol].percentage, 20, 'missing open cannot erase proven percentage');
    for (const last of ['bad', 'NaN', 'Infinity']) {
      const [bad] = exchange.parseTickers([{ ...raw, last }]);
      assert(!Number.isFinite(bad.last), 'malformed supplied quote remains a visible failure');
      exchange.publicWebsocket.handleTickerEvents({ data: [{ ...raw, last }] });
      assert(!Number.isFinite(store.tickersBySymbol[linear.symbol].last), 'malformed nonempty stream quote remains a visible failure');
    }
  } finally { exchange.dispose(); }
});

test('real OKX reference refuses malformed supplied IMR/PnL before handlers or formatter can coerce it to zero', async () => {
  const store = new DefaultStore();
  const exchange = new OKXExchange('okx', { key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-passphrase' }, store);
  exchange._accountPositionMode = 'oneway'; exchange._accountLevel = 2;
  store.update({ markets: [linear, inverse], tickers: [linear, inverse].map((market) => ({ id: market.id, symbol: market.symbol, mark: 40000, last: 40000 })) });
  const proven = exchange.mapPositions([raw(linear.id)]);
  const balance = { used: 200, upnl: 50, total: 9950, free: 9800, info: [{ ccy: 'USD', eq: '10000' }] };
  store.update({ positions: proven, balance });
  venueOf('okx').prepareTs(exchange);
  try {
    for (const field of ['imr', 'upl']) {
      for (const value of ['bad', 'NaN', 'Infinity', '-Infinity', '1bad', '0x1', ' ']) {
        const malformed = raw(linear.id, { [field]: value });
        const error = new RegExp(`Invalid OKX reference native risk ${linear.id}\\.${field}`);
        assert.throws(() => exchange.privateWebsocket.handlePositionTopic({ data: [malformed] }), error);
        assert.deepEqual(store.positions, proven, `${field}=${value} must not mutate proven positions`);
        assert.deepEqual(store.balance, balance, `${field}=${value} must not alter proven equity or risk`);
        const mapped = exchange.mapPositions([malformed]);
        await assert.rejects(exchange.formatBalanceUsedAvail(balance.info, mapped, []), error);
        assert.deepEqual(store.balance, balance);
        store.update({ positions: mapped });
        assert.throws(() => exchange.privateWebsocket.handleAccountTopic({ data: [{ totalEq: '12000' }] }), error);
        assert.deepEqual(store.balance, balance, 'an account push cannot conceal malformed retained position risk');
        store.update({ positions: proven });
      }
    }
    for (const field of ['imr', 'upl']) {
      const overflow = raw(inverse.id, { ccy: 'BTC', imr: '0.01', upl: '0.001', [field]: '1e308' });
      assert.throws(() => exchange.privateWebsocket.handlePositionTopic({ data: [overflow] }), /Invalid OKX reference converted risk/);
      assert.deepEqual(store.positions, proven, 'finite coin risk that overflows USD cannot replace proven state');
      assert.deepEqual(store.balance, balance);
    }
    assert.throws(() => exchange.privateWebsocket.handlePositionTopic({ data: [raw(linear.id, { imr: '-1' })] }), /Invalid OKX reference native risk/);
    assert.throws(() => exchange.privateWebsocket.handlePositionTopic({ data: [raw(linear.id, { imr: '', upl: '' })] }), /Invalid OKX reference native risk/);
    exchange.privateWebsocket.handlePositionTopic({ data: [raw(linear.id, { imr: '220', upl: '-10' })] });
    assert.deepEqual(amounts(store.balance), { used: 220, upnl: -10, total: 10010, free: 9780 }, 'a valid row still updates after rejected malformed risk');
    exchange.privateWebsocket.handlePositionTopic({ data: [raw(linear.id, { mgnMode: 'isolated', imr: '', margin: '230', upl: '0' })] });
    assert.deepEqual(amounts(store.balance), { used: 230, upnl: 0, total: 10000, free: 9770 }, 'isolated unavailable IMR uses only native reported locked margin');
    exchange.privateWebsocket.handlePositionTopic({ data: [raw(linear.id, { pos: '0', imr: '', upl: '' })] });
    assert.deepEqual(amounts(store.balance), { used: 0, upnl: 0, total: 10000, free: 10000 }, 'flat empty risk retires exposure without inventing an open-row value');
  } finally { exchange.dispose(); }
});
