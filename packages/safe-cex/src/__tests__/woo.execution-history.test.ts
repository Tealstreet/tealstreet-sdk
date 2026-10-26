import { describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from './reference/woo/woo.exchange';
import { ENDPOINTS } from './reference/woo/woo.types';
import { OrderSide } from '../types';

const createExchangeLike = () => {
  const store = {
    subscribe: vi.fn(),
    tickersBySymbol: {},
    marketsBySymbol: {},
    orders: [],
    positions: [],
  } as any;

  const exchange = new WOOXExchange(
    'woo' as any,
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
      applicationId: 'app-id',
    } as any,
    store,
  );

  const get = vi.fn();
  exchange.xhr = { get } as any;

  return { exchange, get };
};

describe('WOOXExchange execution history mapping', () => {
  it('fetches transaction history and maps executions', async () => {
    const { exchange, get } = createExchangeLike();

    get.mockResolvedValueOnce({
      data: {
        rows: [
          {
            symbol: 'PERP_BTC_USDT',
            orderId: 60780383217,
            executedPrice: '87700.5',
            executedQuantity: '0.1',
            fee: '0.0001',
            feeAsset: 'LTC',
            side: 'BUY',
            executedTimestamp: '1752055173.630',
          },
          {
            symbol: 'PERP_BTC_USDT',
            orderId: 60780383218,
            executedPrice: '87710.5',
            executedQuantity: '0.2',
            fee: '0.0002',
            feeAsset: 'LTC',
            side: 'SELL',
            executedTimestamp: '1752055174.000',
          },
        ],
      },
    });

    const executions = await exchange.fetchHistoricalFills({
      symbol: 'BTCUSDT',
      orderId: '60780383217',
      limit: 5000,
    });

    expect(get).toHaveBeenCalledTimes(1);
    expect(get).toHaveBeenCalledWith(ENDPOINTS.TRANSACTION_HISTORY, {
      params: { symbol: 'PERP_BTC_USDT', limit: 500 },
    });
    expect(executions).toEqual([
      {
        timestamp: 1752055173,
        symbol: 'BTCUSDT',
        orderId: '60780383217',
        price: 87700.5,
        qty: 0.1,
        side: OrderSide.Buy,
        commission: 0.0001,
        commissionAsset: 'LTC',
        positionSide: 'both',
      },
    ]);
  });

  it('normalizes time filters and sorts filtered executions', async () => {
    const { exchange, get } = createExchangeLike();

    get.mockResolvedValueOnce({
      data: {
        rows: [
          {
            symbol: 'PERP_ETH_USDT',
            orderId: 42,
            executedPrice: '3000',
            executedQuantity: '1',
            fee: '0.2',
            feeAsset: 'USDT',
            side: 'SELL',
            executedTimestamp: '1752055300.100',
          },
          {
            symbol: 'PERP_BTC_USDT',
            order_id: 41,
            executedPrice: '50000',
            executedQuantity: '0.01',
            total_fee: '0.1',
            fee_asset: 'USDT',
            side: 'BUY',
            executedTimestamp: '1752055200.200',
          },
          {
            symbol: 'PERP_BTC_USDT',
            orderId: 40,
            executedPrice: '49999',
            executedQuantity: '0.02',
            side: 'BUY',
            executedTimestamp: '1752055000.000',
          },
        ],
      },
    });

    const executions = await exchange.fetchHistoricalFills({
      from: 1_752_055_100_000,
      to: 1_752_055_400,
      limit: 10,
    });

    expect(get).toHaveBeenCalledWith(ENDPOINTS.TRANSACTION_HISTORY, {
      params: {
        limit: 10,
        startTime: 1_752_055_100_000,
        endTime: 1_752_055_400_000,
      },
    });
    expect(executions).toEqual([
      {
        timestamp: 1752055300,
        symbol: 'ETHUSDT',
        orderId: '42',
        price: 3000,
        qty: 1,
        side: OrderSide.Sell,
        commission: 0.2,
        commissionAsset: 'USDT',
        positionSide: 'both',
      },
      {
        timestamp: 1752055200,
        symbol: 'BTCUSDT',
        orderId: '41',
        price: 50000,
        qty: 0.01,
        side: OrderSide.Buy,
        commission: 0.1,
        commissionAsset: 'USDT',
        positionSide: 'both',
      },
    ]);
  });

  it('falls back to alternate symbol formats when first symbol query is empty', async () => {
    const { exchange, get } = createExchangeLike();

    get.mockResolvedValueOnce({ data: { rows: [] } });
    get.mockResolvedValueOnce({
      data: {
        rows: [
          {
            symbol: 'BTC_USDT_PERP',
            orderId: 99,
            executedPrice: '60000',
            executedQuantity: '0.005',
            side: 'BUY',
            executedTimestamp: '1752055400.100',
          },
        ],
      },
    });

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 100 });

    expect(get).toHaveBeenCalledTimes(2);
    expect(get.mock.calls[0][1]).toEqual({ params: { symbol: 'PERP_BTC_USDT', limit: 100 } });
    expect(get.mock.calls[1][1]).toEqual({ params: { symbol: 'BTC_USDT_PERP', limit: 100 } });
    expect(executions).toEqual([
      {
        timestamp: 1752055400,
        symbol: 'BTCUSDT',
        orderId: '99',
        price: 60000,
        qty: 0.005,
        side: OrderSide.Buy,
        positionSide: 'both',
      },
    ]);
  });

  it('maps transaction history to historical trades for refresh recovery', async () => {
    const { exchange, get } = createExchangeLike();

    get.mockResolvedValueOnce({
      data: {
        rows: [
          {
            symbol: 'PERP_BTC_USDT',
            orderId: 60780383217,
            executedPrice: '87700.5',
            executedQuantity: '0.1',
            fee: '0.0001',
            feeAsset: 'USDT',
            realizedPnl: '12.34',
            positionSide: 'LONG',
            side: 'BUY',
            executedTimestamp: '1752055173.630',
          },
        ],
      },
    });

    const trades = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    expect(get).toHaveBeenCalledWith(ENDPOINTS.TRANSACTION_HISTORY, {
      params: { symbol: 'PERP_BTC_USDT', limit: 500 },
    });
    expect(trades).toEqual([
      {
        symbol: 'BTCUSDT',
        orderId: '60780383217',
        price: 87700.5,
        qty: 0.1,
        realizedPnl: 12.34,
        positionSide: 'long',
        side: OrderSide.Buy,
        commission: 0.0001,
        commissionAsset: 'USDT',
        // `Execution.timestamp` is SECONDS; the removed shape carried ms.
        timestamp: 1752055173,
      },
    ]);
  });

  it('normalizes prefixed symbols when fetching executions', async () => {
    const { exchange, get } = createExchangeLike();

    get.mockResolvedValueOnce({
      data: {
        rows: [
          {
            symbol: 'PERP_BTC_USDT',
            orderId: 1,
            executedPrice: '61000',
            executedQuantity: '0.01',
            side: 'BUY',
            executedTimestamp: '1752055500.100',
          },
        ],
      },
    });

    const executions = await exchange.fetchHistoricalFills({ symbol: 'woo:BTCUSDT.P', limit: 50 });

    expect(get).toHaveBeenCalledWith(ENDPOINTS.TRANSACTION_HISTORY, {
      params: { symbol: 'PERP_BTC_USDT', limit: 50 },
    });
    expect(executions).toHaveLength(1);
    expect(executions[0].symbol).toBe('BTCUSDT');
  });

  it('falls back to legacy trade endpoints when v3 returns empty', async () => {
    const { exchange, get } = createExchangeLike();

    // v3 attempts: symbol candidates + unfiltered
    get.mockResolvedValueOnce({ data: { rows: [] } }); // PERP_BTC_USDT
    get.mockResolvedValueOnce({ data: { rows: [] } }); // BTC_USDT_PERP
    get.mockResolvedValueOnce({ data: { rows: [] } }); // BTC-USDT-PERP
    get.mockResolvedValueOnce({ data: { rows: [] } }); // BTCUSDT
    get.mockResolvedValueOnce({ data: { rows: [] } }); // unfiltered

    // legacy /v1/client/trades with first symbol candidate
    get.mockResolvedValueOnce({
      data: {
        rows: [
          {
            symbol: 'PERP_BTC_USDT',
            order_id: 123,
            average_executed_price: '62000',
            executed: '0.002',
            side: 'BUY',
            created_time: '1752055600.100',
            total_fee: '0.01',
            fee_asset: 'USDT',
          },
        ],
      },
    });

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT', limit: 10 });

    expect(executions).toEqual([
      {
        timestamp: 1752055600,
        symbol: 'BTCUSDT',
        orderId: '123',
        price: 62000,
        qty: 0.002,
        side: OrderSide.Buy,
        commission: 0.01,
        commissionAsset: 'USDT',
        positionSide: 'both',
      },
    ]);
    expect(get.mock.calls.some((call) => call[0] === '/v1/client/trades')).toBe(true);
  });

  it('falls back to historical trades mapping when executions are empty', async () => {
    const { exchange, get } = createExchangeLike();

    // fetchHistoricalFills -> transaction history returns empty for all attempts
    get.mockResolvedValue({ data: { rows: [] } });

    const trades = [
      {
        symbol: 'BTCUSDT',
        orderId: 'abc-1',
        price: 61500,
        qty: 0.01,
        realizedPnl: 1.23,
        positionSide: 'both' as const,
        side: OrderSide.Buy,
        commission: 0.1,
        commissionAsset: 'USDT',
        time: 1_752_055_800_123,
      },
    ];

    const spy = vi.spyOn(exchange as any, 'fetchHistoricalTradesInternal').mockResolvedValueOnce(trades as any);

    const executions = await exchange.fetchHistoricalFills({ symbol: 'BTCUSDT' });

    expect(spy).toHaveBeenCalledWith({
      symbol: 'BTCUSDT',
      orderId: undefined,
      from: undefined,
      to: undefined,
      limit: 500,
    });
    expect(executions).toEqual([
      {
        timestamp: 1752055800,
        symbol: 'BTCUSDT',
        orderId: 'abc-1',
        price: 61500,
        qty: 0.01,
        side: OrderSide.Buy,
        commission: 0.1,
        commissionAsset: 'USDT',
        positionSide: 'both',
        // Carried through from the HistoricalTrade fallback, which already parsed it.
        realizedPnl: 1.23,
      },
    ]);
  });
});
