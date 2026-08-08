import { beforeEach, describe, expect, it, vi } from 'vitest';

import { CoinbaseExchange } from '../../exchanges/coinbase/coinbase.exchange';
import { ENDPOINTS } from '../../exchanges/coinbase/coinbase.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const { uuidMock } = vi.hoisted(() => ({ uuidMock: vi.fn() }));
vi.mock('../../utils/uuid', () => ({ uuid: uuidMock }));

const createExchange = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC-USD',
        symbol: 'BTC-USD',
        precision: { amount: 0.001, price: 0.5 },
        limits: { amount: { min: 0.001, max: 100 } },
      } as any,
    ],
  });
  return new CoinbaseExchange('coinbase' as any, { key: 'key', secret: 'secret' } as any, store);
};

describe('Coinbase regular order clientOrderId', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    uuidMock.mockReset();
    uuidMock.mockReturnValue('generated-client-id');
  });

  it('uses a caller-provided client_order_id for regular orders', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, success_response: { order_id: 'native-1' } } });
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.01,
        price: 50_000,
        clientOrderId: 'caller-client-id',
      }),
    ).resolves.toEqual(['native-1']);

    expect(post.mock.calls[0][1]).toMatchObject({ client_order_id: 'caller-client-id' });
    expect(uuidMock).not.toHaveBeenCalled();
  });

  it('preserves the generated client_order_id on non-regular stop placement', async () => {
    const exchange = createExchange();
    const post = vi.fn().mockResolvedValue({ data: { success: true, success_response: { order_id: 'native-2' } } });
    exchange.xhr = { post } as any;

    await exchange.placeOrder({
      symbol: 'BTC-USD',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      amount: 0.01,
      price: 49_000,
      clientOrderId: 'ignored-client-id',
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ORDER,
      expect.objectContaining({ client_order_id: 'tealstreet-generated-client-id' }),
    );
  });

  it('rejects an empty caller clientOrderId on regular orders', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTC-USD',
        type: OrderType.Limit,
        side: OrderSide.Buy,
        amount: 0.01,
        price: 50_000,
        clientOrderId: '',
      }),
    ).rejects.toThrow('Coinbase clientOrderId must be non-empty');

    expect(post).not.toHaveBeenCalled();
  });
});
