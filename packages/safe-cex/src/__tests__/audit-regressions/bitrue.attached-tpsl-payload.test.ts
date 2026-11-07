import { describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { ENDPOINTS } from '../reference/bitrue/bitrue.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, Market, OrderSide, OrderStopTriggerType, OrderType, Position, PositionMode, PositionSide } from '../../types';

const MARKET: Market = {
  id: 'E-BTC-USDT',
  symbol: 'E-BTC-USDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: {
    amount: { min: 0.001, max: 1_000 },
    leverage: { min: 1, max: 100 },
  },
  info: { minOrderMoney: 5 },
};

const POSITION: Position = {
  id: 'E-BTC-USDT-long',
  symbol: MARKET.symbol,
  side: PositionSide.Long,
  entryPrice: 60_000,
  notional: 600,
  leverage: 7,
  marginType: MarginType.Isolated,
  positionMode: PositionMode.OneWay,
  unrealizedPnl: 0,
  contracts: 0.01,
  liquidationPrice: 0,
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [MARKET],
    positions: [POSITION],
    tickers: [{ symbol: MARKET.symbol, mark: 60_000, last: 60_000, bid: 59_999, ask: 60_001 } as any],
  });
  return new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);
};

describe('audit regression: Bitrue attached TP/SL payloads', () => {
  it.each([
    ['long limit', OrderSide.Buy, OrderType.Limit, 'BUY'],
    ['short limit', OrderSide.Sell, OrderType.Limit, 'SELL'],
    ['long market', OrderSide.Buy, OrderType.Market, 'BUY'],
    ['short market', OrderSide.Sell, OrderType.Market, 'SELL'],
  ] as const)('sends %s TP/SL only as parent-attached trigger params', async (_name, side, type, expectedSide) => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: '0', data: { orderId: 'server-order-id' } } });
    const refetchOrders = vi.spyOn(exchange, 'callMethodInWorker').mockResolvedValue([]);
    exchange.unlimitedXHR = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: MARKET.symbol,
        type,
        side,
        amount: 0.01,
        price: type === OrderType.Limit ? 60_000 : undefined,
        stopLoss: side === OrderSide.Buy ? 59_000 : 61_000,
        takeProfit: side === OrderSide.Buy ? 61_000 : 59_000,
        slTriggerType: OrderStopTriggerType.LastPrice,
        tpTriggerType: OrderStopTriggerType.MarkPrice,
      }),
    ).resolves.toEqual([]);

    expect(post).toHaveBeenCalledTimes(1);
    const [endpoint, parent] = post.mock.calls[0] as [string, Record<string, unknown>];
    expect(endpoint).toBe(ENDPOINTS.PLACE_ORDER);
    expect(parent).toEqual(expect.objectContaining({ side: expectedSide, open: 'OPEN', type: type === OrderType.Limit ? 'LIMIT' : 'MARKET' }));
    expect(parent).not.toHaveProperty('conditionOrder');
    if (type === OrderType.Limit) {
      expect(parent.volume).toBe('0.01');
    } else {
      expect(parent.amount).toEqual(expect.any(Number));
      expect(parent).not.toHaveProperty('volume');
    }

    const children = parent.triggerOrderCreateParams as Array<Record<string, unknown>>;
    expect(children).toHaveLength(2);
    expect(children).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ triggerType: 1, triggerPriceType: 1, triggerPrice: side === OrderSide.Buy ? 59_000 : 61_000, type: 'MARKET', price: 0 }),
        expect.objectContaining({ triggerType: 2, triggerPriceType: 2, triggerPrice: side === OrderSide.Buy ? 61_000 : 59_000, type: 'MARKET', price: 0 }),
      ]),
    );
    for (const child of children) {
      expect(child.clientOrderId).toEqual(expect.any(String));
      expect(child).not.toHaveProperty('side');
      expect(child).not.toHaveProperty('open');
      expect(child).not.toHaveProperty('volume');
      expect(child).not.toHaveProperty('conditionOrder');
    }
    expect(children[0].clientOrderId).not.toBe(children[1].clientOrderId);
    expect(refetchOrders).toHaveBeenCalledWith('fetchAndSetOrders', [MARKET.symbol]);
  });

  it('does not send a sibling conditional when Bitrue rejects the parent', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: '-2019', msg: 'Parent rejected' } });
    const refetchOrders = vi.spyOn(exchange, 'callMethodInWorker');
    exchange.unlimitedXHR = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: MARKET.symbol,
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.01,
        price: 60_000,
        stopLoss: 59_000,
      }),
    ).rejects.toThrow('Parent rejected');

    expect(post).toHaveBeenCalledTimes(1);
    expect(refetchOrders).not.toHaveBeenCalled();
  });
});
