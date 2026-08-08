import { describe, expect, it, vi } from 'vitest';

import { PhemexExchange } from '../../exchanges/phemex/phemex.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const createExchange = () => {
  const exchange = new PhemexExchange('phemex' as any, { key: 'key', secret: 'secret' } as any, new DefaultStore());
  exchange.store.update({
    markets: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
        info: { symbol: 'BTCUSDT' },
      },
    ],
  });
  vi.spyOn(exchange as any, 'getOrderPositionSide').mockResolvedValue('Long');
  vi.spyOn(exchange as any, 'getOrFetchPositionsForSymbol').mockResolvedValue([]);
  return exchange;
};

const limitOrder = (overrides = {}) => ({
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 50_000,
  ...overrides,
});

describe('Phemex regular-order clientOrderId', () => {
  it('uses caller clientOrderId as clOrdID for a single native regular order', async () => {
    const [payload] = await (createExchange() as any).formatCreateOrder(
      limitOrder({ clientOrderId: 'phemex-client-1' }),
    );

    expect(payload.clOrdID).toBe('phemex-client-1');
  });

  it('preserves generated clOrdID when the caller does not provide one', async () => {
    const [payload] = await (createExchange() as any).formatCreateOrder(limitOrder());

    expect(payload.clOrdID).toMatch(/^Tealstreet_/);
  });

  it('does not reuse caller clientOrderId when one safe-cex order splits into multiple native orders', async () => {
    await expect(
      (createExchange() as any).formatCreateOrder(limitOrder({ amount: 20.5, clientOrderId: 'phemex-client-1' })),
    ).rejects.toThrow('Phemex clientOrderId is only supported when one safe-cex order creates one native order');
  });

  it('rejects caller clientOrderId values longer than Phemex allows', async () => {
    await expect(
      (createExchange() as any).formatCreateOrder(limitOrder({ clientOrderId: 'x'.repeat(41) })),
    ).rejects.toThrow('Phemex clientOrderId must be 1-40 characters');
  });

  it('keeps caller clientOrderId off attached TP/SL bracket payloads', async () => {
    const [payload] = await (createExchange() as any).formatCreateOrder(
      limitOrder({ clientOrderId: 'phemex-client-1', stopLoss: 49_000 }),
    );

    expect(payload.clOrdID).toMatch(/^Tealstreet_/);
  });
});
