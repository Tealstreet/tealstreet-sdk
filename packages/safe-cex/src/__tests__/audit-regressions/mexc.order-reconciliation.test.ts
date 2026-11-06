import { afterEach, describe, expect, it, vi } from 'vitest';

import { MexcExchange } from '../reference/mexc/mexc.exchange';
import { ENDPOINTS } from '../reference/mexc/mexc.types';
import { normalizeMarket, normalizeOrder, normalizePosition } from '../reference/mexc/mexc.utils';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PositionMode } from '../../types';

const market = normalizeMarket({
  symbol: 'BTC_USDT', baseCoin: 'BTC', quoteCoin: 'USDT', settleCoin: 'USDT', state: 0, apiAllowed: true,
  contractSize: 0.0001, volUnit: 1, priceUnit: 0.1, minVol: 1, maxVol: 400000, minLeverage: 1, maxLeverage: 500,
});
const wireOrder = (state = 2, dealVol = 0) => ({
  orderId: 'original', symbol: 'BTC_USDT', side: 1, orderType: 1, vol: 60, dealVol, price: 60000, state,
});
const ok = (data?: unknown) => ({ data: { success: true, code: 0, data } });
const setup = () => {
  const exchange = new MexcExchange('mexc', { key: 'key', secret: 'secret' }, new DefaultStore());
  exchange.store.update({ markets: [market] });
  return exchange;
};
const brackets = { stopLossPrice: '55000', takeProfitPrice: '65000', lossTrend: 2, profitTrend: 3 };

// MEXC native sizes and independent bracket references follow mexc/CLAUDE.md.
// A REST snapshot cannot undo cumulative executions already observed on WS.
describe('MEXC snapshot and bracket reconciliation', () => {
  afterEach(() => vi.useRealTimers());
  it.each([
    ['full-account', false, undefined],
    ['full-symbol', false, ['BTCUSDT']],
    ['regular-account', true, undefined],
    ['regular-symbol', true, ['BTCUSDT']],
  ] as const)('preserves newer fills and terminal status through the actual %s fetch path', async (_name, regularOnly, symbols) => {
    const exchange = setup();
    let state = 2;
    let filled = 10;
    const get = vi.fn(async (path: string) => {
      if (!path.startsWith(ENDPOINTS.OPEN_ORDERS)) return ok([]);
      exchange.privateWebsocket.onMessage({ data: JSON.stringify({
        channel: 'push.personal.order', data: wireOrder(state, filled),
      }) } as MessageEvent);
      return ok([wireOrder(2, 0)]);
    });
    exchange.xhr = { get } as any;
    for (const [nextState, nextFilled, expectedStatus] of [
      [2, 10, OrderStatus.Open], [4, 15, OrderStatus.Canceled], [3, 60, OrderStatus.Closed],
    ] as const) {
      state = nextState;
      filled = nextFilled;
      const rows = regularOnly ? await exchange.fetchAndSetRegularOrders(symbols ? [...symbols] : undefined) :
        await exchange.fetchAndSetOrders(symbols ? [...symbols] : undefined);
      const expected = {
        status: expectedStatus, filled: nextFilled * 0.0001,
        remaining: (60 - nextFilled) * 0.0001,
        extra: { remainingNative: 60 - nextFilled, filledInPlacementUnits: nextFilled },
      };
      expect(exchange.store.orders[0]).toMatchObject(expected);
      expect(rows[0]).toMatchObject(expected);
    }
    expect(get.mock.calls.some(([path]) => path.startsWith(ENDPOINTS.OPEN_ORDERS))).toBe(true);
  });

  it('normalizes both bracket prices and their independent native reference sources', () => {
    expect(normalizeOrder({ ...wireOrder(), ...brackets }, market)).toMatchObject({
      stopLoss: 55000, takeProfit: 65000,
      slTriggerType: OrderStopTriggerType.MarkPrice, tpTriggerType: OrderStopTriggerType.IndexPrice,
    });
    const cleared = normalizeOrder({ ...wireOrder(), stopLossPrice: 0, takeProfitPrice: 0 }, market)!;
    expect(cleared.stopLoss).toBeUndefined();
    expect(cleared.takeProfit).toBeUndefined();
  });

  it('preserves a fill arriving before placement acknowledgement and retains optimistic bracket metadata', async () => {
    const exchange = setup();
    const post = vi.fn(async () => {
      exchange.privateWebsocket.onMessage({ data: JSON.stringify({
        channel: 'push.personal.order', data: wireOrder(2, 10),
      }) } as MessageEvent);
      return ok('original');
    });
    exchange.xhr = { post } as any;
    await exchange.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 60, price: 60000,
      stopLoss: 55000.04, takeProfit: 65000.04,
      slTriggerType: OrderStopTriggerType.MarkPrice, tpTriggerType: OrderStopTriggerType.IndexPrice });
    expect(exchange.store.orders[0]).toMatchObject({
      filled: 0.001, remaining: 0.005, extra: { remainingNative: 50, filledInPlacementUnits: 10 },
      stopLoss: 55000, takeProfit: 65000,
      slTriggerType: OrderStopTriggerType.MarkPrice, tpTriggerType: OrderStopTriggerType.IndexPrice,
    });
  });

  it.each([0, 10])('retains native bracket clears and source corrections preceding the placement acknowledgement (filled=%s)', async (filled) => {
    const exchange = setup();
    const post = vi.fn().mockImplementationOnce(async () => {
      exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.order', data: {
        ...wireOrder(2, filled), stopLossPrice: 0, takeProfitPrice: 66000, profitTrend: 2,
      } }) } as MessageEvent);
      // A second sparse row must retain knowledge of the preceding explicit clear.
      exchange.privateWebsocket.onMessage({ data: JSON.stringify({
        channel: 'push.personal.order', data: wireOrder(2, filled),
      }) } as MessageEvent);
      return ok('original');
    }).mockResolvedValueOnce(ok([{ orderId: 'original', errorCode: 0 }])).mockResolvedValueOnce(ok('replacement'));
    exchange.xhr = { post, get: vi.fn().mockResolvedValue(ok(wireOrder(4, filled))) } as any;
    await exchange.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 60, price: 60000,
      stopLoss: 55000, takeProfit: 65000,
      slTriggerType: OrderStopTriggerType.MarkPrice, tpTriggerType: OrderStopTriggerType.IndexPrice });
    const current = exchange.store.orders[0];
    expect(current.stopLoss).toBeUndefined();
    expect(current).toMatchObject({ takeProfit: 66000, tpTriggerType: OrderStopTriggerType.MarkPrice,
      filled: filled * 0.0001, extra: { remainingNative: 60 - filled, filledInPlacementUnits: filled },
      info: { stopLossPrice: 0, takeProfitPrice: 66000, profitTrend: 2 } });
    expect(current.info).not.toHaveProperty('__mexcOptimisticRegular');
    await exchange.updateOrder({ order: current, update: { price: 59000 } });
    const payload = post.mock.calls[2][1];
    expect(payload).not.toHaveProperty('stopLossPrice');
    expect(payload).not.toHaveProperty('lossTrend');
    expect(payload).toMatchObject({ price: '59000', vol: 60 - filled, takeProfitPrice: '66000', profitTrend: 2 });
    exchange.dispose();
  });

  it.each([false, true])('reconciles placement → native WS → amend with explicit-clear=%s', async (clearStopLoss) => {
    const exchange = setup();
    const post = vi.fn().mockResolvedValueOnce(ok('original'))
      .mockResolvedValueOnce(ok([{ orderId: 'original', errorCode: 0 }])).mockResolvedValueOnce(ok('replacement'));
    const get = vi.fn().mockResolvedValue(ok(wireOrder(4, 10)));
    exchange.xhr = { post, get } as any;
    await exchange.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 60, price: 60000,
      stopLoss: 55000, takeProfit: 65000,
      slTriggerType: OrderStopTriggerType.MarkPrice, tpTriggerType: OrderStopTriggerType.IndexPrice });
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.order', data: {
      ...wireOrder(2, 10), ...(clearStopLoss ? { stopLossPrice: 0, takeProfitPrice: 66000, profitTrend: 2 } : {}),
    } }) } as MessageEvent);
    const current = exchange.store.orders[0];
    expect(current.stopLoss).toBe(clearStopLoss ? undefined : 55000);
    expect(current).toMatchObject({ takeProfit: clearStopLoss ? 66000 : 65000,
      slTriggerType: OrderStopTriggerType.MarkPrice,
      tpTriggerType: clearStopLoss ? OrderStopTriggerType.MarkPrice : OrderStopTriggerType.IndexPrice });
    await exchange.updateOrder({ order: current, update: { price: 59000 } });
    const payload = post.mock.calls[2][1];
    expect(payload).toMatchObject({ price: '59000', vol: 50,
      takeProfitPrice: clearStopLoss ? '66000' : '65000', profitTrend: clearStopLoss ? 2 : 3 });
    if (clearStopLoss) {
      expect(payload).not.toHaveProperty('stopLossPrice');
      expect(payload).not.toHaveProperty('lossTrend');
    } else expect(payload).toMatchObject({ stopLossPrice: '55000', lossTrend: 2 });
    exchange.dispose();
  });

  it('retains the generic trigger source when it supplied both attached bracket references', async () => {
    const exchange = setup();
    const post = vi.fn().mockResolvedValueOnce(ok('original'))
      .mockResolvedValueOnce(ok([{ orderId: 'original', errorCode: 0 }])).mockResolvedValueOnce(ok('replacement'));
    exchange.xhr = { post, get: vi.fn().mockResolvedValue(ok(wireOrder(4, 0))) } as any;
    await exchange.placeOrder({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 60, price: 60000,
      stopLoss: 55000, takeProfit: 65000, triggerType: OrderStopTriggerType.MarkPrice });
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.order', data: wireOrder() }) } as MessageEvent);
    expect(exchange.store.orders[0]).toMatchObject({ slTriggerType: OrderStopTriggerType.MarkPrice, tpTriggerType: OrderStopTriggerType.MarkPrice });
    await exchange.updateOrder({ order: exchange.store.orders[0], update: { price: 59000 } });
    expect(post).toHaveBeenLastCalledWith(ENDPOINTS.ORDER_SUBMIT, expect.objectContaining({ lossTrend: 2, profitTrend: 2 }));
    exchange.dispose();
  });

  it.each(['price', 'amount'] as const)('retains brackets on a %s-only amend, using updated native fields from the definitive read', async (field) => {
    const exchange = setup();
    const original = normalizeOrder({ ...wireOrder(), ...brackets }, market)!;
    exchange.store.update({ orders: [original] });
    const updated = { stopLossPrice: '56000', takeProfitPrice: '66000', lossTrend: 3, profitTrend: 2 };
    const get = vi.fn().mockResolvedValue(ok({ ...wireOrder(4, 10), ...updated }));
    const post = vi.fn().mockResolvedValueOnce(ok([{ orderId: 'original', errorCode: 0 }])).mockResolvedValueOnce(ok('replacement'));
    exchange.xhr = { post, get } as any;
    await exchange.updateOrder({ order: original, update: field === 'price' ? { price: 59000 } : { amount: 40 } });
    expect(get).toHaveBeenCalledWith('/api/v1/private/order/get/original');
    expect(post).toHaveBeenLastCalledWith(ENDPOINTS.ORDER_SUBMIT, expect.objectContaining({
      ...updated, price: field === 'price' ? '59000' : '60000', vol: field === 'price' ? 50 : 40,
    }));
    expect(exchange.store.orders.find(({ id }) => id === 'replacement')).toMatchObject({
      stopLoss: 56000, takeProfit: 66000,
      slTriggerType: OrderStopTriggerType.IndexPrice, tpTriggerType: OrderStopTriggerType.MarkPrice,
    });
  });

  it('retains known bracket references when the final read omits those native fields', async () => {
    const exchange = setup();
    const original = normalizeOrder({ ...wireOrder(), ...brackets }, market)!;
    exchange.store.update({ orders: [original] });
    const get = vi.fn().mockResolvedValue(ok(wireOrder(4, 0)));
    const post = vi.fn().mockResolvedValueOnce(ok([{ orderId: 'original', errorCode: 0 }])).mockResolvedValueOnce(ok('replacement'));
    exchange.xhr = { post, get } as any;
    await exchange.updateOrder({ order: original, update: { price: 59000 } });
    expect(post).toHaveBeenLastCalledWith(ENDPOINTS.ORDER_SUBMIT, expect.objectContaining(brackets));
  });

  it('honors an explicitly cleared stop-loss while carrying the updated take-profit', async () => {
    const exchange = setup();
    const original = normalizeOrder({ ...wireOrder(), ...brackets }, market)!;
    exchange.store.update({ orders: [original] });
    const get = vi.fn().mockResolvedValue(ok({ ...wireOrder(4, 0), stopLossPrice: 0, takeProfitPrice: '67000', profitTrend: 1 }));
    const post = vi.fn().mockResolvedValueOnce(ok([{ orderId: 'original', errorCode: 0 }])).mockResolvedValueOnce(ok('replacement'));
    exchange.xhr = { post, get } as any;
    await exchange.updateOrder({ order: original, update: { amount: 30 } });
    const payload = post.mock.calls[1][1];
    expect(payload).not.toHaveProperty('stopLossPrice');
    expect(payload).not.toHaveProperty('lossTrend');
    expect(payload).toMatchObject({ takeProfitPrice: '67000', profitTrend: 1, vol: 30 });
  });

  it.each([
    [OrderType.StopLoss, OrderStopTriggerType.MarkPrice, 2, 2, true],
    [OrderType.TakeProfit, OrderStopTriggerType.IndexPrice, 3, 1, true],
    [OrderType.StopLoss, OrderStopTriggerType.MarkPrice, 2, 2, false],
    [OrderType.TakeProfit, OrderStopTriggerType.IndexPrice, 3, 1, false],
  ] as const)('retains %s reference through optimistic placement, a native plan frame, and a price amend (trend present=%s)', async (type, source, trend, triggerType, hasTrend) => {
    vi.useFakeTimers();
    const exchange = setup();
    const post = vi.fn().mockResolvedValueOnce(ok('plan')).mockResolvedValueOnce(ok()).mockResolvedValueOnce(ok('replacement'));
    exchange.xhr = { post } as any;
    await exchange.placeOrder({ symbol: 'BTCUSDT', type, side: OrderSide.Sell, amount: 10, price: 55000,
      slTriggerType: type === OrderType.StopLoss ? source : undefined,
      tpTriggerType: type === OrderType.TakeProfit ? source : undefined });
    expect(exchange.store.orders[0].triggerType).toBe(source);
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.plan.order', data: {
      id: 'plan', symbol: 'BTC_USDT', side: 4, state: 1, vol: 10, triggerPrice: 55000, triggerType, ...(hasTrend ? { trend } : {}),
    } }) } as MessageEvent);
    const plan = exchange.store.orders.find(({ id }) => id === 'plan')!;
    expect(type === OrderType.StopLoss ? plan.slTriggerType : plan.tpTriggerType).toBe(source);
    await exchange.updateOrder({ order: plan, update: { price: 54000 } });
    expect(post).toHaveBeenLastCalledWith(ENDPOINTS.PLAN_ORDER_PLACE, expect.objectContaining({ trend, triggerPrice: '54000', vol: 10 }));
    expect(exchange.store.orders.find(({ id }) => id === 'replacement')?.triggerType).toBe(source);
    exchange.dispose();
  });

  it.each([
    [OrderType.StopLoss, 2, OrderStopTriggerType.MarkPrice, 'lossTrend', 2],
    [OrderType.TakeProfit, 1, OrderStopTriggerType.IndexPrice, 'profitTrend', 3],
  ] as const)('retains %s position-stop references and full-position sizing across a native frame and amend', async (type, triggerSide, source, trendField, trend) => {
    vi.useFakeTimers();
    const exchange = setup();
    const nativePosition = '739113577038255616';
    exchange.store.update({ positions: [normalizePosition({
      positionId: nativePosition, symbol: 'BTC_USDT', positionType: 1, positionMode: 1,
      holdVol: 60, holdAvgPrice: 60000, leverage: 10, openType: 2,
    }, market, PositionMode.Hedged)!] });
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.stop.order', data: {
      id: 'full', symbol: 'BTC_USDT', positionType: 1, positionId: nativePosition, state: 1,
      volType: 2, vol: 60, triggerSide, stopLossPrice: 55000, takeProfitPrice: 65000, lossTrend: 2, profitTrend: 3,
    } }) } as MessageEvent);
    const order = exchange.store.orders[0];
    expect(order).toMatchObject({ amount: 0, remaining: 0, triggerType: source,
      slTriggerType: OrderStopTriggerType.MarkPrice, tpTriggerType: OrderStopTriggerType.IndexPrice });
    const post = vi.fn().mockResolvedValueOnce(ok()).mockResolvedValueOnce(ok([{ id: 'replacement' }]));
    exchange.xhr = { post } as any;
    await exchange.updateOrder({ order, update: { price: 54000 } });
    expect(post).toHaveBeenLastCalledWith(ENDPOINTS.STOP_ORDER_PLACE, expect.objectContaining({
      positionId: nativePosition, vol: 60, volType: 2, [trendField]: trend,
      [type === OrderType.StopLoss ? 'stopLossPrice' : 'takeProfitPrice']: '54000',
    }));
    expect(exchange.store.orders.find(({ id }) => id === 'replacement')).toMatchObject({ amount: 0, triggerType: source });
    exchange.dispose();
  });
});
