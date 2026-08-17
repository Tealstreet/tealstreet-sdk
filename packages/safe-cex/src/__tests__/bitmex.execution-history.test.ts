import { describe, expect, it, vi } from 'vitest';

import { BitmexExchange } from '../exchanges/bitmex/bitmex.exchange';
import { OrderSide } from '../types';

const createExchangeLike = () => {
  const store = {
    subscribe: vi.fn(),
    tickersBySymbol: {
      XBTUSDT: { last: 50000 },
    },
    marketsBySymbol: {
      XBTUSD: {
        symbol: 'XBTUSD',
        isInverse: true,
        info: { settlCurrency: 'XBt' },
      },
      XBTUSDT: {
        symbol: 'XBTUSDT',
        isInverse: false,
        info: { settlCurrency: 'USDt' },
      },
    },
  } as any;

  const exchange = new BitmexExchange(
    'bitmex' as any,
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
    } as any,
    store,
  );

  exchange.convertOutputsFromMicro = false;
  (exchange as any).currencies = {
    XBt: { precision: 0.00000001, majorCurrency: 'XBT' },
    USDt: { precision: 0.000001, majorCurrency: 'USDT' },
    USD: { precision: 1, majorCurrency: 'USD' },
  };

  const get = vi.fn();
  exchange.xhr = { get } as any;
  return { exchange, get };
};

describe('BitmexExchange execution history mapping', () => {
  it('falls back to user execution endpoint and maps executions', async () => {
    const { exchange, get } = createExchangeLike();
    const transactTime = '2026-03-20T00:00:10.000Z';

    get.mockRejectedValueOnce(new Error('Signature not valid'));
    get.mockResolvedValueOnce({
      data: [
        {
          execType: 'Trade',
          symbol: 'XBTUSD',
          orderID: 'oid-1',
          lastQty: 100,
          lastPx: 50000,
          side: 'Buy',
          transactTime,
          execComm: -150,
          settlCurrency: 'XBt',
        },
        {
          execType: 'Funding',
          symbol: 'XBTUSD',
          orderID: 'oid-2',
          lastQty: 100,
          lastPx: 50000,
          side: 'Sell',
          transactTime,
        },
      ],
    });

    const executions = await exchange.fetchHistoricalFills({
      symbol: 'XBTUSD',
      orderId: 'oid-1',
      limit: 200,
    });

    expect(get).toHaveBeenCalledTimes(2);
    expect(get.mock.calls[0][0]).toContain('/execution/tradeHistory?');
    expect(get.mock.calls[1][0]).toContain('/user/executionHistory?');
    expect(executions).toHaveLength(1);
    expect(executions[0]).toEqual({
      timestamp: Math.floor(Date.parse(transactTime) / 1000),
      symbol: 'XBTUSD',
      orderId: 'oid-1',
      price: 50000,
      qty: 100,
      side: OrderSide.Buy,
      // BitMEX reports no per-fill hedge leg, so the mapper emits the one-way
      // default rather than leaving it absent — `undefined` would read as "not
      // reported" to a consumer that can distinguish, and several can.
      positionSide: 'both',
      commission: 0.075,
      commissionAsset: 'USD',
    });
  });

  it('maps historical trades with realized pnl and commission', async () => {
    const { exchange, get } = createExchangeLike();
    const time = Date.parse('2026-03-20T00:00:20.000Z');

    get.mockResolvedValueOnce({
      data: [
        {
          execType: 'Trade',
          symbol: 'XBTUSDT',
          execID: 'exec-1',
          lastQty: 0.5,
          lastPx: 62000,
          side: 'Sell',
          timestamp: new Date(time).toISOString(),
          execComm: -1250,
          currency: 'USDt',
          realisedPnl: 250000,
        },
      ],
    });

    const historicalTrades = await exchange.fetchHistoricalFills({
      symbol: 'XBTUSDT',
      limit: 50,
    });

    expect(historicalTrades).toHaveLength(1);
    expect(historicalTrades[0]).toMatchObject({
      symbol: 'XBTUSDT',
      // The venue reported no orderID on this row. The removed mapper fell back
      // to putting the EXECUTION id here, which made a fill id look like the id
      // of the order it belonged to. It now lands in `id`, where a per-fill
      // dedupe key belongs, and `orderId` stays absent — which is the honest
      // answer and the only reason bitmex fills are now storable at all.
      id: 'exec-1',
      price: 62000,
      qty: 0.5,
      side: OrderSide.Sell,
      positionSide: 'both',
      commissionAsset: 'USD',
    });
    // `Execution.timestamp` is SECONDS; the removed HistoricalTrade shape was ms.
    expect(historicalTrades[0].timestamp).toBe(Math.floor(time / 1000));
    expect(historicalTrades[0].commission).toBeCloseTo(0.00125, 8);
    expect(historicalTrades[0].realizedPnl).toBeCloseTo(0.25, 8);
  });

  it('normalizes uppercase XBT settlement for inverse realized pnl', async () => {
    const { exchange, get } = createExchangeLike();
    const time = Date.parse('2026-03-20T00:00:30.000Z');

    get.mockResolvedValueOnce({
      data: [
        {
          execType: 'Trade',
          symbol: 'XBTUSD',
          execID: 'exec-uppercase-xbt',
          lastQty: 100,
          lastPx: 50000,
          side: 'Buy',
          timestamp: new Date(time).toISOString(),
          execComm: -150,
          settlCurrency: 'XBT',
          realisedPnl: 2000,
        },
      ],
    });

    const historicalTrades = await exchange.fetchHistoricalFills({
      symbol: 'XBTUSD',
      limit: 50,
    });

    expect(historicalTrades).toHaveLength(1);
    expect(historicalTrades[0].commission).toBeCloseTo(0.075, 8);
    expect(historicalTrades[0].realizedPnl).toBeCloseTo(1, 8);
  });

  it('prefers market settlement currency over execution currency for linear contracts', async () => {
    const { exchange, get } = createExchangeLike();
    const time = Date.parse('2026-03-20T00:01:00.000Z');

    get.mockResolvedValueOnce({
      data: [
        {
          execType: 'Trade',
          symbol: 'XBTUSDT',
          execID: 'exec-market-settle-currency',
          lastQty: 0.0002,
          lastPx: 66342.3,
          side: 'Sell',
          timestamp: new Date(time).toISOString(),
          // These values are micro USDt; using "USD" precision would inflate by 10,000x.
          execComm: -1193800,
          realisedPnl: -1733800,
          currency: 'USD',
          settlCurrency: 'USD',
        },
      ],
    });

    const historicalTrades = await exchange.fetchHistoricalFills({
      symbol: 'XBTUSDT',
      limit: 50,
    });

    expect(historicalTrades).toHaveLength(1);
    expect(historicalTrades[0].commission).toBeCloseTo(1.1938, 8);
    expect(historicalTrades[0].realizedPnl).toBeCloseTo(-1.7338, 8);
  });

  it('falls back to symbol-based settlement inference when payload currencies are USD-like', async () => {
    const { exchange, get } = createExchangeLike();
    const time = Date.parse('2026-03-20T00:02:00.000Z');

    (exchange.store as any).marketsBySymbol = {};
    get.mockResolvedValueOnce({
      data: [
        {
          execType: 'Trade',
          symbol: 'XBTUSD',
          execID: 'exec-symbol-fallback',
          lastQty: 100,
          lastPx: 50000,
          side: 'Buy',
          timestamp: new Date(time).toISOString(),
          execComm: -150,
          realisedPnl: 2000,
          currency: 'USD',
          settlCurrency: 'USD',
        },
      ],
    });

    const historicalTrades = await exchange.fetchHistoricalFills({
      symbol: 'XBTUSD',
      limit: 50,
    });

    expect(historicalTrades).toHaveLength(1);
    expect(historicalTrades[0].realizedPnl).toBeCloseTo(1, 8);
    expect(historicalTrades[0].commission).toBeCloseTo(0.075, 8);
  });

  it('uses built-in precision fallbacks when wallet assets metadata is unavailable', async () => {
    const { exchange, get } = createExchangeLike();
    const time = Date.parse('2026-03-20T00:03:00.000Z');

    (exchange as any).currencies = {};
    get.mockResolvedValueOnce({
      data: [
        {
          execType: 'Trade',
          symbol: 'XBTUSDT',
          execID: 'exec-fallback-precision',
          lastQty: 0.0002,
          lastPx: 66342.3,
          side: 'Sell',
          timestamp: new Date(time).toISOString(),
          execComm: -1193800,
          realisedPnl: -1733800,
          currency: 'USD',
          settlCurrency: 'USD',
        },
      ],
    });

    const historicalTrades = await exchange.fetchHistoricalFills({
      symbol: 'XBTUSDT',
      limit: 50,
    });

    expect(historicalTrades).toHaveLength(1);
    expect(historicalTrades[0].commission).toBeCloseTo(1.1938, 8);
    expect(historicalTrades[0].realizedPnl).toBeCloseTo(-1.7338, 8);
  });
});
