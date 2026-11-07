import { describe, expect, it, vi } from 'vitest';

import { ENDPOINTS } from '../../exchanges/bybit/bybit.types';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PositionMode } from '../../types';

const createStore = () => {
  const market = {
    id: 'BTCUSD',
    symbol: 'BTCUSD',
    base: 'BTC',
    quote: 'USD',
    active: true,
    precision: {
      amount: 1,
      price: 0.1,
    },
    limits: {
      amount: {
        min: 1,
        max: 100,
      },
      leverage: {
        min: 1,
        max: 100,
      },
    },
    isInverse: true,
  };

  return {
    subscribe: vi.fn(),
    markets: [market],
    marketsBySymbol: {
      BTCUSD: market,
    },
    tickersBySymbol: {
      BTCUSD: {
        id: 'BTCUSD',
        symbol: 'BTCUSD',
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
    positions: [],
  } as any;
};

const createExchange = () => {
  const exchange = new BybitInverseExchange(
    'bybitinverse',
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
    } as any,
    createStore(),
  );
  const post = vi.fn().mockResolvedValue({ data: { retMsg: 'OK', result: { orderId: 'oid-1' } } });
  exchange.xhr = { post } as any;
  (exchange as any).fetchPositionMode = vi.fn().mockResolvedValue(PositionMode.Hedged);
  return { exchange, post };
};

const rawOrder = (overrides: Record<string, any> = {}) => ({
  orderId: 'oid-1',
  orderStatus: 'Untriggered',
  symbol: 'BTCUSD',
  side: 'Buy',
  orderType: 'Market',
  stopOrderType: 'Stop',
  triggerPrice: '30500',
  price: '',
  qty: '25',
  leavesQty: '25',
  cumExecQty: '0',
  reduceOnly: false,
  closeOnTrigger: false,
  timeInForce: 'GTC',
  triggerBy: 'MarkPrice',
  ...overrides,
});

describe('audit regression: Bybit inverse opening conditional orders', () => {
  it('places an opening conditional market trigger order with integer USD contracts', async () => {
    const { exchange, post } = createExchange();

    const ids = await exchange.placeOrder({
      symbol: 'BTCUSD',
      type: OrderType.Conditional,
      side: OrderSide.Buy,
      amount: 25.4,
      triggerPrice: 30500,
      reduceOnly: true,
    });

    expect(ids).toEqual(['oid-1']);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.CREATE_ORDER, {
      category: 'inverse',
      symbol: 'BTCUSD',
      side: 'Buy',
      orderType: 'Market',
      qty: '25',
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
      symbol: 'BTCUSD',
      type: OrderType.Conditional,
      side: OrderSide.Sell,
      amount: 25,
      price: 29400,
      triggerPrice: 29500,
      timeInForce: OrderTimeInForce.PostOnly,
    });

    expect(post).toHaveBeenCalledWith(ENDPOINTS.CREATE_ORDER, {
      category: 'inverse',
      symbol: 'BTCUSD',
      side: 'Sell',
      orderType: 'Limit',
      qty: '25',
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
      symbol: 'BTCUSD',
      type: OrderType.Conditional,
      side: OrderSide.Buy,
      amount: 25,
      triggerPrice: 30500,
      triggerType: OrderStopTriggerType.MarkPrice,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.CREATE_ORDER,
      expect.objectContaining({
        triggerBy: 'MarkPrice',
      }),
    );
  });

  it('places an opening conditional trigger order with index-price trigger source when requested', async () => {
    const { exchange, post } = createExchange();

    await exchange.placeOrder({
      symbol: 'BTCUSD',
      type: OrderType.Conditional,
      side: OrderSide.Buy,
      amount: 25,
      triggerPrice: 30005,
      triggerType: OrderStopTriggerType.IndexPrice,
    });

    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.CREATE_ORDER,
      expect.objectContaining({
        triggerDirection: '1',
        triggerBy: 'IndexPrice',
      }),
    );
  });

  it('places batch opening conditional trigger orders', async () => {
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
        symbol: 'BTCUSD',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 25,
        triggerPrice: 30500,
      },
    ]);

    expect(ids).toEqual(['batch-conditional-1']);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.CREATE_BATCH_ORDER, {
      category: 'inverse',
      request: [
        {
          symbol: 'BTCUSD',
          side: 'Buy',
          orderType: 'Market',
          qty: '25',
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
        symbol: 'BTCUSD',
        type: OrderType.Conditional,
        side: OrderSide.Buy,
        amount: 25,
        triggerPrice: 30005,
        triggerType: OrderStopTriggerType.IndexPrice,
      },
    ]);

    expect(ids).toEqual(['batch-conditional-index']);
    expect(post).toHaveBeenCalledWith(ENDPOINTS.CREATE_BATCH_ORDER, {
      category: 'inverse',
      request: [
        expect.objectContaining({
          triggerDirection: '1',
          triggerBy: 'IndexPrice',
        }),
      ],
    });
  });

  it('maps REST and websocket opening trigger orders to conditional', () => {
    const { exchange } = createExchange();

    const [restOrder] = exchange.mapOrder(rawOrder());
    const [wsOrder] = exchange.mapOrder(
      rawOrder({
        orderId: 'ws-conditional-1',
        createType: 'CreateByStopOrder',
        stopOrderType: 'StopLoss',
        tpslMode: 'UNKNOWN',
      }),
    );

    expect(restOrder.type).toBe(OrderType.Conditional);
    expect(restOrder.price).toBe(30500);
    expect(restOrder.reduceOnly).toBe(false);
    expect(restOrder.close).toBe(false);
    expect(restOrder.triggerType).toBe(OrderStopTriggerType.MarkPrice);
    expect(wsOrder.type).toBe(OrderType.Conditional);
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

    await exchange.updateOrder({
      order,
      update: {
        price: 29600,
        amount: 30,
      },
    });

    expect(post).toHaveBeenCalledWith(ENDPOINTS.REPLACE_ORDER, {
      category: 'inverse',
      orderId: 'conditional-limit-1',
      symbol: 'BTCUSD',
      qty: '30',
      triggerPrice: '29600',
    });
  });

  it('keeps close-only and position TP/SL mappings protective', () => {
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
        createType: 'CreateByPartialTakeProfit',
        tpslMode: 'Partial',
        reduceOnly: true,
        closeOnTrigger: true,
        triggerPrice: '32000',
        price: '30000',
      }),
    );

    expect(positionTakeProfit.type).toBe(OrderType.TakeProfit);
    expect(positionTakeProfit.price).toBe(32000);
  });
});
