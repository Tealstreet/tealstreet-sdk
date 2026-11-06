import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { getBybitMarketSlippageParams } from '../../exchanges/bybit/bybit.order-params';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { BybitDemoExchange } from '../reference/bybit/bybitdemo.exchange';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { BybitSpotExchange } from '../reference/bybit/bybitspot.exchange';
import { OrderSide, OrderType, PositionMode } from '../../types';

const marketOrder = (maxSlippagePct: number) => ({
  symbol: 'BTCUSDT',
  type: OrderType.Market,
  side: OrderSide.Buy,
  amount: 0.5,
  maxSlippagePct,
});

const createStore = (symbol: string, quote: string, isInverse = false) => {
  const market = {
    id: symbol,
    symbol,
    base: 'BTC',
    quote,
    active: true,
    precision: { amount: isInverse ? 1 : 0.001, price: 0.1 },
    limits: { amount: { min: isInverse ? 1 : 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
    isInverse,
  };
  return {
    subscribe: vi.fn(),
    markets: [market],
    marketsBySymbol: { [symbol]: market },
    tickersBySymbol: {},
    orders: [],
    positions: [],
    // Demo placement optimistically upserts the just-placed row so the chaser
    // can see its own order when the private WS drops the event.
    addOrUpdateOrders: vi.fn(),
  } as any;
};

describe('Bybit native market slippage tolerance', () => {
  it('serializes the shared percentage contract to V5 fields', () => {
    expect(getBybitMarketSlippageParams(marketOrder(1.25))).toEqual({
      slippageToleranceType: 'Percent',
      slippageTolerance: '1.25',
    });
  });

  it.each([0, 0.001, 10.01, 1.234, Number.POSITIVE_INFINITY])(
    'rejects an unsupported percentage value (%s)',
    (maxSlippagePct) => {
      expect(() => getBybitMarketSlippageParams(marketOrder(maxSlippagePct))).toThrow(
        'Bybit maxSlippagePct must be from 0.01 to 10 with at most two decimals',
      );
    },
  );

  it('rejects non-market and attached-protection uses before sending a request', () => {
    expect(() => getBybitMarketSlippageParams({ ...marketOrder(1), type: OrderType.Limit, price: 30_000 })).toThrow(
      'Bybit maxSlippagePct is only supported for plain market orders',
    );
    expect(() => getBybitMarketSlippageParams({ ...marketOrder(1), stopLoss: 29_000 })).toThrow(
      'Bybit maxSlippagePct is only supported for plain market orders',
    );
  });

  it('sends Percent protection through every Bybit single-order variant', async () => {
    const variants = [
      {
        exchange: new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT')),
        order: marketOrder(0.75),
      },
      {
        exchange: new BybitInverseExchange(
          'bybitinverse',
          { key: 'key', secret: 'secret' },
          createStore('BTCUSD', 'USD', true),
        ),
        order: { ...marketOrder(0.75), symbol: 'BTCUSD', amount: 25 },
      },
      {
        exchange: new BybitDemoExchange(
          'bybitdemo',
          { key: 'key', secret: 'secret' },
          createStore('BTCUSDT', 'USDT'),
        ),
        order: marketOrder(0.75),
      },
      {
        exchange: new BybitSpotExchange('bybitspot', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT')),
        order: marketOrder(0.75),
      },
    ];

    for (const { exchange, order } of variants) {
      if ('fetchPositionMode' in exchange) {
        (exchange as any).fetchPositionMode = vi.fn().mockResolvedValue(PositionMode.OneWay);
      }
      const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
      exchange.xhr = { post } as any;

      await expect(exchange.placeOrder(order)).resolves.toEqual(['order-1']);
      expect(post).toHaveBeenCalledWith(
        ENDPOINTS.CREATE_ORDER,
        expect.objectContaining({
          orderType: 'Market',
          slippageToleranceType: 'Percent',
          slippageTolerance: '0.75',
        }),
      );
    }
  });

  it('keeps protected market orders off the batch endpoint, which does not document slippage fields', async () => {
    const exchange = new BybitExchange('bybit', { key: 'key', secret: 'secret' }, createStore('BTCUSDT', 'USDT'));
    (exchange as any).fetchPositionMode = vi.fn().mockResolvedValue(PositionMode.OneWay);
    const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'order-1' } } });
    exchange.xhr = { post } as any;

    await expect(exchange.placeOrders([marketOrder(1)])).resolves.toEqual(['order-1']);
    expect(post).toHaveBeenCalledTimes(1);
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.CREATE_ORDER,
      expect.objectContaining({ slippageToleranceType: 'Percent', slippageTolerance: '1' }),
    );
  });
});
