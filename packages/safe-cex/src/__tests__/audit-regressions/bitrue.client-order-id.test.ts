import type { Market, PlaceOrderOpts, Position } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../../exchanges/bitrue/bitrue.exchange';
import { ENDPOINTS } from '../../exchanges/bitrue/bitrue.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType, PositionMode, PositionSide } from '../../types';

const market: Market = {
  id: 'E-BTC-USDT',
  symbol: 'E-BTC-USDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1 }, leverage: { min: 1, max: 100 } },
  info: { minOrderMoney: 5 },
};

const position: Position = {
  id: 'E-BTC-USDT-long',
  symbol: market.symbol,
  side: PositionSide.Long,
  entryPrice: 50_000,
  notional: 500,
  leverage: 5,
  marginType: MarginType.Cross,
  positionMode: PositionMode.OneWay,
  unrealizedPnl: 0,
  contracts: 0.01,
  liquidationPrice: 0,
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [market], positions: [position] });
  return new BitrueExchange('bitrue', { key: 'key', secret: 'secret' }, store);
};

const order = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: market.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 0.5,
  price: 50_000,
  ...overrides,
});

describe('Bitrue regular order clientOrderId support', () => {
  it('uses caller clientOrderId for single native regular placeOrder payloads', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: '0', data: { orderId: 'order-1' } } });
    exchange.unlimitedXHR = { post } as any;

    await expect(exchange.placeOrder(order({ clientOrderId: 'custom-client-id' }))).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({ clientOrderId: 'custom-client-id' }),
    );
  });

  it('rejects caller clientOrderId when one safe-cex order splits into multiple native orders', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.unlimitedXHR = { post } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder(order({ amount: 2.25, clientOrderId: 'custom-client-id' })),
    ).rejects.toThrow('Bitrue clientOrderId is supported only when one safe-cex order maps to one native order');
    expect(post).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledWith(
      'Bitrue clientOrderId is supported only when one safe-cex order maps to one native order',
    );
  });

  it('rejects caller clientOrderId values at or above Bitrue length limit', async () => {
    const exchange = createExchange();

    await expect(exchange.placeOrder(order({ clientOrderId: 'x'.repeat(32) }))).rejects.toThrow(
      'Bitrue clientOrderId must be a non-empty string shorter than 32 characters',
    );
  });
});
