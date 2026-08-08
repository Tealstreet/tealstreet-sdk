import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../../exchanges/bybit/bybit.exchange';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { BybitInverseExchange } from '../../exchanges/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../../exchanges/bybit/bybitspot.exchange';
import { OrderSide, OrderType, PositionMode } from '../../types';

const createStore = (symbol: string, quote: string, max = 100, isInverse = false) => {
  const market = {
    id: symbol,
    symbol,
    base: 'BTC',
    quote,
    active: true,
    precision: { amount: isInverse ? 1 : 0.001, price: 0.1 },
    limits: { amount: { min: isInverse ? 1 : 0.001, max }, leverage: { min: 1, max: 100 } },
    isInverse,
  };
  return {
    subscribe: vi.fn(),
    markets: [market],
    marketsBySymbol: { [symbol]: market },
    tickersBySymbol: {},
    orders: [],
    positions: [],
  } as any;
};

const mockPositionMode = (exchange: BybitExchange | BybitInverseExchange) => {
  (exchange as any).fetchPositionMode = vi.fn().mockResolvedValue(PositionMode.OneWay);
};

const limitOrder = (symbol = 'BTCUSDT', amount = 1, overrides = {}) => ({
  symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount,
  price: 50_000,
  ...overrides,
});

describe('Bybit regular-order clientOrderId', () => {
  it('uses caller clientOrderId as orderLinkId for linear, inverse, and spot single native regular orders', async () => {
    const variants = [
      {
        exchange: new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT')),
        order: limitOrder('BTCUSDT', 1, { clientOrderId: 'bybit-linear-client-1' }),
      },
      {
        exchange: new BybitInverseExchange(
          'bybitinverse',
          { key: 'key', secret: 'secret' },
          createStore('BTCUSD', 'USD', 100, true),
        ),
        order: limitOrder('BTCUSD', 25, { clientOrderId: 'bybit-inverse-client-1' }),
      },
      {
        exchange: new BybitSpotExchange('bybitspot', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT')),
        order: limitOrder('BTCUSDT', 1, { clientOrderId: 'bybit-spot-client-1' }),
      },
    ];

    for (const { exchange, order } of variants) {
      if (!(exchange instanceof BybitSpotExchange)) mockPositionMode(exchange);
      const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
      exchange.xhr = { post } as any;

      await expect(exchange.placeOrder(order as any)).resolves.toEqual(['order-1']);

      expect(post).toHaveBeenCalledWith(
        ENDPOINTS.CREATE_ORDER,
        expect.objectContaining({ orderLinkId: order.clientOrderId }),
      );
    }
  });

  it('preserves absent orderLinkId when the caller does not provide clientOrderId', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT'));
    mockPositionMode(exchange);
    const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
    exchange.xhr = { post } as any;

    await exchange.placeOrder(limitOrder());

    expect(post.mock.calls[0][1]).not.toHaveProperty('orderLinkId');
  });

  it('does not reuse caller clientOrderId when one safe-cex order splits into multiple native orders', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT', 1));
    mockPositionMode(exchange);
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(exchange.placeOrder(limitOrder('BTCUSDT', 2.5, { clientOrderId: 'bybit-client-1' }))).rejects.toThrow(
      'Bybit clientOrderId is only supported when one safe-cex order creates one native order',
    );
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects caller clientOrderId values outside Bybit constraints', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT'));
    mockPositionMode(exchange);
    exchange.xhr = { post: vi.fn() } as any;

    await expect(exchange.placeOrder(limitOrder('BTCUSDT', 1, { clientOrderId: 'invalid.id' }))).rejects.toThrow(
      'Bybit clientOrderId must match',
    );
  });

  it('keeps caller clientOrderId off attached TP/SL bracket payloads', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT'));
    mockPositionMode(exchange);
    const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
    exchange.xhr = { post } as any;

    await exchange.placeOrder(limitOrder('BTCUSDT', 1, { clientOrderId: 'bybit-client-1', stopLoss: 49_000 }));

    expect(post.mock.calls[0][1]).not.toHaveProperty('orderLinkId');
  });
});
