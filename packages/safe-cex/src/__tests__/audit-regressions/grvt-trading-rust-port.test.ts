import { describe, expect, it, vi } from 'vitest';
import { GrvtExchange } from '../reference/grvt/grvt.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderTimeInForce, OrderType } from '../../types';

const exchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [{ id: 'BTC_USDT_Perp', symbol: 'BTC_USDT_PERP', base: 'BTC', quote: 'USDT', active: true,
    info: { ih: '0x030501', bd: 9, qd: 6 },
    precision: { amount: 0.001, price: 0.1 }, limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } } } as any] });
  const e = new GrvtExchange('grvt', {} as any, store);
  e.grvtTradingAccountId = '777';
  return e;
};
describe('GRVT order semantics repaired by the Rust port', () => {
  it('creates an ordinary entry followed by both opposite reducing triggers', async () => {
    const e = exchange();
    const place = e.placeOrder;
    const calls: any[] = [];
    e.placeOrder = vi.fn(async (opts) => { calls.push(opts); return [String(calls.length)]; });
    await expect(place({ symbol: 'BTC_USDT_PERP', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100,
      stopLoss: 90, takeProfit: 120 })).resolves.toEqual(['1', '2', '3']);
    expect(calls[0]).toEqual({ symbol: 'BTC_USDT_PERP', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100 });
    expect(calls[1]).toMatchObject({ type: OrderType.StopLoss, side: OrderSide.Sell, price: 90, reduceOnly: true, close: false });
    expect(calls[2]).toMatchObject({ type: OrderType.TakeProfit, side: OrderSide.Sell, price: 120, reduceOnly: true, close: false });
  });
  it('rejects an invalid bracket before creating its entry', async () => {
    const e = exchange(); const place = e.placeOrder; e.placeOrder = vi.fn();
    await expect(place({ symbol: 'BTC_USDT_PERP', type: OrderType.Limit, side: OrderSide.Buy, amount: 1, price: 100, stopLoss: -1 })).rejects.toThrow('Invalid GRVT bracket');
    expect(e.placeOrder).not.toHaveBeenCalled();
  });
  it('reads complete native orders and cancels exact IDs without cancelling same-base siblings', async () => {
    const e = exchange();
    const cancelAllOrders = vi.fn();
    const openOrders = vi.fn().mockResolvedValueOnce({ status: 200, data: { r: [
      { sa: '777', oi: '0x1', l: [{ i: 'BTC_USDT_Perp' }], m: { co: '1' } },
      { sa: '777', oi: '0x2', l: [{ i: 'BTC_USDT_Fut_30Dec26' }], m: { co: '2' } },
    ] } }).mockResolvedValueOnce({ status: 200, data: { r: { a: true } } });
    e.tradingXhr.post = openOrders;
    e.grvtClient = { tdgClient: { cancelAllOrders }, authenticatedEndpoint: async () => ({}) } as any;
    await e.cancelSymbolOrders('BTC_USDT_PERP');
    expect(openOrders.mock.calls.map(([path, body]) => [path, body])).toEqual([
      ['/lite/v1/open_orders', { sa: '777' }],
      ['/lite/v1/cancel_order', { sa: '777', oi: '0x1' }],
    ]);
    expect(cancelAllOrders).not.toHaveBeenCalled();
  });
  it('replaces only the remaining quantity after a partial fill', async () => {
    const e = exchange(); e.cancelOrder = vi.fn().mockResolvedValue(undefined); e.placeOrder = vi.fn().mockResolvedValue(['new']);
    const order = { id: '1', symbol: 'BTC_USDT_PERP', type: OrderType.Limit, side: OrderSide.Buy, amount: 1,
      filled: 0.2, remaining: 0.8, price: 100, reduceOnly: false, status: OrderStatus.Open, timeInForce: OrderTimeInForce.GoodTillCancel } as any;
    await e.updateOrder({ order, update: { price: 101 } });
    expect(e.placeOrder).toHaveBeenCalledWith(expect.objectContaining({ amount: 0.8, price: 101 }));
  });
});
