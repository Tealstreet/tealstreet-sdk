import { afterEach, describe, expect, it, vi } from 'vitest';

import { BitrueExchange } from '../reference/bitrue/bitrue.exchange';
import { DefaultStore } from '../../store/store.base';

const symbol = 'E-BTC-USDT';
const row = {
  orderId: '2334151155877155281',
  clientOrderId: 'caller-id',
  price: 59000,
  origQty: 0.004,
  executedQty: 0.001,
  status: 'PART_FILLED',
  type: 'LIMIT',
  side: 'BUY',
  action: 'OPEN',
  transactTime: 1700000000123,
};
const make = () => {
  vi.useFakeTimers();
  const store = new DefaultStore();
  const exchange = new BitrueExchange('bitrue', { key: 'key', secret: 'secret' }, store);
  vi.spyOn(exchange as any, 'emitAndLogError').mockImplementation(() => undefined);
  const get = vi.fn().mockResolvedValue({ data: { code: '0', data: [row] } });
  (exchange as any).xhr.get = get;
  return { exchange, store, get };
};
afterEach(() => vi.useRealTimers());

describe('Bitrue current TS parity contract', () => {
  it('does not declare complete order, PnL or funding histories', () => {
    const { exchange, get } = make();
    expect(exchange.capabilities).toMatchObject({
      orderHistory: false,
      closedPnl: false,
      incomeHistory: false,
      incomeRange: false,
    });
    expect(get).not.toHaveBeenCalled();
  });
  it('does not support hedge or margin-mode changes', async () => {
    const { exchange, get } = make();
    await expect(exchange.changePositionMode(true)).rejects.toThrow('Not implemented');
    await expect(exchange.changeMarginType(true, symbol)).rejects.toThrow('API does not support it');
    expect(get).not.toHaveBeenCalled();
  });
  it('looks up the caller ID using only symbol-scoped native open orders', async () => {
    const { exchange, store, get } = make();
    const found = await exchange.findOrderByClientOrderId(symbol, 'caller-id');
    expect(found).toMatchObject({ found: true, order: { id: row.orderId, symbol, clientOrderId: 'caller-id' } });
    expect(get).toHaveBeenCalledWith('/fapi/v2/openOrders', { params: { contractName: symbol } });
    expect(store.orders).toEqual([]);
    await expect(exchange.findOrderByClientOrderId(symbol, 'missing')).resolves.toEqual({
      found: false,
      definitive: false,
    });
  });
  it('checks exact cached symbol and client identity without reading', async () => {
    const { exchange, store, get } = make();
    const orders = await exchange.fetchOrdersForSymbol(symbol);
    store.addOrUpdateOrders(orders);
    get.mockClear();
    await expect(exchange.findOrderByClientOrderId(symbol, 'caller-id')).resolves.toMatchObject({
      found: true,
      order: { id: row.orderId },
    });
    expect(get).not.toHaveBeenCalled();
    await expect(exchange.findOrderByClientOrderId(symbol, row.orderId)).resolves.toMatchObject({
      found: true,
      order: { id: row.orderId },
    });
    expect(get).not.toHaveBeenCalled();
  });
  it('does not turn failed open-order reads into definitive absence', async () => {
    const { exchange, get } = make();
    get.mockRejectedValue(new Error('network'));
    await expect(exchange.findOrderByClientOrderId(symbol, 'caller-id')).resolves.toEqual({
      found: false,
      definitive: false,
    });
  });
});
