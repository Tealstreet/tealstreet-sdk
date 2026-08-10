import { describe, expect, it, vi } from 'vitest';

import { BitgetExchange } from '../../exchanges/bitget/bitget.exchange';
import { ENDPOINTS } from '../../exchanges/bitget/bitget.types';
import { DefaultStore } from '../../store/store.base';
import { MarginType, OrderSide, OrderType, PositionMode } from '../../types';

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

  it('rejects caller clientOrderId on attached TP/SL bracket orders', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    const unlimitedPost = vi.fn();
    exchange.xhr = { post } as any;
    exchange.unlimitedXHR = { post: unlimitedPost } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder(limitOrder({ clientOrderId: 'bitget-client-1', stopLoss: 49_000 })),
    ).rejects.toThrow(/clientOrderId cannot be honored/);
    expect(post).not.toHaveBeenCalled();
    expect(unlimitedPost).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('clientOrderId cannot be honored'));
  });

  it('rejects caller clientOrderId on standalone algo orders', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    const unlimitedPost = vi.fn();
    exchange.xhr = { post } as any;
    exchange.unlimitedXHR = { post: unlimitedPost } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder(
        limitOrder({ type: OrderType.StopLoss, price: 49_000, clientOrderId: 'bitget-client-1' }),
      ),
    ).rejects.toThrow(/clientOrderId cannot be honored/);
    expect(post).not.toHaveBeenCalled();
    expect(unlimitedPost).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('clientOrderId cannot be honored'));
  });

  it('sends the caller clientOrderId on the wire for a supported regular order', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange as any, 'getOrFetchMarginType').mockResolvedValue(MarginType.Cross);
    const post = vi.fn();
    const unlimitedPost = vi
      .fn()
      .mockResolvedValue({ data: { data: { successList: [{ orderId: 'order-1' }] } } });
    exchange.xhr = { post } as any;
    exchange.unlimitedXHR = { post: unlimitedPost } as any;

    await expect(exchange.placeOrder(limitOrder({ clientOrderId: 'bitget-client-1' }))).resolves.toEqual(['order-1']);

    expect(post).not.toHaveBeenCalled();
    expect(unlimitedPost).toHaveBeenCalledWith(
      ENDPOINTS.BATCH_ORDERS,
      expect.objectContaining({ orderList: [expect.objectContaining({ clientOid: 'bitget-client-1' })] }),
    );
  });

  it('rejects the whole batch before any send when an algo order carries a caller clientOrderId', async () => {
    const exchange = createExchange();
    const post = vi.fn();
    const unlimitedPost = vi.fn();
    exchange.xhr = { post } as any;
    exchange.unlimitedXHR = { post: unlimitedPost } as any;
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrders([
        limitOrder(),
        limitOrder({ type: OrderType.StopLoss, price: 49_000, clientOrderId: 'bitget-client-1' }),
      ]),
    ).rejects.toThrow(/clientOrderId cannot be honored/);
    expect(post).not.toHaveBeenCalled();
    expect(unlimitedPost).not.toHaveBeenCalled();
    expect(emitAndLogError).toHaveBeenCalledWith(expect.stringContaining('clientOrderId cannot be honored'));
  });
});
