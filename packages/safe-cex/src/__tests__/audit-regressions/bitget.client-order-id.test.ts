import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType, PositionMode } from '../../types';

const createExchange = () => {
  const exchange = new BitgetExchange(
    'bitget',
    { key: 'key', secret: 'secret', passphrase: 'passphrase' },
    new DefaultStore(),
  );
  exchange.store.update({
    markets: [
      {
        id: 'BTCUSDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
      },
    ],
  });
  vi.spyOn(exchange as any, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay);
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

describe('Bitget regular-order clientOrderId', () => {
  it('uses caller clientOrderId for a single native regular order', async () => {
    const [payload] = await createExchange().formatCreateOrder(limitOrder({ clientOrderId: 'bitget-client-1' }));

    expect(payload.clientOid).toBe('bitget-client-1');
  });

  it('preserves generated clientOid when the caller does not provide one', async () => {
    const [payload] = await createExchange().formatCreateOrder(limitOrder());

    expect(payload.clientOid).toEqual(expect.any(String));
    expect(payload.clientOid).not.toBe('bitget-client-1');
  });

  it('does not reuse caller clientOrderId when one safe-cex order splits into multiple native orders', async () => {
    await expect(
      createExchange().formatCreateOrder(limitOrder({ amount: 20.5, clientOrderId: 'bitget-client-1' })),
    ).rejects.toThrow('Bitget clientOrderId is only supported when one safe-cex order creates one native order');
  });

  it('rejects caller clientOrderId values outside Bitget constraints', async () => {
    await expect(createExchange().formatCreateOrder(limitOrder({ clientOrderId: 'x'.repeat(33) }))).rejects.toThrow(
      'Bitget clientOrderId must match',
    );
  });

  it('keeps caller clientOrderId off attached TP/SL bracket payloads', async () => {
    const [payload] = await createExchange().formatCreateOrder(
      limitOrder({ clientOrderId: 'bitget-client-1', stopLoss: 49_000 }),
    );

    expect(payload.clientOid).toEqual(expect.any(String));
    expect(payload.clientOid).not.toBe('bitget-client-1');
  });
});
