import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from './reference/bybit/bybit.exchange';
import { BybitDemoExchange } from './reference/bybit/bybitdemo.exchange';
import { BybitInverseExchange } from './reference/bybit/bybitinverse.exchange';
import { OrderSide } from '../types';

type TestExchange = BybitExchange | BybitDemoExchange | BybitInverseExchange;
const BYBIT_MAX_QUERY_RANGE_MS = 7 * 24 * 60 * 60 * 1000 - 1;

const createStore = () =>
  ({
    subscribe: vi.fn(),
    markets: [],
    marketsBySymbol: {},
    tickersBySymbol: {},
    orders: [],
    positions: [],
  }) as any;

const createLinearExchange = () => {
  const exchange = new BybitExchange(
    'bybit' as any,
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
    } as any,
    createStore(),
  );
  const get = vi.fn();
  exchange.xhr = { get } as any;
  return { exchange, get };
};

const createInverseExchange = () => {
  const exchange = new BybitInverseExchange(
    'bybitinverse' as any,
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
    } as any,
    createStore(),
  );
  const get = vi.fn();
  exchange.xhr = { get } as any;
  return { exchange, get };
};

const createDemoExchange = () => {
  const exchange = new BybitDemoExchange(
    'bybitdemo' as any,
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
    } as any,
    createStore(),
  );
  const get = vi.fn();
  exchange.xhr = { get } as any;
  return { exchange, get };
};

const mockExecutionsPayload = {
  retMsg: 'OK',
  result: {
    list: [
      {
        execType: 'TRADE',
        execTime: '1700000001000',
        symbol: 'BTCUSDT',
        orderId: 'oid-trade',
        execPrice: '50000',
        execQty: '0.2',
        side: 'buy',
        execFee: '0.1',
        feeCurrency: 'USDT',
      },
      {
        execType: 'Funding',
        execTime: '1700000002000',
        symbol: 'BTCUSDT',
        orderId: 'oid-funding',
        execPrice: '50000',
        execQty: '0.2',
        side: 'Sell',
        execFee: '0',
        feeCurrency: 'USDT',
      },
      {
        execType: 'SETTLE',
        execTime: '1700000003000',
        symbol: 'BTCUSDT',
        orderId: 'oid-settle',
        execPrice: '50000',
        execQty: '0.2',
        side: 'Sell',
        execFee: '0',
        feeCurrency: 'USDT',
      },
    ],
  },
};

const assertTradeOnly = async (exchange: TestExchange, get: ReturnType<typeof vi.fn>, symbol: string) => {
  get.mockResolvedValueOnce({ data: mockExecutionsPayload });

  const executions = await exchange.fetchHistoricalFills({ symbol, limit: 50 });

  expect(executions).toEqual([
    {
      timestamp: 1700000001,
      symbol: 'BTCUSDT',
      orderId: 'oid-trade',
      price: 50000,
      qty: 0.2,
      side: OrderSide.Buy,
      commission: 0.1,
      commissionAsset: 'USDT',
    },
  ]);
};

const assertOrderHistoryFallback = async (exchange: TestExchange, get: ReturnType<typeof vi.fn>, symbol: string) => {
  let orderHistoryCallCount = 0;
  get.mockImplementation((url: string) => {
    if (url === '/v5/execution/list') {
      return Promise.resolve({
        data: {
          retMsg: 'OK',
          result: {
            list: [],
          },
        },
      });
    }

    if (url === '/v5/order/history') {
      orderHistoryCallCount += 1;
      return Promise.resolve({
        data: {
          retMsg: 'OK',
          result: {
            list:
              orderHistoryCallCount === 1
                ? [
                    {
                      symbol: 'BTCUSDT',
                      orderId: 'oid-history',
                      orderStatus: 'Filled',
                      side: 'Sell',
                      avgPrice: '51000',
                      cumExecQty: '0.25',
                      updatedTime: '1700000100000',
                      cumExecFee: '0.05',
                      feeCurrency: 'USDT',
                    },
                  ]
                : [],
          },
        },
      });
    }

    return Promise.reject(new Error(`Unexpected URL ${url}`));
  });

  const executions = await exchange.fetchHistoricalFills({ symbol, limit: 50 });

  expect(executions).toEqual([
    {
      timestamp: 1700000100,
      symbol: 'BTCUSDT',
      orderId: 'oid-history',
      price: 51000,
      qty: 0.25,
      side: OrderSide.Sell,
      commission: 0.05,
      commissionAsset: 'USDT',
    },
  ]);

  expect(get.mock.calls[0][0]).toBe('/v5/execution/list');
  expect(get.mock.calls.some((call) => call[0] === '/v5/order/history')).toBe(true);
};

const assertAlternatePrimaryExecutionShape = async (
  exchange: TestExchange,
  get: ReturnType<typeof vi.fn>,
  symbol: string,
) => {
  get.mockResolvedValueOnce({
    data: {
      retMsg: 'OK',
      result: {
        list: [
          {
            execId: 'exec-alt',
            tradeTime: '1700000200000',
            symbol,
            orderId: 'oid-alt',
            avgPrice: '50025',
            qty: '0.5',
            orderSide: 'Sell',
            fee: '-0.07',
            feeCoin: 'USD',
            isMaker: true,
          },
        ],
      },
    },
  });

  const executions = await exchange.fetchHistoricalFills({ symbol, limit: 50 });

  expect(executions).toEqual([
    {
      id: 'exec-alt',
      timestamp: 1700000200,
      symbol,
      orderId: 'oid-alt',
      price: 50025,
      qty: 0.5,
      side: OrderSide.Sell,
      commission: 0.07,
      commissionAsset: 'USD',
      maker: true,
    },
  ]);
};

const assertMicrosecondPrimaryExecutionTimestamp = async (
  exchange: TestExchange,
  get: ReturnType<typeof vi.fn>,
  symbol: string,
) => {
  get.mockResolvedValueOnce({
    data: {
      retMsg: 'OK',
      result: {
        list: [
          {
            execId: 'exec-micro',
            execTime: '1793096306496430',
            symbol,
            orderId: 'oid-micro',
            execPrice: '50000',
            execQty: '0.1',
            side: 'Buy',
            execFee: '0.01',
            feeCurrency: 'USDT',
          },
        ],
      },
    },
  });

  const executions = await exchange.fetchHistoricalFills({ symbol, limit: 50 });

  expect(executions).toEqual([
    {
      id: 'exec-micro',
      timestamp: 1793096306,
      symbol,
      orderId: 'oid-micro',
      price: 50000,
      qty: 0.1,
      side: OrderSide.Buy,
      commission: 0.01,
      commissionAsset: 'USDT',
    },
  ]);
};

const assertMicrosecondOrderHistoryTimestamp = async (
  exchange: TestExchange,
  get: ReturnType<typeof vi.fn>,
  symbol: string,
) => {
  get.mockImplementation((url: string) => {
    if (url === '/v5/execution/list') {
      return Promise.resolve({
        data: {
          retMsg: 'OK',
          result: {
            list: [],
          },
        },
      });
    }

    if (url === '/v5/order/history') {
      return Promise.resolve({
        data: {
          retMsg: 'OK',
          result: {
            list: [
              {
                symbol,
                orderId: 'oid-history-micro',
                orderStatus: 'Filled',
                side: 'Sell',
                avgPrice: '50100',
                cumExecQty: '0.2',
                updatedTime: '1793096306496430',
                cumExecFee: '0.02',
                feeCurrency: 'USDT',
              },
            ],
          },
        },
      });
    }

    return Promise.reject(new Error(`Unexpected URL ${url}`));
  });

  // An explicit window around the fixture: with the default range the result
  // depended on today's date once the fixture (2026-10-27) fell inside it.
  const fixtureMs = 1793096306496;
  const executions = await exchange.fetchHistoricalFills({
    symbol,
    limit: 50,
    from: fixtureMs - 60 * 60 * 1000,
    to: fixtureMs + 60 * 60 * 1000,
  });

  expect(executions).toEqual([
    {
      timestamp: 1793096306,
      symbol,
      orderId: 'oid-history-micro',
      price: 50100,
      qty: 0.2,
      side: OrderSide.Sell,
      commission: 0.02,
      commissionAsset: 'USDT',
    },
  ]);
};

const assertOrderHistoryFallbackAfterPrimaryError = async (
  exchange: TestExchange,
  get: ReturnType<typeof vi.fn>,
  symbol: string,
) => {
  get.mockImplementation((url: string) => {
    if (url === '/v5/execution/list') {
      return Promise.reject(new Error('execution endpoint unavailable'));
    }

    if (url === '/v5/order/history') {
      return Promise.resolve({
        data: {
          retMsg: 'OK',
          result: {
            list: [
              {
                symbol,
                orderId: 'oid-history-after-error',
                orderStatus: 'Filled',
                side: 'Buy',
                avgPrice: '49950',
                cumExecQty: '0.3',
                updatedTime: '1700000300000',
                cumExecFee: '0.02',
                feeCurrency: 'USD',
              },
            ],
          },
        },
      });
    }

    return Promise.reject(new Error(`Unexpected URL ${url}`));
  });

  const executions = await exchange.fetchHistoricalFills({
    symbol,
    from: 1700000000000,
    to: 1700000600000,
    limit: 50,
  });

  expect(executions).toEqual([
    {
      timestamp: 1700000300,
      symbol,
      orderId: 'oid-history-after-error',
      price: 49950,
      qty: 0.3,
      side: OrderSide.Buy,
      commission: 0.02,
      commissionAsset: 'USD',
    },
  ]);
};

const assertChunkedRangeRequests = async (exchange: TestExchange, get: ReturnType<typeof vi.fn>, symbol: string) => {
  const to = 1_800_000_000_000;
  const from = to - 30 * 24 * 60 * 60 * 1000;

  get.mockImplementation((url: string) => {
    if (url === '/v5/execution/list') {
      return Promise.resolve({
        data: {
          retMsg: 'OK',
          result: {
            list: [],
          },
        },
      });
    }

    if (url === '/v5/order/history') {
      return Promise.resolve({
        data: {
          retMsg: 'OK',
          result: {
            list: [
              {
                symbol,
                orderId: 'oid-windowed',
                orderStatus: 'Filled',
                side: 'Buy',
                avgPrice: '50001',
                cumExecQty: '0.1',
                updatedTime: `${to - 1000}`,
                cumExecFee: '0.01',
                feeCurrency: 'USDT',
              },
            ],
          },
        },
      });
    }

    return Promise.reject(new Error(`Unexpected URL ${url}`));
  });

  const executions = await exchange.fetchHistoricalFills({ symbol, from, to, limit: 1 });
  expect(executions.length).toBe(1);

  const rangedCalls = get.mock.calls.filter((call) => {
    const url = call[0];
    return url === '/v5/execution/list' || url === '/v5/order/history';
  });
  expect(rangedCalls.length).toBeGreaterThan(1);

  for (const call of rangedCalls) {
    const params = call[1]?.params || {};
    expect(typeof params.startTime).toBe('number');
    expect(typeof params.endTime).toBe('number');
    expect(params.endTime - params.startTime).toBeLessThanOrEqual(BYBIT_MAX_QUERY_RANGE_MS);
  }
};

describe('Bybit execution history mapping', () => {
  it('filters non-trade executions for linear bybit', async () => {
    const { exchange, get } = createLinearExchange();
    await assertTradeOnly(exchange, get, 'BTCUSDT');
  });

  it('filters non-trade executions for inverse bybit', async () => {
    const { exchange, get } = createInverseExchange();
    await assertTradeOnly(exchange, get, 'BTCUSDT');
  });

  it('falls back to order history for linear bybit when execution list is empty', async () => {
    const { exchange, get } = createLinearExchange();
    await assertOrderHistoryFallback(exchange, get, 'BTCUSDT');
  });

  it('falls back to order history for inverse bybit when execution list is empty', async () => {
    const { exchange, get } = createInverseExchange();
    await assertOrderHistoryFallback(exchange, get, 'BTCUSDT');
  });

  it('maps alternate primary execution fields for linear bybit', async () => {
    const { exchange, get } = createLinearExchange();
    await assertAlternatePrimaryExecutionShape(exchange, get, 'BTCUSDT');
  });

  it('maps alternate primary execution fields for inverse bybit', async () => {
    const { exchange, get } = createInverseExchange();
    await assertAlternatePrimaryExecutionShape(exchange, get, 'BTCUSD');
  });

  it('maps microsecond primary execution timestamps for linear bybit', async () => {
    const { exchange, get } = createLinearExchange();
    await assertMicrosecondPrimaryExecutionTimestamp(exchange, get, 'BTCUSDT');
  });

  it('maps microsecond primary execution timestamps for bybit demo', async () => {
    const { exchange, get } = createDemoExchange();
    await assertMicrosecondPrimaryExecutionTimestamp(exchange, get, 'BTCUSDT');
  });

  it('maps microsecond primary execution timestamps for inverse bybit', async () => {
    const { exchange, get } = createInverseExchange();
    await assertMicrosecondPrimaryExecutionTimestamp(exchange, get, 'BTCUSD');
  });

  it('maps microsecond order-history fallback timestamps for linear bybit', async () => {
    const { exchange, get } = createLinearExchange();
    await assertMicrosecondOrderHistoryTimestamp(exchange, get, 'BTCUSDT');
  });

  it('maps microsecond order-history fallback timestamps for inverse bybit', async () => {
    const { exchange, get } = createInverseExchange();
    await assertMicrosecondOrderHistoryTimestamp(exchange, get, 'BTCUSD');
  });

  it('falls back to order history for inverse bybit when execution list errors', async () => {
    const { exchange, get } = createInverseExchange();
    await assertOrderHistoryFallbackAfterPrimaryError(exchange, get, 'BTCUSD');
  });

  it('splits linear bybit execution/history range requests into <=7 day windows', async () => {
    const { exchange, get } = createLinearExchange();
    await assertChunkedRangeRequests(exchange, get, 'BTCUSDT');
  });

  it('splits inverse bybit execution/history range requests into <=7 day windows', async () => {
    const { exchange, get } = createInverseExchange();
    await assertChunkedRangeRequests(exchange, get, 'BTCUSD');
  });
});
