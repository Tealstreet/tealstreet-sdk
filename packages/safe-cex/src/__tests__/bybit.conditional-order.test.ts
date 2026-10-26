import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from './reference/bybit/bybit.exchange';
import { OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionMode, PositionSide } from '../types';

const createStore = () =>
  ({
    subscribe: vi.fn(),
    markets: [],
    marketsBySymbol: {
      BTCUSDT: {
        id: 'BTCUSDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: {
          amount: 0.001,
          price: 0.1,
        },
        limits: {
          amount: {
            min: 0.001,
            max: 100,
          },
          leverage: {
            min: 1,
            max: 100,
          },
        },
      },
    },
    tickersBySymbol: {
      BTCUSDT: {
        id: 'BTCUSDT',
        symbol: 'BTCUSDT',
        bid: 29999,
        ask: 30001,
        last: 30010,
        mark: 30000,
        index: 30000,
        percentage: 0,
        openInterest: 0,
        fundingRate: 0,
        fundingTime: 0,
        volume: 0,
        quoteVolume: 0,
      },
    },
    orders: [],
    positions: [
      {
        id: 'BTCUSDT:long',
        symbol: 'BTCUSDT',
        side: PositionSide.Long,
        entryPrice: 30000,
        notional: 30000,
        leverage: 10,
        unrealizedPnl: 0,
        contracts: 1,
        liquidationPrice: 0,
        positionMode: PositionMode.Hedged,
      },
    ],
  }) as any;

const createExchange = () => {
  const exchange = new BybitExchange(
    'bybit',
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
    } as any,
    createStore(),
  );
  const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'oid-1' } } });
  exchange.xhr = { post } as any;
  return { exchange, post };
};

const rawOrder = (overrides: Record<string, any> = {}) => ({
  orderId: 'oid-1',
  orderStatus: 'Untriggered',
  symbol: 'BTCUSDT',
  side: 'Buy',
  orderType: 'Market',
  stopOrderType: 'Stop',
  triggerPrice: '30500',
  price: '',
  qty: '0.5',
  leavesQty: '0.5',
  cumExecQty: '0',
  reduceOnly: false,
  closeOnTrigger: false,
  timeInForce: 'GTC',
  triggerBy: 'MarkPrice',
  ...overrides,
});

describe('Bybit conditional orders', () => {
  it('places an opening conditional market trigger order', async () => {
    const { exchange, post } = createExchange();

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Conditional,
      side: OrderSide.Buy,
      amount: 0.5,
      triggerPrice: 30500,
      reduceOnly: true,
    });

    expect(ids).toEqual(['oid-1']);
    expect(post).toHaveBeenCalledWith('/v5/order/create', {
      category: 'linear',
      symbol: 'BTCUSDT',
      side: 'Buy',
      orderType: 'Market',
      qty: '0.5',
      reduceOnly: false,
      triggerPrice: '30500',
      triggerDirection: '1',
      triggerBy: 'LastPrice',
      closeOnTrigger: false,
      positionIdx: 1,
    });
  });

  it('places an opening conditional limit trigger order', async () => {
    const { exchange, post } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Conditional,
      side: OrderSide.Sell,
      amount: 0.25,
      price: 29400,
      triggerPrice: 29500,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    expect(post).toHaveBeenCalledWith('/v5/order/create', {
      category: 'linear',
      symbol: 'BTCUSDT',
      side: 'Sell',
      orderType: 'Limit',
      qty: '0.25',
      price: '29400',
      reduceOnly: false,
      triggerPrice: '29500',
      triggerDirection: '2',
      triggerBy: 'LastPrice',
      timeInForce: 'PostOnly',
      closeOnTrigger: false,
      positionIdx: 2,
    });
  });

  it('places an opening conditional trigger order with mark-price trigger source when requested', async () => {
    const { exchange, post } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Conditional,
      side: OrderSide.Buy,
      amount: 0.5,
      triggerPrice: 30500,
      triggerType: OrderStopTriggerType.MarkPrice,
    });

    expect(post).toHaveBeenCalledWith(
      '/v5/order/create',
      expect.objectContaining({
        triggerBy: 'MarkPrice',
      }),
    );
  });

  it('places an opening conditional trigger order with index-price trigger source when requested', async () => {
    const { exchange, post } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTCUSDT',
      type: OrderType.Conditional,
      side: OrderSide.Buy,
      amount: 0.5,
      triggerPrice: 30005,
      triggerType: OrderStopTriggerType.IndexPrice,
    });

    expect(post).toHaveBeenCalledWith(
      '/v5/order/create',
      expect.objectContaining({
        triggerDirection: '1',
        triggerBy: 'IndexPrice',
      }),
    );
  });

  it('places batch opening conditional trigger orders with last-price trigger source', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValueOnce({
      data: {
        retMsg: 'OK',
        result: { list: [{ orderId: 'batch-conditional-1' }] },
        retExtInfo: { list: [{ code: 0, msg: 'OK' }] },
      },
    });

    const ids = await exchange.placeOrders([
      {
        symbol: 'BTCUSDT',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 0.5,
        triggerPrice: 30500,
      },
    ]);

    expect(ids).toEqual(['batch-conditional-1']);
    expect(post).toHaveBeenCalledWith('/v5/order/create-batch', {
      category: 'linear',
      request: [
        {
          symbol: 'BTCUSDT',
          side: 'Buy',
          orderType: 'Market',
          qty: '0.5',
          reduceOnly: false,
          triggerPrice: '30500',
          triggerDirection: '1',
          triggerBy: 'LastPrice',
          closeOnTrigger: false,
          positionIdx: 1,
        },
      ],
    });
  });

  it('places batch opening conditional trigger orders with index-price trigger source', async () => {
    const { exchange, post } = createExchange();
    post.mockResolvedValueOnce({
      data: {
        retMsg: 'OK',
        result: { list: [{ orderId: 'batch-conditional-index' }] },
        retExtInfo: { list: [{ code: 0, msg: 'OK' }] },
      },
    });

    const ids = await exchange.placeOrders([
      {
        symbol: 'BTCUSDT',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 0.5,
        triggerPrice: 30005,
        triggerType: OrderStopTriggerType.IndexPrice,
      },
    ]);

    expect(ids).toEqual(['batch-conditional-index']);
    expect(post).toHaveBeenCalledWith('/v5/order/create-batch', {
      category: 'linear',
      request: [
        expect.objectContaining({
          triggerDirection: '1',
          triggerBy: 'IndexPrice',
        }),
      ],
    });
  });

  it('maps opening Bybit trigger orders to conditional', () => {
    const { exchange } = createExchange();

    const [order] = exchange.mapOrder(rawOrder());

    expect(order.type).toBe(OrderType.Conditional);
    expect(order.price).toBe(30500);
    expect(order.reduceOnly).toBe(false);
    expect(order.close).toBe(false);
    expect(order.triggerType).toBe(OrderStopTriggerType.MarkPrice);
  });

  it('maps opening Bybit trigger limit orders to conditional with trigger price as display price', () => {
    const { exchange } = createExchange();

    const [order] = exchange.mapOrder(
      rawOrder({
        orderType: 'Limit',
        triggerPrice: '29500',
        price: '29400',
      }),
    );

    expect(order.type).toBe(OrderType.Conditional);
    expect(order.price).toBe(29500);
    expect(order.info.price).toBe('29400');
  });

  it('maps Bybit websocket opening trigger partials to conditional', () => {
    const { exchange } = createExchange();

    const [order] = exchange.mapOrder(
      rawOrder({
        createType: 'CreateByStopOrder',
        stopOrderType: 'StopLoss',
        tpslMode: 'UNKNOWN',
      }),
    );

    expect(order.type).toBe(OrderType.Conditional);
    expect(order.price).toBe(30500);
  });

  it('amends opening conditional trigger price directly', async () => {
    const { exchange, post } = createExchange();
    const [order] = exchange.mapOrder(
      rawOrder({
        orderId: 'conditional-limit-1',
        orderType: 'Limit',
        triggerPrice: '29500',
        price: '29400',
      }),
    );
    post.mockClear();

    const ids = await exchange.updateOrder({
      order,
      update: {
        price: 29600,
        amount: 0.3,
      },
    });

    expect(ids).toEqual(['oid-1']);
    expect(post).toHaveBeenCalledWith('/v5/order/amend', {
      category: 'linear',
      orderId: 'conditional-limit-1',
      symbol: 'BTCUSDT',
      qty: '0.3',
      triggerPrice: '29600',
    });
  });

  it('amends websocket conditional trigger partials directly', async () => {
    const { exchange, post } = createExchange();
    const [order] = exchange.mapOrder(
      rawOrder({
        orderId: 'conditional-ws-1',
        createType: 'CreateByStopOrder',
        stopOrderType: 'StopLoss',
        tpslMode: 'UNKNOWN',
      }),
    );
    post.mockClear();

    await exchange.updateOrder({
      order,
      update: {
        price: 30600,
      },
    });

    expect(post).toHaveBeenCalledWith('/v5/order/amend', {
      category: 'linear',
      orderId: 'conditional-ws-1',
      symbol: 'BTCUSDT',
      triggerPrice: '30600',
    });
  });

  it('keeps close-only and position TP/SL mappings intact', () => {
    const { exchange } = createExchange();

    const [closeOnlyStop] = exchange.mapOrder(
      rawOrder({
        orderId: 'sl-1',
        side: 'Sell',
        stopOrderType: 'StopLoss',
        reduceOnly: true,
        closeOnTrigger: true,
      }),
    );
    const [positionTakeProfit] = exchange.mapOrder(
      rawOrder({
        orderId: 'tp-1',
        side: 'Sell',
        stopOrderType: 'TakeProfit',
        tpslMode: 'Full',
        closeOnTrigger: true,
      }),
    );

    expect(closeOnlyStop.type).toBe(OrderType.StopLoss);
    expect(positionTakeProfit.type).toBe(OrderType.TakeProfit);
  });

  it('uses createType to keep transient position TP/SL rows protective', () => {
    const { exchange } = createExchange();

    const [positionTakeProfit] = exchange.mapOrder(
      rawOrder({
        orderId: 'tp-reconciling',
        side: 'Sell',
        orderType: 'Limit',
        stopOrderType: 'UNKNOWN',
        createType: 'CreateByTakeProfit',
        tpslMode: 'Full',
        reduceOnly: true,
        closeOnTrigger: true,
        triggerPrice: '32000',
        price: '30000',
      }),
    );
    const [positionStopLoss] = exchange.mapOrder(
      rawOrder({
        orderId: 'sl-reconciling',
        side: 'Sell',
        orderType: 'Limit',
        stopOrderType: 'UNKNOWN',
        createType: 'CreateByPartialStopLoss',
        tpslMode: 'Partial',
        reduceOnly: true,
        closeOnTrigger: true,
        triggerPrice: '29000',
        price: '30000',
      }),
    );

    expect(positionTakeProfit.type).toBe(OrderType.TakeProfit);
    expect(positionTakeProfit.price).toBe(32000);
    expect(positionTakeProfit.amount).toBe(0);
    expect(positionStopLoss.type).toBe(OrderType.StopLoss);
    expect(positionStopLoss.price).toBe(29000);
  });

  it('keeps Bybit websocket position TP/SL partials protective', () => {
    const { exchange } = createExchange();

    const [positionStopLoss] = exchange.mapOrder(
      rawOrder({
        createType: 'CreateByStopLoss',
        stopOrderType: 'StopLoss',
        tpslMode: 'Partial',
      }),
    );

    expect(positionStopLoss.type).toBe(OrderType.StopLoss);
  });
});
