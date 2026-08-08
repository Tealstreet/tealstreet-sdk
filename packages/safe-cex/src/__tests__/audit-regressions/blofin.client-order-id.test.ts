import type { Market, PlaceOrderOpts } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { BlofinExchange } from '../../exchanges/blofin/blofin.exchange';
import { ENDPOINTS } from '../../exchanges/blofin/blofin.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType, PositionMode } from '../../types';

const market: Market = {
  id: 'BTC-USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: { amount: { min: 1, max: 5 }, leverage: { min: 1, max: 100 } },
  info: { lotSize: '1', minSize: '1', contractValue: '0.001' },
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [market] });
  const exchange = new BlofinExchange('blofin', { key: 'key', secret: 'secret', passphrase: 'pass' }, store);
  (exchange as any)._accountMarginType = MarginType.Cross;
  (exchange as any)._accountPositionMode = PositionMode.OneWay;
  return exchange;
};

const order = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: market.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 2,
  price: 50_000,
  ...overrides,
});

describe('BloFin regular order clientOrderId support', () => {
  it('uses a caller clientOrderId for single native regular placeOrder payloads', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: '0', msg: 'success', data: [{ orderId: 'order-1' }] } });
    exchange.xhr = { post } as any;

    await expect(exchange.placeOrder(order({ clientOrderId: 'Client01' }))).resolves.toEqual(['order-1']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDERS_BATCH,
      [expect.objectContaining({ clientOrderId: 'Client01' })],
    );
  });

  it('preserves generated clientOrderId values when no caller ID is supplied', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: '0', msg: 'success', data: [{ orderId: 'order-1' }] } });
    exchange.xhr = { post } as any;

    await exchange.placeOrder(order());

    const payload = post.mock.calls[0][1][0];
    expect(payload.clientOrderId).toMatch(/^[A-Za-z0-9]{32}$/);
    expect(payload.clientOrderId).not.toBe('Client01');
  });

  it('rejects caller clientOrderId when one safe-cex order splits into multiple native orders', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder(order({ amount: 12, clientOrderId: 'Client01' })),
    ).rejects.toThrow('BloFin clientOrderId is supported only when one safe-cex order maps to one native order');
    expect(post).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledWith(
      'BloFin clientOrderId is supported only when one safe-cex order maps to one native order',
    );
  });

  it('rejects caller clientOrderId values outside BloFin constraints', async () => {
    const exchange = createExchange();

    await expect(exchange.placeOrder(order({ clientOrderId: 'bad_id' }))).rejects.toThrow(
      'BloFin clientOrderId must be 1-32 alphanumeric characters',
    );
  });
});
