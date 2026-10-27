import { describe, expect, it, vi } from 'vitest';

import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';

const createExchange = (apiVersion: 'v1' | 'v3') => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        symbol: 'BTCUSDT',
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { max: 100 } },
        info: { filters: [{ filterType: 'PRICE_FILTER', tickSize: '0.1' }] },
      } as any,
    ],
  });

  return new AsterDEXExchange(
    'asterdex',
    {
      key: 'test-key',
      secret: 'test-secret',
      extra: { asterdexApiVersion: apiVersion },
    },
    store,
  );
};

describe('audit regression: AsterDEX V3 self-trade prevention', () => {
  it('sends a validated per-order STP override only on V3', async () => {
    const exchange = createExchange('v3');
    const post = vi.fn().mockResolvedValue({ data: { orderId: '12345' } });
    (exchange as any).secondaryAPI.post = post;

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 0.25,
        price: 50_000,
        timeInForce: OrderTimeInForce.PostOnly,
        stpMode: 'EXPIRE_TAKER',
      }),
    ).resolves.toEqual(['12345']);

    expect(post).toHaveBeenCalledWith('/fapi/v3/order', expect.objectContaining({ stpMode: 'EXPIRE_TAKER' }));
  });

  it('rejects V1 and malformed STP overrides before order I/O', async () => {
    const v1 = createExchange('v1');
    const v1Post = vi.fn();
    (v1 as any).secondaryAPI.post = v1Post;

    await expect(
      v1.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 1,
        price: 50_000,
        stpMode: 'EXPIRE_BOTH',
      }),
    ).rejects.toThrow('requires API v3');
    expect(v1Post).not.toHaveBeenCalled();

    const v3 = createExchange('v3');
    const v3Post = vi.fn();
    (v3 as any).secondaryAPI.post = v3Post;
    await expect(
      v3.placeOrder({
        symbol: 'BTCUSDT',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 1,
        price: 50_000,
        stpMode: 'INVALID' as any,
      }),
    ).rejects.toThrow('Invalid AsterDEX self-trade-prevention mode');
    expect(v3Post).not.toHaveBeenCalled();
  });

  it('reads and changes the validated account-level V3 STP mode', async () => {
    const exchange = createExchange('v3');
    const get = vi.fn().mockResolvedValue({ data: { stpMode: 'EXPIRE_MAKER' } });
    const post = vi.fn().mockResolvedValue({ data: { code: 200, msg: 'success' } });
    (exchange as any).primaryAPI.get = get;
    (exchange as any).primaryAPI.post = post;

    await expect(exchange.fetchStpMode()).resolves.toBe('EXPIRE_MAKER');
    await expect(exchange.changeStpMode('EXPIRE_BOTH')).resolves.toBeUndefined();

    expect(get).toHaveBeenCalledWith('/fapi/v3/stpMode');
    expect(post).toHaveBeenCalledWith('/fapi/v3/stpMode', { stpMode: 'EXPIRE_BOTH' });
  });

  it('fails closed on an unknown account-level STP response', async () => {
    const exchange = createExchange('v3');
    (exchange as any).primaryAPI.get = vi.fn().mockResolvedValue({ data: { stpMode: 'UNKNOWN' } });

    await expect(exchange.fetchStpMode()).rejects.toThrow('AsterDEX returned an invalid self-trade-prevention mode');
  });
});
