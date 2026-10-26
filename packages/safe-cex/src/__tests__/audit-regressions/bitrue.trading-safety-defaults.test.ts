import { describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { ENDPOINTS } from '../reference/bitrue/bitrue.types';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
} from '../../types';

const market: Market = {
  id: 'E-BTC-USDT',
  symbol: 'E-BTC-USDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1_000 }, leverage: { min: 1, max: 100 } },
  info: { minOrderMoney: 5 },
};

const setup = () => {
  const store = new DefaultStore();
  store.update({
    markets: [market],
    positions: [
      {
        id: 'long',
        symbol: market.symbol,
        side: PositionSide.Long,
        entryPrice: 60_000,
        notional: 600,
        leverage: 7,
        marginType: MarginType.Isolated,
        positionMode: PositionMode.OneWay,
        unrealizedPnl: 0,
        contracts: 0.01,
        liquidationPrice: 0,
      } as Position,
    ],
    tickers: [{ symbol: market.symbol, mark: 60_000, last: 60_000, bid: 59_999, ask: 60_001 } as any],
  });
  const exchange = new BitrueExchange('bitrue', { key: 'test-key', secret: 'test-secret' }, store);
  const post = vi.fn().mockResolvedValue({ data: { code: '0', data: { orderId: '2334151155877155281' } } });
  exchange.unlimitedXHR = { post } as any;
  exchange.xhr = { post } as any;
  vi.spyOn(exchange, 'callMethodInWorker').mockResolvedValue([]);
  return { exchange, post };
};

describe('audit regression: Bitrue trading safety defaults', () => {
  it('mints an independent client ID for every native split', async () => {
    const { exchange, post } = setup();
    await exchange.placeOrder({
      symbol: market.symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 2000,
      price: 60000,
    });
    const ids = post.mock.calls.map((call) => call[1].clientOrderId);
    expect(ids).toHaveLength(2);
    expect(ids.every((id) => typeof id === 'string' && id.length > 0)).toBe(true);
    expect(new Set(ids).size).toBe(ids.length);
  });
  it('defaults attached SL to mark and TP to last price', async () => {
    const { exchange, post } = setup();
    await exchange.placeOrder({
      symbol: market.symbol,
      type: OrderType.Limit,
      side: OrderSide.Buy,
      amount: 0.01,
      price: 60_000,
      stopLoss: 59_000,
      takeProfit: 61_000,
    });
    expect(post.mock.calls[0][1].triggerOrderCreateParams).toEqual([
      expect.objectContaining({ triggerType: 1, triggerPriceType: 2 }),
      expect.objectContaining({ triggerType: 2, triggerPriceType: 1 }),
    ]);
  });

  it.each([
    [OrderType.StopLoss, undefined, undefined, 2],
    [OrderType.TakeProfit, undefined, undefined, 1],
    [OrderType.TakeProfit, OrderStopTriggerType.LastPrice, OrderStopTriggerType.MarkPrice, 2],
  ] as const)(
    'binds %s to CLOSE and uses its own trigger reference',
    async (type, slTriggerType, tpTriggerType, expectedReference) => {
      const { exchange, post } = setup();
      await exchange.placeOrder({
        symbol: market.symbol,
        type,
        side: OrderSide.Sell,
        amount: 0.01,
        price: type === OrderType.StopLoss ? 59_000 : 61_000,
        reduceOnly: false,
        slTriggerType,
        tpTriggerType,
      });
      expect(post.mock.calls[0][1]).toEqual(
        expect.objectContaining({
          open: 'CLOSE',
          conditionOrder: true,
          type: 'MARKET',
          triggerPriceType: expectedReference,
        }),
      );
    },
  );

  it.each([
    [OrderType.Market, false],
    [OrderType.StopLoss, true],
  ] as const)('cancels %s by its exact native order identity', async (type, conditionOrder) => {
    const { exchange, post } = setup();
    await exchange.cancelOrders([
      {
        id: '2334151155877155281',
        symbol: market.symbol,
        type,
        status: OrderStatus.Open,
        side: OrderSide.Sell,
        price: 60_000,
        amount: 0.01,
        filled: 0,
        remaining: 0.01,
        info: { clientOrderId: 'stale-client-id' },
      } as Order,
    ]);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.CANCEL_ORDER, {
      orderId: '2334151155877155281',
      contractName: market.symbol,
      conditionOrder,
    });
  });
});
