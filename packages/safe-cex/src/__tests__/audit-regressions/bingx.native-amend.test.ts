import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { BingxExchange } from '../reference/bingx/bingx.exchange';
import { ENDPOINTS } from '../reference/bingx/bingx.types';
import { DefaultStore } from '../../store/store.base';
import {
  MarginType,
  Market,
  Order,
  OrderSide,
  OrderStatus,
  OrderTimeInForce,
  OrderType,
  PositionMode,
  PositionSide,
} from '../../types';

const MARKET: Market = {
  id: 'BTC-USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};

const createOrder = (overrides: Partial<Order> = {}): Order => ({
  id: '1989246372640980993',
  status: OrderStatus.Open,
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1.25,
  filled: 0,
  remaining: 1.25,
  reduceOnly: false,
  timeInForce: OrderTimeInForce.GoodTillCancel,
  lastUpdate: 1_784_505_600_000,
  info: {
    orderId: '1989246372640980993',
    symbol: 'BTC-USDT',
    side: 'BUY',
    positionSide: 'BOTH',
    type: 'LIMIT',
    origQty: '1.250',
    executedQty: '0',
    reduceOnly: false,
    timeInForce: 'GTC',
    postOnly: false,
    status: 'NEW',
    clientOrderId: 'client-order-1',
  },
  ...overrides,
});

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new BingxExchange('bingx', { key: 'key', secret: 'secret' }, store);
  const amendPost = vi.fn().mockImplementation(async (_endpoint, payload) => ({
    data: {
      code: 0,
      msg: '',
      data: {
        orderId: '1989246372640980993',
        clientOrderId: '',
        symbol: 'BTC-USDT',
        quantity: payload.quantity,
      },
    },
  }));
  const replacementPost = vi.fn();
  exchange.xhr = { post: amendPost } as unknown as Axios;
  exchange.unlimitedXHR = { post: replacementPost } as unknown as Axios;

  return { exchange, amendPost, replacementPost };
};

const expectNoReplacementWrites = (exchange: BingxExchange, replacementPost: ReturnType<typeof vi.fn>) => {
  const cancelOrders = vi.spyOn(exchange, 'cancelOrders');
  const placeOrder = vi.spyOn(exchange, 'placeOrder');
  return () => {
    expect(replacementPost).not.toHaveBeenCalled();
    expect(cancelOrders).not.toHaveBeenCalled();
    expect(placeOrder).not.toHaveBeenCalled();
  };
};

describe('BingX native amend safety boundary', () => {
  it('amends an exactly reconciled unfilled LIMIT quantity in place and preserves the stable order id', async () => {
    const { exchange, amendPost, replacementPost } = createExchange();
    const assertNoReplacementWrites = expectNoReplacementWrites(exchange, replacementPost);

    await expect(exchange.updateOrder({ order: createOrder(), update: { amount: 0.7504 } })).resolves.toEqual([
      '1989246372640980993',
    ]);

    expect(amendPost).toHaveBeenCalledOnce();
    expect(amendPost).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      orderId: '1989246372640980993',
      quantity: 0.75,
      symbol: 'BTC-USDT',
    });
    expect(ENDPOINTS.AMEND_ORDER).toBe('/openApi/swap/v1/trade/amend');
    assertNoReplacementWrites();
  });

  it('uses atomic cancel-replace for a reconciled unfilled LIMIT price move', async () => {
    const { exchange, amendPost, replacementPost } = createExchange();
    amendPost.mockResolvedValueOnce({
      data: {
        code: 0,
        msg: '',
        data: {
          cancelResult: 'true',
          cancelMsg: '',
          cancelResponse: {
            cancelClientOrderId: 'client-order-1',
            orderId: '1989246372640980993',
          },
          replaceResult: 'true',
          replaceMsg: '',
          newOrderResponse: {
            orderId: '1989246372640980994',
            orderID: '',
            symbol: MARKET.id,
            positionSide: 'BOTH',
            side: 'BUY',
            type: 'LIMIT',
            price: 50_100,
            quantity: 1.25,
            clientOrderId: 'tsreplacement1',
            timeInForce: 'GTC',
            status: 'PENDING',
            executedQty: '0',
          },
        },
      },
    });
    vi.spyOn(exchange as any, 'createClientOrderId').mockReturnValue('tsreplacement1');
    exchange.store.addOrUpdateOrder(createOrder());

    await expect(exchange.updateOrder({ order: createOrder(), update: { price: 50_100 } })).resolves.toEqual([
      '1989246372640980994',
    ]);

    expect(amendPost).toHaveBeenCalledOnce();
    expect(amendPost).toHaveBeenCalledWith(ENDPOINTS.CANCEL_REPLACE_ORDER, {
      symbol: MARKET.id,
      cancelClientOrderId: 'client-order-1',
      side: 'BUY',
      positionSide: 'BOTH',
      type: 'LIMIT',
      quantity: 1.25,
      price: 50_100,
      cancelReplaceMode: 'STOP_ON_FAILURE',
      cancelRestrictions: 'ONLY_NEW',
      timeInForce: 'GTC',
      clientOrderId: 'tsreplacement1',
    });
    expect(exchange.store.orders).toEqual([
      expect.objectContaining({ id: '1989246372640980993', status: OrderStatus.Canceled }),
      expect.objectContaining({ id: '1989246372640980994', status: OrderStatus.Open, price: 50_100 }),
    ]);
    expect(replacementPost).not.toHaveBeenCalled();
  });

  it('cancels and replaces a reconciled active STOP_MARKET price move', async () => {
    const { exchange, amendPost, replacementPost } = createExchange();
    const deleteOrder = vi.fn().mockResolvedValueOnce({ data: { code: 0, msg: '' } });
    replacementPost.mockResolvedValueOnce({
      data: {
        code: 0,
        msg: '',
        data: {
          order: {
            orderId: '2086644409474576385',
            orderID: '2086644409474576385',
            symbol: MARKET.id,
            positionSide: 'LONG',
            side: 'SELL',
            type: 'STOP_MARKET',
            quantity: '27',
            price: '0',
            stopPrice: '49000',
            status: 'PENDING',
            executedQty: '0',
            workingType: 'MARK_PRICE',
          },
        },
      },
    });
    exchange.xhr = { post: amendPost, delete: deleteOrder } as unknown as Axios;
    exchange.store.update({
      positions: [
        {
          id: 'BTCUSDT-long',
          symbol: MARKET.symbol,
          side: PositionSide.Long,
          entryPrice: 50_000,
          notional: 1_350_000,
          leverage: 20,
          marginType: MarginType.Cross,
          positionMode: PositionMode.Hedged,
          unrealizedPnl: 0,
          contracts: 27,
          liquidationPrice: 0,
        },
      ],
    });
    (exchange as any)._accountPositionMode = PositionMode.Hedged;
    const stopLoss = createOrder({
      id: '2086644409474576384',
      type: OrderType.StopLoss,
      side: OrderSide.Sell,
      price: 48_500,
      amount: 27,
      remaining: 27,
      reduceOnly: true,
      timeInForce: undefined,
      info: {
        orderId: '2086644409474576384',
        symbol: MARKET.id,
        side: 'SELL',
        positionSide: 'LONG',
        type: 'STOP_MARKET',
        origQty: '27.0',
        price: '0.00000',
        executedQty: '0.0',
        stopPrice: '48500',
        status: 'NEW',
        clientOrderId: '',
        workingType: 'MARK_PRICE',
        reduceOnly: true,
        postOnly: false,
        closePosition: 'false',
      },
    });
    exchange.store.addOrUpdateOrder(stopLoss);

    await expect(exchange.updateOrder({ order: stopLoss, update: { price: 49_000 } })).resolves.toEqual([
      '2086644409474576385',
    ]);

    expect(deleteOrder).toHaveBeenCalledOnce();
    expect(deleteOrder).toHaveBeenCalledWith(ENDPOINTS.ORDER, {
      params: {
        symbol: MARKET.id,
        orderId: '2086644409474576384',
      },
    });
    expect(replacementPost).toHaveBeenCalledOnce();
    expect(replacementPost).toHaveBeenCalledWith(ENDPOINTS.ORDER, {
      symbol: MARKET.id,
      positionSide: 'LONG',
      side: 'SELL',
      type: 'STOP_MARKET',
      quantity: 27,
      stopPrice: 49_000,
      workingType: 'MARK_PRICE',
    });
    expect(amendPost).not.toHaveBeenCalled();
    expect(exchange.store.orders).toEqual([
      expect.objectContaining({ id: '2086644409474576384', status: OrderStatus.Canceled }),
      expect.objectContaining({ id: '2086644409474576385', status: OrderStatus.Open, price: 49_000 }),
    ]);
  });

  it('preserves the raw STOP_MARKET type when a stale mark reclassified the row as take-profit', async () => {
    const { exchange, amendPost, replacementPost } = createExchange();
    const deleteOrder = vi.fn().mockResolvedValueOnce({ data: { code: 0, msg: '' } });
    replacementPost.mockResolvedValueOnce({
      data: {
        code: 0,
        msg: '',
        data: {
          order: {
            orderId: '2086644409474576385',
            orderID: '2086644409474576385',
            symbol: MARKET.id,
            positionSide: 'LONG',
            side: 'SELL',
            type: 'STOP_MARKET',
            quantity: '27',
            price: '0',
            stopPrice: '49000',
            status: 'PENDING',
            executedQty: '0',
            workingType: 'MARK_PRICE',
          },
        },
      },
    });
    exchange.xhr = { post: amendPost, delete: deleteOrder } as unknown as Axios;
    exchange.store.update({
      positions: [
        {
          id: 'BTCUSDT-long',
          symbol: MARKET.symbol,
          side: PositionSide.Long,
          entryPrice: 50_000,
          notional: 1_350_000,
          leverage: 20,
          marginType: MarginType.Cross,
          positionMode: PositionMode.Hedged,
          unrealizedPnl: 0,
          contracts: 27,
          liquidationPrice: 0,
        },
      ],
    });
    (exchange as any)._accountPositionMode = PositionMode.Hedged;
    const misclassifiedStopLoss = createOrder({
      id: '2086644409474576384',
      type: OrderType.TakeProfit,
      side: OrderSide.Sell,
      price: 48_500,
      amount: 27,
      remaining: 27,
      reduceOnly: true,
      timeInForce: undefined,
      info: {
        orderId: '2086644409474576384',
        symbol: MARKET.id,
        side: 'SELL',
        positionSide: 'LONG',
        type: 'STOP_MARKET',
        origQty: '27.0',
        price: '0.00000',
        executedQty: '0.0',
        stopPrice: '48500',
        status: 'NEW',
        clientOrderId: '',
        workingType: 'MARK_PRICE',
        reduceOnly: true,
        postOnly: false,
        closePosition: 'false',
      },
    });

    await exchange.updateOrder({ order: misclassifiedStopLoss, update: { price: 49_000 } });

    expect(replacementPost).toHaveBeenCalledWith(ENDPOINTS.ORDER, {
      symbol: MARKET.id,
      positionSide: 'LONG',
      side: 'SELL',
      type: 'STOP_MARKET',
      quantity: 27,
      stopPrice: 49_000,
      workingType: 'MARK_PRICE',
    });
    expect(amendPost).not.toHaveBeenCalled();
  });

  it('allows BingX PENDING/post-only rows with empty TP/SL placeholders to be cancel-replaced', async () => {
    const { exchange, amendPost } = createExchange();
    amendPost.mockResolvedValueOnce({
      data: {
        code: 0,
        msg: '',
        data: {
          cancelResult: 'SUCCESS',
          newOrderResult: 'SUCCESS',
          cancelOrderId: '1989246372640980993',
          newOrderId: '1989246372640980994',
        },
      },
    });
    vi.spyOn(exchange as any, 'createClientOrderId').mockReturnValue('tsreplacement1');
    const pendingPostOnly = createOrder({
      timeInForce: OrderTimeInForce.PostOnly,
      info: {
        ...createOrder().info,
        status: 'PENDING',
        timeInForce: 'PostOnly',
        postOnly: true,
        takeProfit: { type: 'TAKE_PROFIT', quantity: 0, stopPrice: 0, price: 0 },
        stopLoss: { type: 'STOP', quantity: 0, stopPrice: 0, price: 0 },
      },
    });

    await exchange.updateOrder({ order: pendingPostOnly, update: { price: 50_100 } });

    expect(amendPost).toHaveBeenCalledWith(
      ENDPOINTS.CANCEL_REPLACE_ORDER,
      expect.objectContaining({
        cancelRestrictions: 'ONLY_PENDING',
        timeInForce: 'PostOnly',
        price: 50_100,
      }),
    );
  });

  it('falls back to normalized order fields when raw info lacks REST identity aliases', async () => {
    const { exchange, amendPost } = createExchange();
    amendPost.mockResolvedValueOnce({
      data: {
        code: 0,
        msg: '',
        data: {
          cancelResult: 'SUCCESS',
          newOrderResult: 'SUCCESS',
          cancelOrderId: '1989246372640980993',
          newOrderId: '1989246372640980994',
        },
      },
    });
    vi.spyOn(exchange as any, 'createClientOrderId').mockReturnValue('tsreplacement1');
    (exchange as any)._accountPositionMode = PositionMode.Hedged;
    const orderWithSparseInfo = createOrder({
      timeInForce: OrderTimeInForce.PostOnly,
      info: {
        orderId: '1989246372640980993',
        c: 'client-order-1',
        status: 'PENDING',
        quantity: '1.25',
        executedQty: '0',
      },
    });

    await exchange.updateOrder({ order: orderWithSparseInfo, update: { price: 50_100 } });

    expect(amendPost).toHaveBeenCalledWith(
      ENDPOINTS.CANCEL_REPLACE_ORDER,
      expect.objectContaining({
        symbol: MARKET.id,
        cancelClientOrderId: 'client-order-1',
        side: 'BUY',
        positionSide: 'LONG',
        timeInForce: 'PostOnly',
      }),
    );
  });

  it('reconciles REST order details before cancel-replace when WS info omitted time in force', async () => {
    const { exchange, amendPost } = createExchange();
    const get = vi.fn().mockResolvedValue({
      data: {
        code: 0,
        msg: '',
        data: {
          orders: [
            {
              orderId: '1989246372640980993',
              symbol: MARKET.id,
              side: 'BUY',
              positionSide: 'LONG',
              type: 'LIMIT',
              origQty: '1.25',
              price: '50000',
              executedQty: '0',
              status: 'PENDING',
              clientOrderId: 'client-order-1',
              timeInForce: 'PostOnly',
              postOnly: true,
              reduceOnly: false,
              takeProfit: { type: 'TAKE_PROFIT', quantity: 0, stopPrice: 0, price: 0 },
              stopLoss: { type: 'STOP', quantity: 0, stopPrice: 0, price: 0 },
            },
          ],
        },
      },
    });
    amendPost.mockResolvedValueOnce({
      data: {
        code: 0,
        msg: '',
        data: {
          cancelResult: 'SUCCESS',
          newOrderResult: 'SUCCESS',
          cancelOrderId: '1989246372640980993',
          newOrderId: '1989246372640980994',
        },
      },
    });
    exchange.xhr = { get, post: amendPost } as unknown as Axios;
    vi.spyOn(exchange as any, 'createClientOrderId').mockReturnValue('tsreplacement1');
    const wsOnlyOrder = createOrder({
      timeInForce: undefined,
      info: {
        i: '1989246372640980993',
        s: MARKET.id,
        S: 'BUY',
        ps: 'LONG',
        o: 'LIMIT',
        q: '1.25',
        p: '50000',
        z: '0',
        X: 'WORKING',
        c: 'client-order-1',
        ro: false,
      },
    });

    await exchange.updateOrder({ order: wsOnlyOrder, update: { price: 50_100 } });

    expect(get).toHaveBeenCalledWith(ENDPOINTS.OPEN_ORDERS);
    expect(amendPost).toHaveBeenCalledWith(
      ENDPOINTS.CANCEL_REPLACE_ORDER,
      expect.objectContaining({
        cancelClientOrderId: 'client-order-1',
        timeInForce: 'PostOnly',
        positionSide: 'LONG',
      }),
    );
  });

  it('preserves post-only, reduce-only, side, and hedged position-side attributes by amending only identity and size', async () => {
    const { exchange, amendPost } = createExchange();
    const order = createOrder({
      side: OrderSide.Sell,
      reduceOnly: true,
      timeInForce: OrderTimeInForce.PostOnly,
      info: {
        ...createOrder().info,
        side: 'SELL',
        positionSide: 'SHORT',
        reduceOnly: true,
        timeInForce: 'PostOnly',
        postOnly: true,
      },
    });

    await exchange.updateOrder({ order, update: { amount: 1 } });

    expect(amendPost).toHaveBeenCalledWith(ENDPOINTS.AMEND_ORDER, {
      orderId: order.id,
      quantity: 1,
      symbol: MARKET.id,
    });
    expect(amendPost.mock.calls[0][1]).not.toHaveProperty('side');
    expect(amendPost.mock.calls[0][1]).not.toHaveProperty('positionSide');
    expect(amendPost.mock.calls[0][1]).not.toHaveProperty('reduceOnly');
    expect(amendPost.mock.calls[0][1]).not.toHaveProperty('timeInForce');
  });

  it.each([
    {
      name: 'combined price and quantity edit',
      order: createOrder(),
      update: { price: 50_100, amount: 1 },
      error: 'cannot be combined',
    },
    {
      name: 'unchanged normalized quantity',
      order: createOrder(),
      update: { amount: 1.2504 },
      error: 'must change after market-precision normalization',
    },
    {
      name: 'partially filled total-size ambiguity',
      order: createOrder({
        filled: 0.25,
        remaining: 1,
        info: { ...createOrder().info, status: 'PARTIALLY_FILLED', executedQty: '0.25' },
      }),
      update: { amount: 1 },
      error: 'disabled until quantity is documented as total or remaining size',
    },
    {
      name: 'trigger order',
      order: createOrder({ type: OrderType.StopLoss }),
      update: { amount: 1 },
      error: 'trigger order movement supports price updates only',
    },
    {
      name: 'LIMIT with attached protection',
      order: createOrder({
        info: {
          ...createOrder().info,
          takeProfit: { type: 'TAKE_PROFIT_MARKET', stopPrice: '51000', quantity: '1.25' },
        },
      }),
      update: { amount: 1 },
      error: 'cannot prove attached TP/SL quantity preservation',
    },
  ])('rejects $name before any amend, cancel-replace, cancel, or placement write', async ({ order, update, error }) => {
    const { exchange, amendPost, replacementPost } = createExchange();
    const assertNoReplacementWrites = expectNoReplacementWrites(exchange, replacementPost);

    await expect(exchange.updateOrder({ order, update } as any)).rejects.toThrow(error);

    expect(amendPost).not.toHaveBeenCalled();
    assertNoReplacementWrites();
  });

  it.each([
    {
      name: 'mismatched order id',
      info: { ...createOrder().info, orderId: 'different-order' },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'mismatched side',
      info: { ...createOrder().info, side: 'SELL' },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'invalid position side',
      info: { ...createOrder().info, positionSide: 'UNKNOWN' },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'missing side provenance',
      info: { ...createOrder().info, side: undefined },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'missing position-side provenance',
      info: { ...createOrder().info, positionSide: undefined },
      error: 'immutable order identity is not reconciled',
    },
    {
      name: 'mismatched reduce-only',
      info: { ...createOrder().info, reduceOnly: true },
      error: 'reduce-only state is not reconciled',
    },
    {
      name: 'mismatched time in force',
      info: { ...createOrder().info, timeInForce: 'PostOnly', postOnly: true },
      error: 'time in force is not reconciled',
    },
    {
      name: 'missing time-in-force provenance',
      info: { ...createOrder().info, timeInForce: undefined, postOnly: undefined },
      error: 'time in force is not reconciled',
    },
    {
      name: 'unknown time-in-force with post-only false',
      info: { ...createOrder().info, timeInForce: 'UNKNOWN', postOnly: false },
      error: 'time in force is not reconciled',
    },
    {
      name: 'omitted time-in-force with post-only false',
      info: { ...createOrder().info, timeInForce: undefined, postOnly: false },
      error: 'time in force is not reconciled',
    },
    {
      name: 'mismatched original quantity',
      info: { ...createOrder().info, origQty: '2' },
      error: 'disabled until quantity is documented as total or remaining size',
    },
    {
      name: 'missing original quantity',
      info: { ...createOrder().info, origQty: undefined },
      error: 'disabled until quantity is documented as total or remaining size',
    },
    {
      name: 'missing executed quantity',
      info: { ...createOrder().info, executedQty: undefined },
      error: 'disabled until quantity is documented as total or remaining size',
    },
    {
      name: 'missing pending status',
      info: { ...createOrder().info, status: undefined },
      error: 'disabled until quantity is documented as total or remaining size',
    },
  ])('rejects $name before any write', async ({ info, error }) => {
    const { exchange, amendPost, replacementPost } = createExchange();
    const assertNoReplacementWrites = expectNoReplacementWrites(exchange, replacementPost);

    await expect(
      exchange.updateOrder({ order: createOrder({ info }), update: { amount: 0.75 } }),
    ).rejects.toThrow(error);

    expect(amendPost).not.toHaveBeenCalled();
    assertNoReplacementWrites();
  });

  it.each([
    {
      name: 'HTTP-200 exchange rejection',
      response: { data: { code: 100400, msg: 'Invalid parameter', data: {} } },
      error: 'Invalid parameter',
    },
    {
      name: 'mismatched acknowledgement id',
      response: {
        data: { code: 0, msg: '', data: { orderId: 'different-order', symbol: 'BTC-USDT', quantity: 0.75 } },
      },
      error: 'did not confirm order 1989246372640980993',
    },
    {
      name: 'missing acknowledged quantity',
      response: {
        data: { code: 0, msg: '', data: { orderId: '1989246372640980993', symbol: 'BTC-USDT' } },
      },
      error: 'did not confirm order 1989246372640980993',
    },
  ])('surfaces $name without canceling or placing a replacement', async ({ response, error }) => {
    const { exchange, amendPost, replacementPost } = createExchange();
    amendPost.mockResolvedValue(response);
    const assertNoReplacementWrites = expectNoReplacementWrites(exchange, replacementPost);

    await expect(exchange.updateOrder({ order: createOrder(), update: { amount: 0.75 } })).rejects.toThrow(error);

    expect(amendPost).toHaveBeenCalledOnce();
    assertNoReplacementWrites();
  });
});
