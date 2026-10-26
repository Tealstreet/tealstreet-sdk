import { afterEach, describe, expect, it, vi } from 'vitest';

import { MexcExchange } from '../reference/mexc/mexc.exchange';
import { ENDPOINTS } from '../reference/mexc/mexc.types';
import { normalizeMarket, normalizeOrder } from '../reference/mexc/mexc.utils';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderStopTriggerType, OrderType, PlaceOrderOpts } from '../../types';

const market = normalizeMarket({
  symbol: 'BTC_USDT', baseCoin: 'BTC', quoteCoin: 'USDT', settleCoin: 'USDT', state: 0, apiAllowed: true,
  contractSize: 0.0001, volUnit: 1, priceUnit: 0.1, minVol: 1, maxVol: 400000, minLeverage: 1, maxLeverage: 500,
});
const wireOrder = (state = 2, dealVol = 0) => ({
  orderId: 'original', symbol: 'BTC_USDT', side: 1, orderType: 1, vol: 60, dealVol, price: 60000, state,
});
const ok = (data?: unknown) => ({ data: { success: true, code: 0, data } });
const opts = (): PlaceOrderOpts => ({ symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 60, price: 60000 });
const setup = () => {
  const exchange = new MexcExchange('mexc', { key: 'key', secret: 'secret' }, new DefaultStore());
  const order = normalizeOrder(wireOrder(), market)!;
  exchange.store.update({ markets: [market], orders: [order] });
  return { exchange, order };
};

// Contract oracles: mexc/CLAUDE.md native volume, TP/SL, cancellation and
// cancel/replace semantics; package CLAUDE.md failed-read preservation.
describe('MEXC trading reference safety', () => {
  afterEach(() => vi.useRealTimers());

  it('sends attached bracket prices and independent reference trends on regular entries', async () => {
    const { exchange } = setup();
    const post = vi.fn().mockResolvedValue(ok('entry'));
    exchange.xhr = { post } as any;
    await exchange.placeOrder({ ...opts(), stopLoss: 55000.04, takeProfit: 65000.04,
      slTriggerType: OrderStopTriggerType.MarkPrice, tpTriggerType: OrderStopTriggerType.IndexPrice });
    expect(post).toHaveBeenCalledWith(ENDPOINTS.ORDER_SUBMIT, expect.objectContaining({
      vol: 60, stopLossPrice: '55000', takeProfitPrice: '65000', lossTrend: 2, profitTrend: 3,
    }));
  });

  it.each([OrderType.Conditional, OrderType.TrailingStopLoss])('rejects an unsupported %s anywhere in a batch before writes', async (type) => {
    const { exchange } = setup();
    const post = vi.fn();
    exchange.xhr = { post } as any;
    await expect(exchange.placeOrders([opts(), { ...opts(), type }])).rejects.toThrow('not implemented');
    await expect(exchange.placeOrder({ ...opts(), type })).rejects.toThrow('not implemented');
    expect(post).not.toHaveBeenCalled();
  });

  it('uses long leverage for a sell stop without a caller reduce-only flag', async () => {
    const { exchange } = setup();
    const get = vi.fn().mockResolvedValue(ok([
      { positionType: 1, leverage: 7, openType: 2 }, { positionType: 2, leverage: 15, openType: 2 },
    ]));
    const post = vi.fn().mockResolvedValue(ok('stop'));
    exchange.xhr = { post, get } as any;
    await exchange.fetchLeverageForSymbol('BTCUSDT');
    vi.useFakeTimers();
    await exchange.placeOrder({ ...opts(), type: OrderType.StopLoss, side: OrderSide.Sell, price: 55000 });
    expect(post).toHaveBeenCalledWith(ENDPOINTS.PLAN_ORDER_PLACE, expect.objectContaining({ side: 4, leverage: 7, reduceOnly: true }));
    exchange.dispose();
  });

  it('cancels all three native scopes and preserves a completion arriving before the cancel acknowledgement', async () => {
    const { exchange, order } = setup();
    const plan = { ...order, id: 'plan', type: OrderType.StopLoss, side: OrderSide.Sell };
    const stop = { ...plan, id: 'position-stop', amount: 0, remaining: 0 };
    exchange.store.update({ orders: [order, plan, stop] });
    const post = vi.fn(async (path: string) => {
      if (path === ENDPOINTS.ORDER_CANCEL_ALL) exchange.store.addOrUpdateOrder({ ...order, status: OrderStatus.Closed, filled: order.amount });
      return ok();
    });
    exchange.xhr = { post } as any;
    await exchange.cancelSymbolOrders('BTCUSDT');
    expect(post.mock.calls.map(([path]) => path)).toEqual([
      ENDPOINTS.ORDER_CANCEL_ALL, '/api/v1/private/planorder/cancel_all', '/api/v1/private/stoporder/cancel_all',
    ]);
    expect(exchange.store.orders.map(({ status }) => status)).toEqual([OrderStatus.Closed, OrderStatus.Canceled, OrderStatus.Canceled]);
  });

  it('commits only acknowledged cancel-all scopes when a later common envelope rejects', async () => {
    const { exchange, order } = setup();
    const plan = { ...order, id: 'plan', type: OrderType.StopLoss };
    exchange.store.update({ orders: [order, plan] });
    const post = vi.fn().mockResolvedValueOnce(ok()).mockResolvedValueOnce({ data: { success: false, code: 600 } });
    exchange.xhr = { post } as any;
    await expect(exchange.cancelSymbolOrders('BTCUSDT')).rejects.toThrow('failed');
    expect(exchange.store.orders.find(({ id }) => id === 'original')?.status).toBe(OrderStatus.Canceled);
    expect(exchange.store.orders.find(({ id }) => id === 'plan')?.status).toBe(OrderStatus.Open);
    expect(post).toHaveBeenCalledTimes(2);
  });

  it.each([undefined, 40])('uses a definitive post-cancel quantity while retaining explicit amount intent (%s)', async (amount) => {
    const { exchange, order } = setup();
    const get = vi.fn().mockResolvedValue(ok(wireOrder(4, 30)));
    const post = vi.fn().mockResolvedValueOnce(ok([{ orderId: 'original', errorCode: 0 }])).mockResolvedValueOnce(ok('replacement'));
    exchange.xhr = { post, get } as any;
    await exchange.updateOrder({ order, update: amount === undefined ? { price: 59000 } : { amount } });
    expect(get).toHaveBeenCalledWith('/api/v1/private/order/get/original');
    expect(post).toHaveBeenLastCalledWith(ENDPOINTS.ORDER_SUBMIT, expect.objectContaining({ vol: amount ?? 30 }));
    expect(exchange.store.orders.find(({ id }) => id === 'original')?.filled).toBeCloseTo(0.003);
  });

  it('commits a definitive fill received after the cancel response and withholds replacement', async () => {
    const { exchange, order } = setup();
    const get = vi.fn().mockResolvedValue(ok(wireOrder(3, 60)));
    const post = vi.fn().mockResolvedValue(ok([{ orderId: 'original', errorCode: 0 }]));
    exchange.xhr = { post, get } as any;
    await expect(exchange.updateOrder({ order, update: { price: 59000 } })).rejects.toThrow('filled during amend');
    expect(post).toHaveBeenCalledTimes(1);
    expect(exchange.store.orders[0]).toMatchObject({ status: OrderStatus.Closed, filled: 0.006 });
  });

  it.each([undefined, 40])('withholds replacement when the final read trails a newer partial fill (%s)', async (amount) => {
    const { exchange, order } = setup();
    const newer = normalizeOrder(wireOrder(2, 40), market)!;
    const post = vi.fn(async () => {
      exchange.store.addOrUpdateOrder(newer);
      return ok([{ orderId: 'original', errorCode: 0 }]);
    });
    const get = vi.fn().mockResolvedValue(ok(wireOrder(4, 30)));
    exchange.xhr = { post, get } as any;
    await expect(exchange.updateOrder({ order, update: amount === undefined ? { price: 59000 } : { amount } }))
      .rejects.toThrow('regressed cumulative fills');
    expect(post).toHaveBeenCalledTimes(1);
    expect(exchange.store.orders[0]).toMatchObject({ ...newer, status: OrderStatus.Canceled });
    expect(exchange.store.orders[0].filled).toBeCloseTo(0.004);
  });

  it('keeps delayed partial fills after cancel acknowledgement without reopening or regressing on older terminal frames', async () => {
    const { exchange, order } = setup();
    const post = vi.fn().mockResolvedValue(ok([{ orderId: 'original', errorCode: 0 }]));
    const frame = (state: number, dealVol: number) => exchange.privateWebsocket.onMessage({
      data: JSON.stringify({ channel: 'push.personal.order', data: wireOrder(state, dealVol) }),
    } as MessageEvent);
    const get = vi.fn(async () => {
      frame(2, 40);
      expect(exchange.store.orders[0]).toMatchObject({ status: OrderStatus.Canceled, filled: 0.004,
        extra: { remainingNative: 20, filledInPlacementUnits: 40 } });
      frame(4, 30);
      return ok(wireOrder(4, 30));
    });
    exchange.xhr = { post, get } as any;
    await expect(exchange.updateOrder({ order, update: { price: 59000 } })).rejects.toThrow('regressed cumulative fills');
    expect(post).toHaveBeenCalledTimes(1);
    expect(exchange.store.orders[0]).toMatchObject({ status: OrderStatus.Canceled, filled: 0.004 });
    frame(3, 60);
    frame(4, 60);
    expect(exchange.store.orders[0]).toMatchObject({ status: OrderStatus.Closed, filled: 0.006 });
  });

  it('keeps cumulative open fills monotonic across individual and batched frames', () => {
    const { exchange } = setup();
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.order', data: [
      wireOrder(2, 40), wireOrder(2, 30),
    ] }) } as MessageEvent);
    expect(exchange.store.orders[0]).toMatchObject({ status: OrderStatus.Open, filled: 0.004,
      extra: { remainingNative: 20, filledInPlacementUnits: 40 } });
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.order', data: wireOrder(2, 20) }) } as MessageEvent);
    expect(exchange.store.orders[0].filled).toBe(0.004);
  });

  it('promotes a canceled row to Closed while preserving its larger known fill quantity', () => {
    const { exchange } = setup();
    exchange.store.addOrUpdateOrder(normalizeOrder(wireOrder(4, 40), market)!);
    exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.order', data: wireOrder(3, 30) }) } as MessageEvent);
    expect(exchange.store.orders[0]).toMatchObject({ status: OrderStatus.Closed, filled: 0.004,
      extra: { remainingNative: 20, filledInPlacementUnits: 40 } });
  });

  it('deduplicates execution IDs without collapsing distinct or unidentified fills on one order', () => {
    const { exchange } = setup();
    const fill = vi.fn();
    exchange.on('fill', fill);
    const frame = (id?: string) => exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.order.deal', data: {
      id, orderId: 'original', symbol: 'BTC_USDT', side: 1, price: 60000, vol: 10,
    } }) } as MessageEvent);
    frame('execution-1');
    frame('execution-1');
    frame('execution-2');
    expect(fill).toHaveBeenCalledTimes(2);
    frame();
    frame();
    expect(fill).toHaveBeenCalledTimes(4);
  });

  it('expires execution deduplication entries after five minutes', () => {
    vi.useFakeTimers();
    const { exchange } = setup();
    const fill = vi.fn();
    exchange.on('fill', fill);
    const frame = () => exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.order.deal', data: {
      tradeId: 'execution-1', orderId: 'original', symbol: 'BTC_USDT', side: 1, price: 60000, vol: 10,
    } }) } as MessageEvent);
    frame();
    vi.setSystemTime(Date.now() + 299_999);
    frame();
    expect(fill).toHaveBeenCalledTimes(1);
    vi.setSystemTime(Date.now() + 300_001);
    frame();
    expect(fill).toHaveBeenCalledTimes(2);
  });

  it('withholds a stop replacement when its execution frame beats the cancel acknowledgement', async () => {
    const { exchange, order } = setup();
    const plan = { ...order, type: OrderType.StopLoss, side: OrderSide.Sell, reduceOnly: true };
    exchange.store.update({ orders: [plan] });
    const post = vi.fn(async () => {
      exchange.privateWebsocket.onMessage({ data: JSON.stringify({ channel: 'push.personal.plan.order', data: {
        id: 'original', symbol: 'BTC_USDT', side: 4, vol: 60, state: 3, triggerPrice: 55000, triggerType: 2,
      } }) } as MessageEvent);
      return ok();
    });
    const get = vi.fn();
    exchange.xhr = { post, get } as any;
    await expect(exchange.updateOrder({ order: plan, update: { price: 54000 } })).rejects.toThrow('filled during amend');
    expect(post).toHaveBeenCalledTimes(1);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.PLAN_ORDER_CANCEL, [{ symbol: 'BTC_USDT', orderId: 'original' }]);
    expect(get).not.toHaveBeenCalled();
    expect(exchange.store.orders[0].status).toBe(OrderStatus.Closed);
  });

  it('bounds inconclusive reads and never retries the cancellation write', async () => {
    vi.useFakeTimers();
    const { exchange, order } = setup();
    const get = vi.fn().mockResolvedValue(ok(wireOrder()));
    const post = vi.fn().mockResolvedValue(ok([{ orderId: 'original', errorCode: 0 }]));
    exchange.xhr = { post, get } as any;
    const pending = expect(exchange.updateOrder({ order, update: { price: 59000 } })).rejects.toThrow('not authoritative');
    await vi.advanceTimersByTimeAsync(1000);
    await pending;
    expect(get).toHaveBeenCalledTimes(6);
    expect(post).toHaveBeenCalledTimes(1);
  });

  it.each(['network', 'invalid', 'zero remaining', 'newer filled'])('withholds replacement after an unsafe final read (%s)', async (condition) => {
    const { exchange, order } = setup();
    const post = vi.fn().mockResolvedValue(ok([{ orderId: 'original', errorCode: 0 }]));
    const get = vi.fn(async () => {
      if (condition === 'network') throw new Error('lost read');
      if (condition === 'newer filled') exchange.store.addOrUpdateOrder({ ...order, status: OrderStatus.Closed, filled: order.amount });
      return ok(wireOrder(condition === 'invalid' ? 5 : 4, condition === 'zero remaining' ? 60 : 30));
    });
    exchange.xhr = { post, get } as any;
    await expect(exchange.updateOrder({ order, update: { price: 59000 } })).rejects.toThrow();
    expect(post).toHaveBeenCalledTimes(1);
    if (condition === 'newer filled') expect(exchange.store.orders[0]).toMatchObject({ status: OrderStatus.Closed, filled: 0.006 });
  });

  it.each(['network', 'envelope', 'malformed'])('preserves last-known orders when the regular snapshot fails (%s)', async (failure) => {
    const { exchange, order } = setup();
    const get = failure === 'network' ? vi.fn().mockRejectedValue(new Error('lost response')) :
      vi.fn().mockResolvedValue(failure === 'envelope' ? { data: { success: false, code: 600 } } : ok({}));
    exchange.xhr = { get } as any;
    await expect(exchange.fetchAndSetRegularOrders()).rejects.toThrow();
    expect(exchange.store.orders).toEqual([order]);
  });
});
