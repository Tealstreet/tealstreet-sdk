import { afterEach, describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { HyrotraderExchange } from '../../exchanges/bybit/hyrotrader.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market, Order, OrderSide, OrderStatus, OrderType, PositionMode } from '../../types';

class TestHyroExchange extends HyrotraderExchange {
  getBybitRestBaseUrl() {
    return 'https://simulator.test';
  }
}

const exchanges: TestHyroExchange[] = [];
const setup = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTCUSDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
      } as Market,
    ],
  });
  const exchange = new TestHyroExchange('hyrotrader', { key: 'test-key', secret: 'test-secret' }, store);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  exchanges.push(exchange);
  return { exchange, store };
};
const order = { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.01, price: 50_000 };
const success = (result: unknown) => ({ data: { retCode: 0, retMsg: 'OK', result } });
const wallet = [{ accountType: 'UNIFIED', totalEquity: '1000', totalAvailableBalance: '900', coin: [] }];
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});

describe('HyroTrader supported subset (partner simulator contract)', () => {
  it('validates through the UNIFIED wallet without account-info', async () => {
    const { exchange } = setup();
    const get = vi.fn().mockResolvedValue(success({ list: wallet }));
    exchange.xhr = { get } as any;
    expect(await exchange.validateAccount()).toMatchObject({ error: '', balance: { total: 1000 } });
    expect(get).toHaveBeenCalledExactlyOnceWith(ENDPOINTS.BALANCE, { params: { accountType: 'UNIFIED' } });
  });
  it.each([
    { retCode: 10003, retMsg: 'API key is invalid' },
    { retCode: 0, retMsg: 'OK', result: { list: [{ ...wallet[0], totalAvailableBalance: 'invalid' }] } },
    {
      retCode: 0,
      retMsg: 'OK',
      result: { list: [{ ...wallet[0], coin: [{ coin: 'USDT', walletBalance: 'invalid' }] }] },
    },
    { retCode: 0, retMsg: 'OK', result: {} },
    { retCode: 0, retMsg: 'OK', result: { list: [{}] } },
    { retCode: 0, retMsg: 'OK', result: { list: [{ ...wallet[0], accountType: 'CONTRACT' }] } },
  ])('rejects failed or malformed wallet validation', async (data) => {
    const { exchange } = setup();
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data }) } as any;
    expect((await exchange.validateAccount()).error).not.toBe('');
  });
  it('uses individual creates with unique client IDs and one-way indices', async () => {
    const { exchange } = setup();
    const post = vi
      .fn()
      .mockResolvedValueOnce(success({ orderId: 'first' }))
      .mockResolvedValueOnce(success({ orderId: 'second' }));
    exchange.xhr = { post } as any;
    expect(await exchange.placeOrders([order, { ...order, price: 49_000 }])).toEqual(['first', 'second']);
    expect(post.mock.calls.every(([path]) => path === ENDPOINTS.CREATE_ORDER)).toBe(true);
    const ids = post.mock.calls.map(([, payload]) => payload.orderLinkId);
    expect(new Set(ids).size).toBe(2);
    ids.forEach((id) => expect(id.length).toBeLessThanOrEqual(36));
    post.mock.calls.forEach(([, payload]) => expect(payload.positionIdx).toBe(0));
  });
  it('cancels individual orders and retires only successful rows', async () => {
    const { exchange, store } = setup();
    const rows = ['first', 'second'].map((id) => ({ ...order, id, status: OrderStatus.Open }) as Order);
    store.update({ orders: rows });
    const post = vi
      .fn()
      .mockResolvedValueOnce(success({}))
      .mockResolvedValueOnce({ data: { retCode: 10005, retMsg: 'permission denied' } });
    exchange.xhr = { post } as any;
    await expect(exchange.cancelOrders(rows)).rejects.toThrow('permission denied');
    expect(post.mock.calls[0]).toEqual([
      ENDPOINTS.CANCEL_ORDER,
      { category: 'linear', symbol: 'BTCUSDT', orderId: 'first' },
    ]);
    expect(store.orders.map(({ id }) => id)).toEqual(['second']);
  });
  it('accepts an already-terminal cancellation and skips virtual bracket IDs', async () => {
    const { exchange, store } = setup();
    const rows = ['first', 'first__stop_loss'].map((id) => ({ ...order, id, status: OrderStatus.Open }) as Order);
    store.update({ orders: rows });
    const post = vi
      .fn()
      .mockResolvedValue({ data: { retCode: 110001, retMsg: 'order not exists or too late to cancel' } });
    exchange.xhr = { post } as any;
    await exchange.cancelOrders(rows);
    expect(post).toHaveBeenCalledTimes(1);
    expect(store.orders).toEqual([]);
  });
  it('disables unsupported capabilities without sending requests', async () => {
    const { exchange } = setup();
    const get = vi.fn();
    exchange.xhr = { get } as any;
    expect(await exchange.fetchFees('linear')).toEqual([]);
    await expect(exchange.getAccount()).rejects.toThrow('does not provide');
    await expect(exchange.fetchApiKeyInfo()).rejects.toThrow('does not provide');
    await expect(exchange.changePositionMode(true)).rejects.toThrow('one-way');
    await expect(exchange.changeMarginType(true)).rejects.toThrow('cross margin');
    expect(exchange.capabilities).toMatchObject({ incomeHistory: false, incomeRange: false });
    expect(get).not.toHaveBeenCalled();
    expect(getExchangeMetadata('hyrotrader')).toMatchObject({
      positionModes: [PositionMode.OneWay],
      hidePositionModeSelector: true,
      supportsFeeRates: false,
      supportsPositionMargin: false,
    });
  });
  it('rejects undocumented Bybit execution controls before any create', async () => {
    const { exchange } = setup();
    const post = vi.fn();
    exchange.xhr = { post } as any;
    await expect(
      exchange.placeOrders([
        order,
        {
          symbol: 'BTCUSDT',
          type: OrderType.Limit,
          side: OrderSide.Buy,
          amount: 0.01,
          bybitBbo: { sideType: 'Counterparty', level: 1 },
        },
      ]),
    ).rejects.toThrow('does not support');
    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Market,
        side: OrderSide.Buy,
        amount: 0.01,
        bybitSlippageToleranceTicks: 5,
      }),
    ).rejects.toThrow('does not support');
    await expect(exchange.placeOrder({ ...order, type: OrderType.Market, maxSlippagePct: 1 })).rejects.toThrow(
      'client-defined slippage',
    );
    expect(post).not.toHaveBeenCalled();
  });
  it('deduplicates ignored status filters and excludes USDC markets', async () => {
    const { exchange, store } = setup();
    const market = store.markets[0];
    vi.spyOn(exchange, 'fetchMarketsHelper').mockResolvedValue([
      market,
      market,
      { ...market, symbol: 'BTCUSDC', quote: 'USDC' },
    ]);
    expect(await exchange.fetchMarkets()).toEqual([market]);
    await expect(exchange.placeOrder({ ...order, symbol: 'BTCUSDC' })).rejects.toThrow('USDT');
  });
});
