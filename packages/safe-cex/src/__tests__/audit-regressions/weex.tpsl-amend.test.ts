import { describe, expect, it, vi } from 'vitest';

import { WeexExchange } from '../reference/weex/weex.exchange';
import { ENDPOINTS } from '../reference/weex/weex.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType } from '../../types';

const createExchange = () => {
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
      },
    ] as any,
  });
  const exchange = new WeexExchange(
    'weex' as any,
    { key: 'key', secret: 'secret', passphrase: 'passphrase' } as any,
    store,
  );
  const post = vi.fn().mockResolvedValue({ data: { success: true } });
  const remove = vi.fn().mockResolvedValue({ data: { success: true } });
  exchange.xhr = { post, delete: remove } as any;
  return { exchange, post, remove };
};

const order = (overrides: Record<string, any> = {}) =>
  ({
    id: 'sl-1',
    status: OrderStatus.Open,
    symbol: 'BTCUSDT',
    type: OrderType.StopLoss,
    side: OrderSide.Sell,
    price: 59_000,
    amount: 0.2,
    filled: 0,
    remaining: 0.2,
    reduceOnly: true,
    timeInForce: OrderTimeInForce.GoodTillCancel,
    lastUpdate: 0,
    slTriggerType: OrderStopTriggerType.MarkPrice,
    info: { executePrice: '0' },
    ...overrides,
  }) as any;

describe('audit regression: WEEX TP/SL amendment lifecycle', () => {
  it('amends a direct protective order without cancel-and-replace', async () => {
    const { exchange, post, remove } = createExchange();

    await expect(exchange.updateOrder({ order: order(), update: { price: 58_999.96 } })).resolves.toEqual(['sl-1']);

    expect(post).toHaveBeenCalledWith(ENDPOINTS.MODIFY_TP_SL_ORDER, {
      orderId: 'sl-1',
      triggerPrice: '59000',
      executePrice: '0',
      triggerPriceType: 'MARK_PRICE',
    });
    expect(remove).not.toHaveBeenCalled();
  });

  it('fails a virtual child edit closed when only the parent order ID is available', async () => {
    const { exchange, post, remove } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    const virtualTakeProfit = order({
      id: 'parent-algo-1__tp',
      parentId: 'parent-algo-1',
      type: OrderType.TakeProfit,
      price: 62_000,
      tpTriggerType: OrderStopTriggerType.LastPrice,
      slTriggerType: undefined,
      info: { tpPrice: '62100' },
    });

    await expect(exchange.updateOrder({ order: virtualTakeProfit, update: { price: 62_500 } })).rejects.toThrow(
      'WEEX virtual TP/SL amendment requires the protective order ID',
    );

    expect(emitAndLogError).toHaveBeenCalledWith(
      'WEEX virtual TP/SL amendment requires the protective order ID',
    );
    expect(post).not.toHaveBeenCalled();
    expect(remove).not.toHaveBeenCalled();
  });

  it('fails TP/SL quantity amendments closed instead of deleting protection', async () => {
    const { exchange, post, remove } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(exchange.updateOrder({ order: order(), update: { amount: 0.1 } })).rejects.toThrow(
      'WEEX TP/SL quantity amendment is not supported',
    );
    expect(emitAndLogError).toHaveBeenCalledWith('WEEX TP/SL quantity amendment is not supported');
    expect(post).not.toHaveBeenCalled();
    expect(remove).not.toHaveBeenCalled();
  });
});
