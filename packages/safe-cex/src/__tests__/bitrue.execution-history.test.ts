import { describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from './reference/bitrue/bitrue.exchange';
import { ENDPOINTS } from './reference/bitrue/bitrue.types';
import { OrderSide } from '../types';

const createExchangeLike = () => {
  const store = {
    subscribe: vi.fn(),
    tickersBySymbol: {},
    marketsBySymbol: {
      'E-BTC-USDT': {
        symbol: 'E-BTC-USDT',
        info: { multiplier: 2 },
      },
      'E-ETH-USDT': {
        symbol: 'E-ETH-USDT',
        info: { multiplier: 1 },
      },
    },
    orders: [],
    positions: [],
  } as any;

  const exchange = new BitrueExchange(
    'bitrue' as any,
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
    } as any,
    store,
  );

  const get = vi.fn();
  exchange.xhr = { get } as any;
  return { exchange, get };
};

describe('BitrueExchange execution history mapping', () => {
  it('maps and filters my trades rows to historical trades', async () => {
    const { exchange, get } = createExchangeLike();

    get.mockResolvedValueOnce({
      data: {
        code: '0',
        data: [
          {
            tradeId: 1001,
            contractName: 'E_BTCUSDT',
            side: 'BUY',
            price: '50000',
            qty: '1',
            amount: '100000',
            fee: '-0.5',
            bidId: 111,
            ctime: 1_700_000_005_000,
            isBuyer: true,
            isMaker: false,
          },
          {
            tradeId: 1002,
            contractName: 'E_BTCUSDT',
            side: 'SELL',
            price: '51000',
            qty: '0.5',
            amount: '25500',
            fee: '0.2',
            askId: 222,
            ctime: 1_700_000_007_000,
            isBuyer: false,
            isMaker: true,
          },
          {
            tradeId: 1003,
            contractName: 'E_ETHUSDT',
            side: 'BUY',
            price: '3000',
            qty: '1',
            amount: '3000',
            bidId: 333,
            ctime: 1_700_000_006_000,
          },
        ],
      },
    });

    const trades = await exchange.fetchHistoricalFills({
      symbol: 'E-BTC-USDT',
      from: 1_700_000_004,
      to: 1_700_000_008,
      limit: 10,
    });

    expect(get).toHaveBeenCalledWith(ENDPOINTS.MY_TRADES, {
      params: {
        contractName: 'E-BTC-USDT',
        limit: 10,
        startTime: 1_700_000_004_000,
        endTime: 1_700_000_008_000,
      },
    });

    expect(trades).toHaveLength(2);
    expect(trades[0]).toMatchObject({
      symbol: 'E-BTC-USDT',
      orderId: '222',
      price: 51000,
      qty: 0.5,
      side: OrderSide.Sell,
      maker: true,
      commission: 0.2,
      commissionAsset: 'USDT',
      positionSide: 'both',
      // `Execution.timestamp` is SECONDS. The removed `HistoricalTrade` shape
      // carried the same instant in milliseconds.
      timestamp: 1_700_000_007,
    });
    expect(trades[1]).toMatchObject({
      symbol: 'E-BTC-USDT',
      orderId: '111',
      // qty is scaled by multiplier (2) to match quote amount/price
      qty: 2,
      side: OrderSide.Buy,
      commission: 0.5,
      commissionAsset: 'USDT',
      timestamp: 1_700_000_005,
    });
  });

  it('maps historical trades into executions and applies orderId filter', async () => {
    const { exchange, get } = createExchangeLike();

    get.mockResolvedValueOnce({
      data: {
        code: 0,
        data: [
          {
            tradeId: 1004,
            contractName: 'E_BTCUSDT',
            side: 'BUY',
            price: '60000',
            qty: '0.25',
            amount: '15000',
            fee: '0.1',
            bidId: '11111',
            ctime: 1_700_000_009_123,
          },
          {
            tradeId: 1005,
            contractName: 'E_BTCUSDT',
            side: 'BUY',
            price: '61000',
            qty: '0.25',
            amount: '15250',
            fee: '0.2',
            bidId: '22222',
            ctime: 1_700_000_010_123,
          },
        ],
      },
    });

    const executions = await exchange.fetchHistoricalFills({
      symbol: 'E-BTC-USDT',
      orderId: '11111',
      limit: 100,
    });

    expect(executions).toEqual([
      {
        id: '1004',
        timestamp: 1_700_000_009,
        symbol: 'E-BTC-USDT',
        orderId: '11111',
        price: 60000,
        qty: 0.25,
        side: OrderSide.Buy,
        // Carried across the re-map now. bitrue reports no per-fill hedge leg,
        // so the inner mapper emits the one-way default.
        positionSide: 'both',
        commission: 0.1,
        commissionAsset: 'USDT',
      },
    ]);
  });

  it('rejects and logs when my trades API responds with error code', async () => {
    const { exchange, get } = createExchangeLike();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});

    get.mockResolvedValueOnce({
      data: {
        code: '1001',
        msg: 'request failed',
        data: [],
      },
    });

    await expect(exchange.fetchHistoricalFills({ symbol: 'E-BTC-USDT' })).rejects.toThrow('request failed');
    expect(emitAndLogError).toHaveBeenCalledWith('request failed');
  });
});
