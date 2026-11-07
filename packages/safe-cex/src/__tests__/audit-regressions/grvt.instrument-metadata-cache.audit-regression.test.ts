import { describe, expect, it, vi } from 'vitest';

import { GrvtExchange } from '../reference/grvt/grvt.exchange';
import { ENDPOINTS } from '../../exchanges/grvt/grvt.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const instrument = (overrides: Record<string, unknown> = {}) => ({
  i: 'BTC_USDT_Perp',
  ih: '0x030501',
  b: 'BTC',
  q: 'USDT',
  k: 'PERPETUAL',
  bd: 3,
  qd: 6,
  ts: '0.1',
  ms: '0.001',
  ia: true,
  ct: '1700000000000000000',
  ...overrides,
});

const makeExchange = () =>
  new GrvtExchange(
    'grvt' as any,
    { tuning: { consoleLogging: false } } as any,
    new DefaultStore(),
  );

describe('GRVT instrument metadata cache audit regression', () => {
  // Official all_instruments defaults omitted/empty k to PERPETUAL. These
  // injected rows prove latent mapper acceptance, not listed product support.
  it.each([
    ['FUTURE', 'BTC_USDT_Fut_20Oct23'],
    ['CALL', 'BTC_USDT_Call_20Oct23_2800'],
    ['PUT', 'BTC_USDT_Put_20Oct23_2800'],
  ])('does not request %s even though its generic mapper accepts injected rows', async (kind, id) => {
    const exchange = makeExchange();
    const post = vi.fn().mockResolvedValue({ data: { r: [instrument({
      i: id, k: kind, e: '1697788800000000000', sp: '2800000000000', sp1: 'DAILY',
    })] } });
    exchange.xhr = { post } as any;

    const markets = await exchange.fetchMarkets();

    expect(post).toHaveBeenCalledExactlyOnceWith(ENDPOINTS.INSTRUMENTS, {});
    expect(markets).toHaveLength(1);
    expect(markets[0]).toMatchObject({ id, symbol: id.toUpperCase(), isSpot: false, isPerpetual: false });
    expect(markets[0].info).toEqual({ ih: '0x030501', bd: 3, qd: 6 });
  });

  it('stores only order-signing metadata from all_instruments on each market', async () => {
    const exchange = makeExchange();
    const post = vi.fn().mockResolvedValue({ data: { r: [instrument({ ignored: 'not-cached' })] } });
    exchange.xhr = { post } as any;

    const markets = await exchange.fetchMarkets();

    expect(markets[0].info).toEqual({ ih: '0x030501', bd: 3, qd: 6 });
    expect(post).toHaveBeenCalledWith(ENDPOINTS.INSTRUMENTS, {});
  });

  it('replaces cached metadata on a same-membership market refresh', async () => {
    const exchange = makeExchange();
    const post = vi
      .fn()
      .mockResolvedValueOnce({ data: { r: [instrument()] } })
      .mockResolvedValueOnce({ data: { r: [instrument({ ih: '0x030599', bd: -3, qd: 8 })] } });
    exchange.xhr = { post } as any;
    exchange.store.update({ markets: await exchange.fetchMarkets() });

    await expect(exchange.refreshMarkets()).resolves.toEqual({ added: [], removed: [] });

    expect(exchange.store.markets[0].info).toEqual({ ih: '0x030599', bd: -3, qd: 8 });
    expect(post).toHaveBeenCalledTimes(2);
  });

  it('uses cached metadata without refetching all instruments per order', async () => {
    const exchange = makeExchange();
    const post = vi.fn().mockResolvedValue({ data: { r: [instrument()] } });
    exchange.xhr = { post } as any;
    exchange.store.update({ markets: await exchange.fetchMarkets() });
    post.mockClear();
    exchange.wallet = {} as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTC_USDT_PERP',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 0,
        price: 50_000,
      }),
    ).rejects.toThrow('Invalid order size');
    expect(post).not.toHaveBeenCalled();
  });

  it.each([
    ['missing', undefined],
    ['non-hex hash', { ih: 'not-hex', bd: 3, qd: 6 }],
    ['non-integer decimals', { ih: '0x030501', bd: 3.5, qd: 6 }],
  ])('fails closed for %s signing metadata', async (_name, info) => {
    const exchange = makeExchange();
    exchange.wallet = {} as any;
    exchange.store.update({
      markets: [
        {
          id: 'BTC_USDT_Perp',
          symbol: 'BTC_USDT_PERP',
          base: 'BTC',
          quote: 'USDT',
          active: true,
          precision: { amount: 0.001, price: 0.1 },
          limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
          info,
        },
      ] as any,
    });
    const post = vi.fn();
    exchange.xhr = { post } as any;

    await expect(
      exchange.placeOrder({
        symbol: 'BTC_USDT_PERP',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 1,
        price: 50_000,
      }),
    ).rejects.toThrow('does not have valid GRVT signing metadata (ih/bd/qd)');
    expect(post).not.toHaveBeenCalled();
  });
});
