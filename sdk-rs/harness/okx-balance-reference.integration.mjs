import { applyReferenceFixture } from '../../packages/safe-cex/src/__tests__/cutover/referenceFixture.ts';
import { nativeFixture } from '../../packages/safe-cex/src/__tests__/cutover/nativeFixture.ts';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { OKXExchange } from '../../packages/safe-cex/src/__tests__/reference/okx/okx.exchange.ts';
import { FrameStore } from '../../packages/safe-cex/src/store/frameStore.ts';
import { venueOf } from './venues.mjs';
const linear = { id: 'BTC-USDT-SWAP', symbol: 'BTCUSDT', base: 'BTC', quote: 'USDT', active: true, precision: { amount: 1, price: 0.1 },
  limits: { amount: { min: 1, max: 10000 }, leverage: { min: 1, max: 100 } }, contractSize: 0.01, isInverse: false, info: { ctValCcy: 'BTC', settleCcy: 'USDT' } };
const inverse = { ...linear, id: 'BTC-USD-SWAP', symbol: 'BTCUSD', quote: 'USD', contractSize: 100, isInverse: true, info: { ctValCcy: 'USD', settleCcy: 'BTC' } };
const raw = (instId, overrides) => ({ instId, pos: '5', posSide: 'net', avgPx: '39000', markPx: '40000', lever: '10', upl: '50', mgnMode: 'cross', imr: '200', mmr: '20', ccy: 'USDT', ...overrides });
const amounts = (balance) => ({ used: balance.used, upnl: balance.upnl, total: balance.total, free: balance.free });
test('real OKX untraded native empty last/open seed clears unavailable scalars without hiding malformed nonempty data', () => {
  const store = new FrameStore(); store.apply(nativeFixture({ markets: [linear] }, store.markets.map(row => row.symbol)));
  const exchange = new OKXExchange('okx', { publicDataOnly: true }, store);
  venueOf('okx').prepareTs(exchange, undefined, data => applyReferenceFixture(store, data));
  try {
    const raw = { instId: linear.id, last: '', open24h: '', bidPx: '9', askPx: '11', volCcy24h: '2', vol24h: '20' };
    const [ticker] = exchange.parseTickers([raw]);
    assert.deepEqual({ last: ticker.last, mark: ticker.mark, index: ticker.index, percentage: ticker.percentage }, { last: 0, mark: 0, index: 0, percentage: 0 });
    assert.deepEqual({ bid: ticker.bid, ask: ticker.ask, volume: ticker.volume, quoteVolume: ticker.quoteVolume }, { bid: 9, ask: 11, volume: 2, quoteVolume: 20 });
    assert.equal(raw.last, '', 'native metadata not mutated');
    store.apply(nativeFixture({ tickers: [ticker] }, store.markets.map(row => row.symbol)));
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

test('real OKX reference formatter refuses malformed native risk before it can become a frame', async () => {
  const store = new FrameStore();
  store.apply(nativeFixture({ markets: [linear, inverse], tickers: [linear, inverse].map((market) => ({
    id: market.id, symbol: market.symbol, bid: 40000, ask: 40000, mark: 40000, last: 40000,
    index: 40000, percentage: 0, openInterest: 0, fundingRate: 0, fundingTime: 0, volume: 0, quoteVolume: 0,
  })) }));
  const exchange = new OKXExchange('okx', { key: 'fixture-key', secret: 'fixture-secret', passphrase: 'fixture-passphrase' }, store);
  exchange._accountPositionMode = 'oneway'; exchange._accountLevel = 2;
  venueOf('okx').prepareTs(exchange, undefined, data => applyReferenceFixture(store, data));
  const equity = [{ ccy: 'USD', eq: '10000' }];
  try {
    for (const field of ['imr', 'upl']) {
      for (const value of ['bad', 'NaN', 'Infinity', '-Infinity', '1bad', '0x1', ' ']) {
        const mapped = exchange.mapPositions([raw(linear.id, { [field]: value })]);
        await assert.rejects(exchange.formatBalanceUsedAvail(equity, mapped, []),
          new RegExp(`Invalid OKX reference native risk ${linear.id}\\.${field}`));
      }
      const mapped = exchange.mapPositions([raw(inverse.id, { ccy: 'BTC', imr: '0.01', upl: '0.001', [field]: '1e308' })]);
      await assert.rejects(exchange.formatBalanceUsedAvail(equity, mapped, []), /Invalid OKX reference converted risk/);
    }
    const valid = exchange.mapPositions([raw(linear.id, { imr: '220', upl: '-10' })]);
    assert.deepEqual(amounts(await exchange.formatBalanceUsedAvail(equity, valid, [])),
      { used: 220, upnl: -10, total: 10010, free: 9780 });
    assert.deepEqual(store.positions, [], 'formatter validation does not publish partial native state');
  } finally { exchange.dispose(); }
});
