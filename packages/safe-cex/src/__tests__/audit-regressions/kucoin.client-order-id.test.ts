import { describe, expect, it, vi } from 'vitest';

import { KucoinExchange } from '../../exchanges/kucoin/kucoin.exchange';
import { KucoinSpotExchange } from '../../exchanges/kucoin/kucoinspot.exchange';
import { KUCOIN_SPOT_ENDPOINTS } from '../../exchanges/kucoin/kucoinspot.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const futuresExchange = () => {
  const exchange = new KucoinExchange(
    'kucoin',
    { key: 'key', secret: 'secret', passphrase: 'passphrase', tealstreetTunnel: false },
    new DefaultStore(),
  );
  exchange.store.update({
    markets: [
      {
        id: 'XBTUSDTM',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 1, price: 0.1 },
        limits: { amount: { min: 1, max: 100 }, leverage: { min: 1, max: 100 } },
      },
    ],
  });
  return exchange;
};

const spotExchange = () => {
  const exchange = new KucoinSpotExchange(
    'kucoinspot',
    { key: 'key', secret: 'secret', passphrase: 'passphrase', tealstreetTunnel: false },
    new DefaultStore(),
  );
  exchange.store.update({
    markets: [
      {
        id: 'BTC-USDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.00001, price: 0.01 },
        limits: { amount: { min: 0.00001, max: 100 }, leverage: { min: 1, max: 1 } },
      },
    ],
  });
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

describe('KuCoin regular-order clientOrderId', () => {
  it('uses caller clientOrderId for futures regular orders', async () => {
    const [payload] = await futuresExchange().formatCreateOrder(limitOrder({ clientOrderId: 'kucoin-client-1' }));

    expect(payload.clientOid).toBe('kucoin-client-1');
  });

  it('preserves generated futures clientOid when the caller does not provide one', async () => {
    const [payload] = await futuresExchange().formatCreateOrder(limitOrder());

    expect(payload.clientOid).toEqual(expect.any(String));
    expect(payload.clientOid).not.toBe('kucoin-client-1');
  });

  it('keeps caller clientOrderId off futures attached TP/SL payloads', async () => {
    const [payload] = await futuresExchange().formatCreateOrder(
      limitOrder({ clientOrderId: 'kucoin-client-1', stopLoss: 49_000 }),
    );

    expect(payload.clientOid).toEqual(expect.any(String));
    expect(payload.clientOid).not.toBe('kucoin-client-1');
  });

  it('rejects an empty futures clientOrderId on regular orders', async () => {
    await expect(futuresExchange().formatCreateOrder(limitOrder({ clientOrderId: '' }))).rejects.toThrow(
      'KuCoin clientOrderId must be non-empty',
    );
  });

  it('uses caller clientOrderId for spot regular orders', async () => {
    const exchange = spotExchange();
    const post = vi.fn().mockResolvedValue({ data: { code: '200000', data: { orderId: 'spot-order-1' } } });
    exchange.unlimitedXHR = { post } as any;

    await expect(
      exchange.placeOrder(limitOrder({ amount: 0.1, clientOrderId: 'kucoin-spot-client-1' })),
    ).resolves.toEqual(['spot-order-1']);

    expect(post).toHaveBeenCalledWith(
      KUCOIN_SPOT_ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({ clientOid: 'kucoin-spot-client-1' }),
    );
  });

  it('rejects an empty spot clientOrderId on regular orders', async () => {
    await expect(spotExchange().placeOrder(limitOrder({ amount: 0.1, clientOrderId: '' }))).rejects.toThrow(
      'KuCoin Spot clientOrderId must be non-empty',
    );
  });
});
