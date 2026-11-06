import { describe, expect, it } from 'vitest';

import { buildUserTradesParams, mapBinanceUserTrade } from '../reference/binance/binance.utils';

// Row captured verbatim from GET /fapi/v1/userTrades on a live account
// (2026-08). Note every numeric arrives as a decimal string.
const ROW = {
  symbol: 'DOGEUSDT',
  id: 3422500270,
  orderId: 101205954323,
  side: 'BUY',
  price: '0.069980',
  qty: '143',
  realizedPnl: '0',
  quoteQty: '10.007140',
  commission: '0.00500357',
  commissionAsset: 'USDT',
  time: 1786158465224,
  positionSide: 'LONG',
  buyer: true,
  maker: false,
};

describe('binance userTrades request options', () => {
  it('honours limit instead of always requesting 1000', () => {
    // Previously hardcoded: a caller asking for 5 rows got the last 1000.
    expect(buildUserTradesParams({ symbol: 'DOGEUSDT', limit: 5 }).limit).toBe(5);
    expect(buildUserTradesParams({ symbol: 'DOGEUSDT' }).limit).toBe(1000);
  });

  it('clamps limit to the endpoint maximum', () => {
    expect(buildUserTradesParams({ symbol: 'DOGEUSDT', limit: 99999 }).limit).toBe(1000);
    expect(buildUserTradesParams({ symbol: 'DOGEUSDT', limit: 0 }).limit).toBe(1);
  });

  it('forwards the time window', () => {
    const params = buildUserTradesParams({ symbol: 'DOGEUSDT', from: 1786158000000, to: 1786158999999 });
    expect(params.startTime).toBe(1786158000000);
    expect(params.endTime).toBe(1786158999999);
  });

  it('forwards orderId', () => {
    expect(buildUserTradesParams({ symbol: 'DOGEUSDT', orderId: '101205954323' }).orderId).toBe('101205954323');
    expect(buildUserTradesParams({ symbol: 'DOGEUSDT' }).orderId).toBeUndefined();
  });

  it('never sends fromId alongside a time window', () => {
    // Binance rejects the combination outright.
    const windowed = buildUserTradesParams({ symbol: 'DOGEUSDT', from: 1, fromId: 42 });
    expect(windowed.fromId).toBeUndefined();
    expect(buildUserTradesParams({ symbol: 'DOGEUSDT', fromId: 42 }).fromId).toBe(42);
  });
});

describe('binance userTrades row mapping', () => {
  it('returns quoteQty as a number, not the raw string', () => {
    // HistoricalTrade declares `quoteQty?: number`; this field was the one
    // numeric passed through without coercion, so arithmetic downstream
    // concatenated instead of adding.
    const mapped = mapBinanceUserTrade(ROW);
    expect(typeof mapped.quoteQty).toBe('number');
    expect(mapped.quoteQty).toBe(10.00714);
  });

  it('coerces every other numeric field', () => {
    const mapped = mapBinanceUserTrade(ROW);
    for (const key of ['price', 'qty', 'realizedPnl', 'commission', 'time'] as const) {
      expect(typeof mapped[key]).toBe('number');
    }
  });

  it('leaves quoteQty absent rather than emitting NaN', () => {
    expect(mapBinanceUserTrade({ ...ROW, quoteQty: undefined }).quoteQty).toBeUndefined();
    expect(mapBinanceUserTrade({ ...ROW, quoteQty: null }).quoteQty).toBeUndefined();
  });

  it('preserves the per-fill id distinctly from orderId', () => {
    const mapped = mapBinanceUserTrade(ROW);
    expect(mapped.id).toBe('3422500270');
    expect(mapped.orderId).toBe('101205954323');
  });

  it('lowercases side and positionSide without throwing on a missing field', () => {
    expect(mapBinanceUserTrade(ROW).positionSide).toBe('long');
    expect(mapBinanceUserTrade(ROW).side).toBe('buy');
    // The old mapper called .toLocaleLowerCase() unguarded and threw here.
    expect(() => mapBinanceUserTrade({ ...ROW, positionSide: undefined })).not.toThrow();
    expect(mapBinanceUserTrade({ ...ROW, positionSide: undefined }).positionSide).toBe('both');
  });
});
