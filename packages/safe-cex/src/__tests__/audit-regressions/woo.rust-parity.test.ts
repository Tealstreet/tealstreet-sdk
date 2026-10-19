import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../../exchanges/woo/woo.exchange';
import { splitWooQuantity } from '../../exchanges/woo/woo.order-state';
import { ENDPOINTS } from '../../exchanges/woo/woo.types';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  OrderSide,
  OrderStatus,
  OrderStopTriggerType,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const NOW = 1786603865910;
const instances: WOOXExchange[] = [];
const make = () => {
  const store = new DefaultStore();
  const exchange = new WOOXExchange(
    'woo',
    { key: 'fixture-key', secret: 'fixture-secret', applicationId: 'fixture' },
    store,
  );
  instances.push(exchange);
  store.update({
    markets: [
      {
        id: 'BTC/USDT:USDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
      },
    ],
    tickers: [{ id: 'BTC/USDT:USDT', symbol: 'BTCUSDT', last: 60000, mark: 60000, index: 60000 } as any],
  });
  vi.spyOn(exchange, 'getPositionMode').mockResolvedValue(PositionMode.OneWay);
  vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
  return { exchange, store };
};
const ordinary = (overrides: Record<string, unknown> = {}) => ({
  orderId: 100,
  symbol: 'PERP_BTC_USDT',
  side: 'BUY',
  orderType: 'LIMIT',
  orderPrice: 59000,
  orderQuantity: 0.01,
  totalExecutedQuantity: 0,
  status: 'NEW',
  ...overrides,
});
const row = (id: number) => ({ id, type: 'REALIZED_PNL', token: 'USDT', amount: 0.01, timestamp: NOW });

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});
afterEach(() => {
  for (const exchange of instances.splice(0)) exchange.dispose();
  vi.useRealTimers();
});

describe('WOO active protocol Rust parity repairs', () => {
  it('publishes the authoritative account position mode into the store', async () => {
    const { exchange, store } = make();
    vi.mocked(exchange.getPositionMode).mockRestore();
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({ data: { success: true, data: { positionMode: 'HEDGE_MODE' } } }),
    } as any;
    await exchange.getPositionMode();
    expect(store.options.isHedged).toBe(true);
  });

  it('keeps camel-case cumulative fills and strongest terminal status across stale frames', () => {
    const { exchange, store } = make();
    exchange.privateWebsocket.handleExecutionReport(
      ordinary({ totalExecutedQuantity: 0.002, status: 'PARTIAL_FILLED' }),
    );
    exchange.privateWebsocket.handleExecutionReport(ordinary({ totalExecutedQuantity: 0.001, status: 'REPLACED' }));
    expect(store.orders[0]).toMatchObject({ amount: 0.01, filled: 0.002, remaining: 0.008 });
    exchange.privateWebsocket.handleExecutionReport(ordinary({ totalExecutedQuantity: 0.003, status: 'FILLED' }));
    exchange.privateWebsocket.handleExecutionReport(ordinary({ totalExecutedQuantity: 0, status: 'NEW' }));
    exchange.privateWebsocket.handleExecutionReport(ordinary({ totalExecutedQuantity: 0.002, status: 'CANCELLED' }));
    expect(store.orders[0]).toMatchObject({ status: OrderStatus.Closed, filled: 0.003, remaining: 0.007 });
    expect(exchange.mapLimitOrder(ordinary())).toMatchObject({ status: OrderStatus.Closed, filled: 0.003 });
  });

  it('emits each ordinary execution once with numeric incremental size', () => {
    const { exchange } = make();
    const fills: any[] = [];
    exchange.on('fill', (fill: any) => fills.push(fill));
    for (const [tradeId, cumulative] of [
      [7001, 0.001],
      [7002, 0.002],
      [7002, 0.002],
      [7003, 0.003],
    ]) {
      exchange.privateWebsocket.handleExecutionReport(
        ordinary({
          status: 'PARTIAL_FILLED',
          tradeId,
          totalExecutedQuantity: cumulative,
          executedQuantity: '0.001',
          executedPrice: '59000',
          executedTimestamp: NOW,
        }),
      );
    }
    expect(fills.map(({ id, amount, price }) => ({ id, amount, price }))).toEqual([
      { id: '7001', amount: 0.001, price: 59000 },
      { id: '7002', amount: 0.001, price: 59000 },
      { id: '7003', amount: 0.001, price: 59000 },
    ]);
  });

  it('uses cumulative differences for algo fills without an execution ID', () => {
    const { exchange } = make();
    const fills: any[] = [];
    exchange.on('fill', (fill: any) => fills.push(fill));
    for (const totalExecutedQuantity of [0.001, 0.002, 0.002, 0.003]) {
      exchange.privateWebsocket.handleAlgoExecutionReport({
        algoOrderId: 200,
        rootAlgoOrderId: 200,
        symbol: 'PERP_BTC_USDT',
        side: 'SELL',
        quantity: 0.01,
        algoType: 'STOP',
        triggerPrice: 58000,
        algoStatus: 'PARTIAL_FILLED',
        totalExecutedQuantity,
        averageExecutedPrice: '58000',
      });
    }
    expect(fills).toHaveLength(3);
    expect(fills.map((f) => f.amount)).toEqual([0.001, 0.001, 0.001]);
  });

  it('accumulates individual executions when cumulative quantity is omitted', () => {
    const { exchange, store } = make();
    const fills: any[] = [];
    exchange.on('fill', (fill: any) => fills.push(fill));
    for (const tradeId of [1, 2, 2]) {
      exchange.privateWebsocket.handleExecutionReport(
        ordinary({
          totalExecutedQuantity: undefined,
          tradeId,
          status: 'PARTIAL_FILLED',
          executedQuantity: '0.001',
          executedPrice: '59000',
        }),
      );
    }
    expect(store.orders[0].filled).toBe(0.002);
    expect(fills).toHaveLength(2);
  });

  it('rejects an unsupported conditional batch before placing a valid prefix', async () => {
    const { exchange } = make();
    const post = vi.fn();
    exchange.xhr = { post } as any;
    await expect(
      exchange.placeOrders([
        { symbol: 'BTCUSDT', type: OrderType.Limit, side: OrderSide.Buy, amount: 0.001, price: 59000 },
        { symbol: 'BTCUSDT', type: OrderType.Conditional, side: OrderSide.Buy, amount: 0.001, price: 59000 },
      ]),
    ).rejects.toThrow('opening conditional');
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects failed order pages without replacing known live orders', async () => {
    const { exchange, store } = make();
    const existing = exchange.mapLimitOrder(ordinary())!;
    store.update({ orders: [existing] });
    exchange.xhr = { get: vi.fn().mockResolvedValue({ data: { success: false, message: 'busy' } }) } as any;
    await expect(exchange.fetchOrders()).rejects.toThrow('busy');
    expect(store.orders).toEqual([existing]);
  });

  it('rejects a malformed row for a known market instead of publishing a partial snapshot', async () => {
    const { exchange } = make();
    vi.spyOn(exchange, 'fetchOrdersRecursively').mockResolvedValue([{ symbol: 'PERP_BTC_USDT', quantity: 1 }]);
    await expect(exchange.fetchLimitOrders()).rejects.toThrow('Invalid WOO regular order');
  });

  it('protects a private update arriving while a REST page is pending', async () => {
    const { exchange, store } = make();
    let resolve!: (value: any) => void;
    const get = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise((done) => {
            resolve = done;
          }),
      )
      .mockResolvedValue({
        data: { success: true, data: { rows: [], meta: { total: 0, records_per_page: 100, current_page: 1 } } },
      });
    exchange.xhr = { get } as any;
    const pending = exchange.fetchOrders();
    exchange.privateWebsocket.handleExecutionReport(
      ordinary({ totalExecutedQuantity: 0.002, status: 'PARTIAL_FILLED' }),
    );
    resolve({ data: { success: true, rows: [], meta: { total: 0, records_per_page: 100, current_page: 1 } } });
    const orders = await pending;
    expect(orders).toHaveLength(1);
    expect(orders[0].filled).toBe(0.002);
    expect(store.orders[0].filled).toBe(0.002);
  });

  it('maps generic STOP direction and reference and a buy trailing callback correctly', () => {
    const { exchange } = make();
    const base = {
      algoOrderId: 200,
      rootAlgoOrderId: 200,
      symbol: 'PERP_BTC_USDT',
      quantity: 0.01,
      side: 'SELL',
      algoType: 'STOP',
      triggerPriceType: 'MARKET_PRICE',
      triggerPrice: 65000,
    };
    expect(exchange.mapAlgoOrder(base)[0]).toMatchObject({
      type: OrderType.TakeProfit,
      tpTriggerType: OrderStopTriggerType.LastPrice,
    });
    expect(exchange.mapAlgoOrder({ ...base, triggerPrice: 58000 })[0].type).toBe(OrderType.StopLoss);
    expect(
      exchange.mapAlgoOrder({
        ...base,
        algoType: 'TRAILING_STOP',
        side: 'BUY',
        triggerPrice: undefined,
        extremePrice: 59000,
        callbackValue: 1000,
      })[0].price,
    ).toBe(60000);
  });

  it('splits exact ticks without creating or losing requested quantity', () => {
    const { store } = make();
    expect(splitWooQuantity(201.001, store.markets[0])).toEqual([67.001, 67, 67]);
  });

  it('sends trailing SL reference and full matching position quantity', async () => {
    const { exchange, store } = make();
    store.update({
      positions: [
        {
          id: 'BTCUSDT:short',
          symbol: 'BTCUSDT',
          side: PositionSide.Short,
          contracts: 0.023,
          entryPrice: 60000,
          notional: 1380,
          unrealizedPnl: 0,
          liquidationPrice: 0,
          leverage: 10,
          marginType: MarginType.Cross,
          positionMode: PositionMode.OneWay,
        },
      ],
    });
    const post = vi.fn().mockResolvedValue({ data: { success: true, data: { rows: [{ orderId: 201 }] } } });
    exchange.xhr = { post } as any;
    await exchange.placeTrailingStopLossOrder({
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.TrailingStopLoss,
      price: 61000,
      amount: 0.001,
      slTriggerType: OrderStopTriggerType.LastPrice,
    });
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.ALGO_ORDER,
      expect.objectContaining({ quantity: '0.023', callbackValue: '1000', triggerPriceType: 'MARKET_PRICE' }),
    );
  });

  it('sends rounded quantity amendments to the correct algo target and rejects invalid sizes before writing', async () => {
    const { exchange } = make();
    const put = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { put } as any;
    const order = exchange.mapAlgoOrder({
      algoOrderId: 200,
      rootAlgoOrderId: 200,
      symbol: 'PERP_BTC_USDT',
      side: 'SELL',
      quantity: 0.01,
      algoType: 'STOP',
      triggerPrice: 58000,
    })[0];
    await exchange.updateAlgoOrder({ order, update: { amount: 0.0124 } });
    expect(put).toHaveBeenCalledWith('/v3/algo/order/200', { quantity: '0.012' });
    await expect(exchange.updateAlgoOrder({ order, update: { amount: -1 } })).rejects.toThrow(
      'Invalid WOO amendment quantity',
    );
    expect(put).toHaveBeenCalledTimes(1);
  });

  it('cancels both families globally and uses the native merged symbol path', async () => {
    const { exchange } = make();
    const del = vi.fn().mockResolvedValue({ data: { success: true } });
    exchange.xhr = { delete: del } as any;
    await exchange.cancelAllOrders();
    await exchange.cancelSymbolOrders('BTCUSDT');
    expect(del.mock.calls).toEqual([
      ['/v3/orders/pending'],
      ['/v3/algo/orders/pending'],
      ['/v3/merge/orders/pending/PERP_BTC_USDT'],
    ]);
  });

  it('attempts algo cancellation even when regular cancel-all fails', async () => {
    const { exchange } = make();
    const del = vi.fn().mockRejectedValueOnce(new Error('busy')).mockResolvedValueOnce({});
    exchange.xhr = { delete: del } as any;
    await expect(exchange.cancelAllOrders()).rejects.toThrow('busy');
    expect(del).toHaveBeenCalledTimes(2);
  });

  it('walks a bounded income range past the old 500-row cap', async () => {
    const { exchange } = make();
    const get = vi.fn().mockImplementation((_path, { params }) => {
      const start = (params.page - 1) * 100;
      const count = Math.min(100, 501 - start);
      return Promise.resolve({
        data: {
          success: true,
          data: {
            rows: Array.from({ length: count }, (_, i) => row(start + i)),
            meta: { total: 501, records_per_page: 100, current_page: params.page },
          },
        },
      });
    });
    exchange.xhr = { get } as any;
    const pending = exchange.fetchIncomeHistoryRange(NOW - 1000, NOW + 1000);
    await vi.runAllTimersAsync();
    expect(await pending).toHaveLength(501);
    expect(get).toHaveBeenCalledTimes(6);
  });

  it('does not treat a short ledger page without metadata as complete', async () => {
    const { exchange } = make();
    const get = vi
      .fn()
      .mockResolvedValueOnce({ data: { success: true, data: { rows: [row(1)] } } })
      .mockResolvedValueOnce({ data: { success: true, data: { rows: [] } } });
    exchange.xhr = { get } as any;
    const pending = exchange.fetchIncomeHistory();
    await vi.runAllTimersAsync();
    expect(await pending).toHaveLength(1);
    expect(get).toHaveBeenCalledTimes(2);
  });

  it('emits malformed ledger metadata errors before rejecting', async () => {
    const { exchange } = make();
    const errors: string[] = [];
    exchange.on('error', (error: unknown) => errors.push(String(error)));
    exchange.xhr = {
      get: vi.fn().mockResolvedValue({
        data: { success: true, data: { rows: [row(1)], meta: { total: 1, records_per_page: 0, current_page: 1 } } },
      }),
    } as any;
    await expect(exchange.fetchIncomeHistory()).rejects.toThrow('Invalid WOO income page metadata');
    expect(errors).toHaveLength(1);
  });
});
