import { describe, expect, it } from 'vitest';

import {
  buildOptionChain,
  BYBIT_GREEK_UNITS,
  mapBybitCoinGreeks,
  mapBybitOptionInstrument,
  mapBybitOptionTicker,
  optionExpiries,
} from './reference/bybit/bybit.options';

// Recorded from api.bybit.com on 2026-10-14.
const CALL = {
  symbolId: 386790,
  symbol: 'BTC-6NOV26-84000-C-USDT',
  status: 'Trading',
  baseCoin: 'BTC',
  quoteCoin: 'USDT',
  settleCoin: 'USDT',
  optionsType: 'Call',
  launchTime: '1788508800000',
  deliveryTime: '1793952000000',
  deliveryFeeRate: '0.00015',
  priceFilter: { minPrice: '5', maxPrice: '1110000', tickSize: '5' },
  lotSizeFilter: { maxOrderQty: '500', minOrderQty: '0.01', qtyStep: '0.01' },
  displayName: 'BTCUSDT-6NOV26-84000-C',
};

const CALL_TICKER = {
  symbol: 'BTC-6NOV26-84000-C-USDT',
  bid1Price: '2865',
  bid1Iv: '0.3464',
  ask1Price: '2895',
  ask1Iv: '0.3499',
  markPrice: '2896.98093401',
  indexPrice: '83535.24461644',
  markIv: '0.3498',
  underlyingPrice: '83893.59726138',
  delta: '0.51221163',
  gamma: '0.00005401',
  vega: '84.16776555',
  theta: '-63.71913125',
};

const put = { ...CALL, symbol: 'BTC-6NOV26-84000-P-USDT', optionsType: 'Put' };
const later = { ...CALL, symbol: 'BTC-27NOV26-90000-C-USDT', deliveryTime: '1795766400000' };

describe('mapBybitOptionInstrument', () => {
  it('reads strike, right, expiry, premium asset and the price floor', () => {
    const market = mapBybitOptionInstrument(CALL)!;
    expect(market.option).toEqual({
      underlying: 'BTC',
      expiry: 1793952000000,
      strike: 84000,
      right: 'call',
      style: 'european',
      settlement: 'cash',
      premiumAsset: 'USDT',
      settleAsset: 'USDT',
    });
    expect(market.precision).toEqual({ amount: 0.01, price: 5 });
    expect(market.limits.price).toEqual({ min: 5 });
    expect(market.limits.amount).toEqual({ min: 0.01, max: 500 });
  });

  it('reads a decimal strike written with d', () => {
    expect(mapBybitOptionInstrument({ ...CALL, symbol: 'SOL-6NOV26-112d5-C-USDT' })!.option!.strike).toBe(112.5);
  });

  it.each(['PreLaunch', 'Delivering'])('leaves out a %s instrument', (status) => {
    expect(mapBybitOptionInstrument({ ...CALL, status })).toBeUndefined();
  });
});

describe('mapBybitOptionTicker', () => {
  it('keeps prices, IVs and greeks with their measured units', () => {
    expect(mapBybitOptionTicker(CALL_TICKER)).toEqual({
      symbol: CALL_TICKER.symbol,
      bid: 2865,
      ask: 2895,
      mark: 2896.98093401,
      markIv: 0.3498,
      bidIv: 0.3464,
      askIv: 0.3499,
      underlyingPrice: 83893.59726138,
      greeks: {
        delta: 0.51221163,
        gamma: 0.00005401,
        vega: 84.16776555,
        theta: -63.71913125,
        units: BYBIT_GREEK_UNITS,
      },
    });
  });

  it('omits greeks when Bybit sends none', () => {
    expect(mapBybitOptionTicker({ ...CALL_TICKER, delta: '' }).greeks).toBeUndefined();
  });
});

describe('buildOptionChain', () => {
  const markets = [later, put, CALL].map((raw) => mapBybitOptionInstrument(raw)!);
  const quotes = [later, put, CALL].map((raw) => mapBybitOptionTicker({ ...CALL_TICKER, symbol: raw.symbol }));

  it('sorts by expiry, then strike, calls first', () => {
    const symbols = buildOptionChain(markets, quotes, { underlying: 'BTC' }).map((r) => r.market.symbol);
    expect(symbols).toEqual([CALL.symbol, put.symbol, later.symbol]);
  });

  it('filters by expiry, right and strike range', () => {
    const rows = buildOptionChain(markets, quotes, {
      underlying: 'BTC',
      expiry: 1793952000000,
      right: 'put',
      strikeMin: 80000,
      strikeMax: 85000,
    });
    expect(rows.map((r) => r.market.symbol)).toEqual([put.symbol]);
  });

  it('drops a market with no quote', () => {
    expect(buildOptionChain(markets, quotes.slice(1), { underlying: 'BTC' })).toHaveLength(2);
  });

  it('lists distinct expiries in order', () => {
    expect(optionExpiries(markets, 'BTC')).toEqual([1793952000000, 1795766400000]);
  });
});

describe('mapBybitCoinGreeks', () => {
  it('maps the account totals for one coin', () => {
    expect(
      mapBybitCoinGreeks({
        baseCoin: 'BTC',
        totalDelta: '-0.1',
        totalGamma: '0.0001',
        totalVega: '12',
        totalTheta: '-3',
      }),
    ).toEqual({ underlying: 'BTC', delta: -0.1, gamma: 0.0001, vega: 12, theta: -3, units: BYBIT_GREEK_UNITS });
  });
});
