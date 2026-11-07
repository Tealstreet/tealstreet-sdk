import { describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { WeexExchange } from '../reference/weex/weex.exchange';
import { ENDPOINTS } from '../reference/weex/weex.types';
import { normalizeAlgoOrder } from '../reference/weex/weex.utils';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStopTriggerType, OrderType } from '../../types';

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
  const post = vi.fn().mockResolvedValue({ data: { success: true, orderId: 'algo-1' } });
  const remove = vi.fn().mockResolvedValue({ data: { success: true } });
  exchange.xhr = { post, delete: remove } as any;
  return { exchange, post, remove };
};

describe('audit regression: WEEX opening conditional orders', () => {
  it('advertises opening triggers only after the lifecycle is wired', () => {
    expect(getExchangeMetadata('weex').supportsOpeningTriggerOrders).toBe(true);
  });

  it('places a market opening trigger through the algo endpoint and forces opening semantics', async () => {
    const { exchange, post } = createExchange();

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 0.1254,
        triggerPrice: 60_123.16,
        reduceOnly: true,
      }),
    ).resolves.toEqual(['algo-1']);

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ALGO_ORDER,
      expect.objectContaining({
        symbol: 'BTCUSDT',
        side: 'BUY',
        positionSide: 'LONG',
        type: 'STOP_MARKET',
        quantity: '0.125',
        triggerPrice: '60123.2',
        clientAlgoId: expect.stringMatching(/^ts/),
      }),
    );
    expect(post.mock.calls[0][1]).not.toHaveProperty('reduceOnly');
    expect(post.mock.calls[0][1]).not.toHaveProperty('newClientOrderId');
  });

  it('places a limit opening trigger with attached protective presets', async () => {
    const { exchange, post } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Conditional,
      side: OrderSide.Sell,
      amount: 0.25,
      price: 59_900,
      triggerPrice: 60_000,
      takeProfit: 58_000,
      stopLoss: 61_000,
      tpTriggerType: OrderStopTriggerType.MarkPrice,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ALGO_ORDER,
      expect.objectContaining({
        type: 'STOP',
        price: '59900',
        triggerPrice: '60000',
        presetTakeProfitPrice: '58000',
        presetStopLossPrice: '61000',
        TpWorkingType: 'MARK_PRICE',
        SlWorkingType: 'CONTRACT_PRICE',
      }),
    );
  });

  it.each([undefined, 0, -1, Number.NaN])('emits and rejects invalid opening trigger price %s', async (triggerPrice) => {
    const { exchange, post } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);

    await expect(
      exchange.placeOrder({
        symbol: 'BTCUSDT',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 1,
        triggerPrice,
      }),
    ).rejects.toThrow('Trigger price is required for WEEX opening conditional orders');
    expect(emitAndLogError).toHaveBeenCalledWith('Trigger price is required for WEEX opening conditional orders');
    expect(post).not.toHaveBeenCalled();
  });

  it('maps REST and websocket opening trigger shapes as conditionals while keeping TP/SL protective', () => {
    const opening = {
      algoId: 'algo-1',
      algoType: 'CONDITIONAL',
      orderType: 'STOP_MARKET',
      symbol: 'BTCUSDT',
      side: 'BUY',
      positionSide: 'LONG',
      quantity: '0.1',
      triggerPrice: '60000',
      algoStatus: 'UNTRIGGERED',
    };
    const [restOrder] = normalizeAlgoOrder(opening);
    const [websocketOrder] = normalizeAlgoOrder({ ...opening, algoType: 'PLAN', orderType: 'STOP' });
    const [protectiveOrder] = normalizeAlgoOrder({
      ...opening,
      algoId: 'sl-1',
      planType: 'STOP_LOSS',
      orderType: 'STOP_MARKET',
      reduceOnly: true,
    });
    const [protectiveTakeProfit] = normalizeAlgoOrder({
      ...opening,
      algoId: 'tp-1',
      planType: 'TAKE_PROFIT',
      orderType: 'STOP_MARKET',
      reduceOnly: true,
    });

    expect(restOrder.type).toBe(OrderType.Conditional);
    expect(restOrder.price).toBe(60_000);
    expect(websocketOrder.type).toBe(OrderType.Conditional);
    expect(protectiveOrder.type).toBe(OrderType.StopLoss);
    expect(protectiveTakeProfit.type).toBe(OrderType.TakeProfit);
  });

  it('cancel-replaces a trigger edit through algo endpoints while preserving its execution price', async () => {
    const { exchange, post, remove } = createExchange();
    const [order] = normalizeAlgoOrder({
      algoId: 'algo-2',
      algoType: 'CONDITIONAL',
      orderType: 'STOP',
      symbol: 'BTCUSDT',
      side: 'BUY',
      positionSide: 'LONG',
      quantity: '0.2',
      price: '59900',
      triggerPrice: '60000',
      algoStatus: 'UNTRIGGERED',
    });

    await exchange.updateOrder({ order, update: { price: 60_100 } });

    expect(remove).toHaveBeenCalledWith(ENDPOINTS.ALGO_ORDER, { params: { orderId: 'algo-2' } });
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ALGO_ORDER,
      expect.objectContaining({ type: 'STOP', price: '59900', triggerPrice: '60100' }),
    );
  });
});
