import { describe, expect, it, vi } from 'vitest';

import { WeexExchange } from '../reference/weex/weex.exchange';
import { ENDPOINTS } from '../reference/weex/weex.types';
import { DefaultStore } from '../../store/store.base';
import type { PlaceOrderOpts } from '../../types';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';

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
  return { exchange, store };
};

const regularOrder = (index: number, overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTCUSDT',
  type: index % 2 ? OrderType.Limit : OrderType.Market,
  side: index % 2 ? OrderSide.Sell : OrderSide.Buy,
  amount: 0.001 * (index + 1),
  price: index % 2 ? 50_000 + index : undefined,
  ...overrides,
});

describe('audit regression: WEEX Contract V3 regular batch placement', () => {
  it('chunks twelve regular orders as 5/5/2 and restores input order by echoed client ID', async () => {
    const { exchange } = createExchange();
    const expectedIds: string[] = [];
    const post = vi.fn().mockImplementation((endpoint: string, body: any) => {
      expect(endpoint).toBe(ENDPOINTS.BATCH_ORDERS);
      const rows = body.batchOrders.map((payload: any) => {
        const orderId = `${expectedIds.length + 1}`;
        expectedIds.push(orderId);
        return {
          orderId,
          clientOrderId: payload.newClientOrderId,
          success: true,
          errorCode: '',
          errorMessage: '',
        };
      });
      return Promise.resolve({ data: rows.reverse() });
    });
    exchange.xhr = { post } as any;

    const ids = await exchange.placeOrders(Array.from({ length: 12 }, (_, index) => regularOrder(index)));

    expect(post).toHaveBeenCalledTimes(3);
    expect(post.mock.calls.map(([, body]) => body.batchOrders.length)).toEqual([5, 5, 2]);
    expect(ids).toEqual(expectedIds);
    const clientOrderIds = post.mock.calls.flatMap(([, body]) =>
      body.batchOrders.map((payload: any) => payload.newClientOrderId),
    );
    expect(new Set(clientOrderIds).size).toBe(12);
    expect(clientOrderIds.every((id) => /^ts.+/.test(id) && id.length <= 36)).toBe(true);
  });

  it('keeps algo, direct TP/SL, attached TP/SL, and reduce-only orders on their single paths', async () => {
    const { exchange } = createExchange();
    let nextId = 0;
    const post = vi.fn().mockImplementation((endpoint: string, body: any) => {
      nextId += 1;
      if (endpoint === ENDPOINTS.BATCH_ORDERS) {
        return Promise.resolve({
          data: body.batchOrders.map((payload: any) => ({
            orderId: `${nextId}`,
            clientOrderId: payload.newClientOrderId,
            success: true,
            errorCode: '',
            errorMessage: '',
          })),
        });
      }
      return Promise.resolve({
        data: {
          orderId: `single-${nextId}`,
          clientOrderId: body.newClientOrderId || body.clientAlgoId,
          success: true,
          errorCode: '',
          errorMessage: '',
        },
      });
    });
    exchange.xhr = { post } as any;

    await exchange.placeOrders([
      regularOrder(0),
      regularOrder(1, { type: OrderType.Conditional, triggerPrice: 49_000 }),
      regularOrder(2, { type: OrderType.StopLoss, price: 48_000 }),
      regularOrder(3, { type: OrderType.TakeProfit, price: 52_000 }),
      regularOrder(4, { type: OrderType.Limit, price: 50_000, takeProfit: 52_000 }),
      regularOrder(5, { type: OrderType.Limit, price: 50_000, reduceOnly: true }),
      regularOrder(6),
    ]);

    expect(post.mock.calls.map(([endpoint]) => endpoint)).toEqual([
      ENDPOINTS.BATCH_ORDERS,
      ENDPOINTS.ALGO_ORDER,
      ENDPOINTS.PLACE_TP_SL_ORDER,
      ENDPOINTS.PLACE_TP_SL_ORDER,
      ENDPOINTS.ORDER,
      ENDPOINTS.ORDER,
      ENDPOINTS.BATCH_ORDERS,
    ]);
    const batchBodies = post.mock.calls
      .filter(([endpoint]) => endpoint === ENDPOINTS.BATCH_ORDERS)
      .map(([, body]) => body.batchOrders);
    expect(batchBodies.flat()).toHaveLength(2);
    expect(batchBodies.flat().every((payload) => !payload.clientAlgoId && !payload.tpTriggerPrice)).toBe(true);
  });

  it('keeps successful PostOnly provenance while surfacing every per-row failure', async () => {
    const { exchange, store } = createExchange();
    const updateOrder = vi.spyOn(store, 'updateOrder');
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    exchange.xhr = {
      post: vi.fn().mockImplementation((_endpoint: string, body: any) =>
        Promise.resolve({
          data: [
            {
              orderId: '101',
              clientOrderId: body.batchOrders[0].newClientOrderId,
              success: true,
              errorCode: '',
              errorMessage: '',
            },
            {
              orderId: '',
              clientOrderId: body.batchOrders[1].newClientOrderId,
              success: false,
              errorCode: '40017',
              errorMessage: 'bad size',
            },
            {
              orderId: '',
              clientOrderId: body.batchOrders[2].newClientOrderId,
              success: false,
              errorCode: '40018',
              errorMessage: 'bad price',
            },
          ],
        }),
      ),
    } as any;

    await expect(
      exchange.placeOrders([
        regularOrder(0, { type: OrderType.Limit, price: 50_000, timeInForce: OrderTimeInForce.PostOnly }),
        regularOrder(1),
        regularOrder(2),
      ]),
    ).rejects.toThrow(/bad size.*bad price/);
    expect(updateOrder).toHaveBeenCalledWith(
      { id: '101' },
      { timeInForce: OrderTimeInForce.PostOnly },
    );
    expect(emitAndLogError).toHaveBeenCalledOnce();
  });

  it.each([
    ['a non-array envelope', (_clientOrderIds: string[]) => ({ success: true })],
    [
      'a row-count mismatch',
      (clientOrderIds: string[]) => [
        { orderId: '1', clientOrderId: clientOrderIds[0], success: true },
      ],
    ],
    [
      'a duplicate echoed identity',
      (clientOrderIds: string[]) => [
        { orderId: '1', clientOrderId: clientOrderIds[0], success: true },
        { orderId: '2', clientOrderId: clientOrderIds[0], success: true },
      ],
    ],
    [
      'an unknown echoed identity',
      (clientOrderIds: string[]) => [
        { orderId: '1', clientOrderId: clientOrderIds[0], success: true },
        { orderId: '2', clientOrderId: 'unknown-client-id', success: true },
      ],
    ],
    [
      'a missing success flag',
      (clientOrderIds: string[]) => [
        { orderId: '1', clientOrderId: clientOrderIds[0], success: true },
        { orderId: '2', clientOrderId: clientOrderIds[1] },
      ],
    ],
    [
      'a successful row without an order ID',
      (clientOrderIds: string[]) => [
        { orderId: '1', clientOrderId: clientOrderIds[0], success: true },
        { clientOrderId: clientOrderIds[1], success: true },
      ],
    ],
    [
      'a successful row with a non-integer order ID',
      (clientOrderIds: string[]) => [
        { orderId: '1', clientOrderId: clientOrderIds[0], success: true },
        { orderId: 'not-an-order-id', clientOrderId: clientOrderIds[1], success: true },
      ],
    ],
    [
      'a non-string error code',
      (clientOrderIds: string[]) => [
        { orderId: '1', clientOrderId: clientOrderIds[0], success: true },
        { orderId: '2', clientOrderId: clientOrderIds[1], success: true, errorCode: 0 },
      ],
    ],
    [
      'a non-string error message',
      (clientOrderIds: string[]) => [
        { orderId: '1', clientOrderId: clientOrderIds[0], success: true },
        { orderId: '2', clientOrderId: clientOrderIds[1], success: true, errorMessage: false },
      ],
    ],
  ])('rejects %s without fabricating successful IDs', async (_label, makeResponse) => {
    const { exchange } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    exchange.xhr = {
      post: vi.fn().mockImplementation((_endpoint: string, body: any) => {
        const clientOrderIds = body.batchOrders.map((payload: any) => payload.newClientOrderId);
        return Promise.resolve({ data: makeResponse(clientOrderIds) });
      }),
    } as any;

    await expect(exchange.placeOrders([regularOrder(0), regularOrder(1)])).rejects.toThrow(
      'WEEX batch order response malformed',
    );
    expect(emitAndLogError).toHaveBeenCalledOnce();
  });
});
