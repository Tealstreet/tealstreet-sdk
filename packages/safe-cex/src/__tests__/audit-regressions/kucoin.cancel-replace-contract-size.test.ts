import type { Axios } from 'axios';

import { KucoinExchange } from '../reference/kucoin/kucoin.exchange';
import { DefaultStore } from '../../store/store.base';
import { Market, Order, OrderType } from '../../types';

const MARKET: Market = {
  id: 'XBTUSDTM',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: {
    amount: { min: 1, max: 1_000_000 },
    leverage: { min: 1, max: 100 },
  },
  contractSize: 0.001,
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });

  const exchange = new KucoinExchange(
    'kucoin',
    {
      key: 'test-key',
      secret: 'test-secret',
      passphrase: 'test-passphrase',
      tealstreetTunnel: false,
    },
    store,
  );
  const getRequest = vi.fn();
  const deleteRequest = vi.fn().mockResolvedValue({ data: { code: '200000' } });
  const postRequest = vi.fn().mockResolvedValue({
    data: { code: '200000', data: { orderId: 'replacement-order-id' } },
  });

  exchange.unlimitedXHR = {
    delete: deleteRequest,
    post: postRequest,
  } as unknown as Axios;
  exchange.xhr = { get: getRequest } as unknown as Axios;

  return { exchange, store, getRequest, deleteRequest, postRequest };
};

const mapOrder = (exchange: KucoinExchange, overrides: Record<string, unknown> = {}): Order[] =>
  exchange.mapOrder({
    id: 'order-id',
    symbol: 'XBTUSDTM',
    status: 'open',
    type: 'limit',
    side: 'buy',
    price: '50000',
    size: '5',
    filledSize: '0',
    reduceOnly: false,
    timeInForce: 'GTC',
    ...overrides,
  });

describe('KuCoin cancel-replace contract sizing', () => {
  it('re-places a normal order with native contracts after a price-only update', async () => {
    const { exchange, store, deleteRequest, postRequest } = createExchange();
    const [order] = mapOrder(exchange);
    store.update({ orders: [order] });

    expect(order.amount).toBe(0.005);

    await exchange.updateOrder({ order, update: { price: 51_000 } });

    expect(deleteRequest).toHaveBeenCalledWith('/api/v1/orders/order-id', {
      params: { symbol: 'XBTUSDTM' },
    });
    expect(postRequest).toHaveBeenCalledWith(
      '/api/v1/orders',
      expect.objectContaining({ symbol: 'XBTUSDTM', price: '51000', size: '5' }),
    );
  });

  it('uses an authoritative remainSize from a partial-fill frame for replacement sizing', async () => {
    const { exchange, store, postRequest } = createExchange();
    const [order] = mapOrder(exchange, {
      id: 'partial-frame-order-id',
      size: '19982',
      filledSize: '0',
      remainSize: '11618',
    });
    store.update({ orders: [order] });

    expect(order.amount).toBe(19.982);
    expect(order.remaining).toBe(11.618);
    expect(order.extra?.remainingNative).toBe(11_618);
    expect(order.extra?.filledInPlacementUnits).toBe(0);

    await exchange.updateOrder({ order, update: { price: 51_000 } });

    expect(postRequest).toHaveBeenCalledWith(
      '/api/v1/orders',
      expect.objectContaining({ symbol: 'XBTUSDTM', price: '51000', size: '11618' }),
    );
  });

  it('keeps the WS native remainder through a REST full replacement before a price-only update', async () => {
    const { exchange, store, getRequest, postRequest } = createExchange();
    const [websocketOrder] = mapOrder(exchange, {
      id: 'ws-then-rest-order-id',
      size: '19982',
      filledSize: '0',
      remainSize: '11618',
    });
    store.addOrUpdateOrder(websocketOrder);

    // KuCoin's REST Get Order List example supplies size/filledSize, but no
    // remainSize. This is the 500 ms polling response that full-replaces the
    // order store after the authoritative websocket update above.
    getRequest
      .mockResolvedValueOnce({
        data: {
          code: '200000',
          data: { items: [{ ...websocketOrder.info, remainSize: undefined }] },
        },
      })
      .mockResolvedValueOnce({ data: { code: '200000', data: { items: [] } } });

    const orders = await exchange.fetchOrders();
    store.update({ orders });

    const [restReplacedOrder] = store.orders;
    expect(restReplacedOrder.extra?.remainingNative).toBe(11_618);
    expect(restReplacedOrder.remaining).toBe(11.618);

    await exchange.updateOrder({ order: restReplacedOrder, update: { price: 51_000 } });

    expect(postRequest).toHaveBeenCalledWith(
      '/api/v1/orders',
      expect.objectContaining({ symbol: 'XBTUSDTM', price: '51000', size: '11618' }),
    );
  });

  it('accepts a smaller REST-derived remainder but never turns an explicit WS zero back into size', async () => {
    const { exchange, store, getRequest } = createExchange();
    const [partiallyFilled] = mapOrder(exchange, {
      id: 'rest-progress-order-id',
      size: '20',
      filledSize: '0',
      remainSize: '15',
    });
    const [zeroRemaining] = mapOrder(exchange, {
      id: 'explicit-zero-order-id',
      size: '20',
      filledSize: '0',
      remainSize: 0,
    });
    store.update({ orders: [partiallyFilled, zeroRemaining] });
    getRequest
      .mockResolvedValueOnce({
        data: {
          code: '200000',
          data: {
            items: [
              { ...partiallyFilled.info, remainSize: undefined, filledSize: '8' },
              { ...zeroRemaining.info, remainSize: undefined },
            ],
          },
        },
      })
      .mockResolvedValueOnce({ data: { code: '200000', data: { items: [] } } });

    const orders = await exchange.fetchOrders();

    expect(orders.find((order) => order.id === partiallyFilled.id)?.extra?.remainingNative).toBe(12);
    expect(orders.find((order) => order.id === zeroRemaining.id)?.extra?.remainingNative).toBe(0);
  });

  it('does not inherit a prior remainder for a new cancel-replace order id', async () => {
    const { exchange, store, getRequest } = createExchange();
    const [previousOrder] = mapOrder(exchange, {
      id: 'cancelled-order-id',
      size: '20',
      filledSize: '0',
      remainSize: '5',
    });
    store.update({ orders: [previousOrder] });
    getRequest
      .mockResolvedValueOnce({
        data: {
          code: '200000',
          data: { items: [{ ...previousOrder.info, id: 'replacement-order-id', remainSize: undefined }] },
        },
      })
      .mockResolvedValueOnce({ data: { code: '200000', data: { items: [] } } });

    const [replacementOrder] = await exchange.fetchOrders();

    expect(replacementOrder.id).toBe('replacement-order-id');
    expect(replacementOrder.extra?.remainingNative).toBe(20);
  });

  it('preserves an explicit zero remainSize instead of deriving a remainder', () => {
    const { exchange } = createExchange();
    const [order] = mapOrder(exchange, {
      id: 'zero-remaining-order-id',
      size: '5',
      filledSize: '0',
      remainSize: 0,
    });

    expect(order.remaining).toBe(0);
    expect(order.extra?.remainingNative).toBe(0);
  });

  it('converts base remaining when native remainder metadata is absent', async () => {
    const { exchange, store, postRequest } = createExchange();
    const [mappedOrder] = mapOrder(exchange, {
      id: 'legacy-partial-order-id',
      size: '20',
      filledSize: '9',
    });
    expect(mappedOrder.extra?.filledInPlacementUnits).toBe(9);
    const order: Order = { ...mappedOrder, extra: undefined };
    store.update({ orders: [order] });

    expect(order.amount).toBe(0.02);
    expect(order.remaining).toBe(0.011);

    await exchange.updateOrder({ order, update: { price: 51_000 } });

    expect(postRequest).toHaveBeenCalledWith(
      '/api/v1/orders',
      expect.objectContaining({ symbol: 'XBTUSDTM', price: '51000', size: '11' }),
    );
  });

  it('does not convert an explicit native amount update a second time', async () => {
    const { exchange, store, postRequest } = createExchange();
    const [order] = mapOrder(exchange, { id: 'amount-update-order-id' });
    store.update({ orders: [order] });

    await exchange.updateOrder({ order, update: { amount: 7 } });

    expect(postRequest).toHaveBeenCalledWith(
      '/api/v1/orders',
      expect.objectContaining({ symbol: 'XBTUSDTM', size: '7' }),
    );
  });

  it('re-places an algo order with native contracts after a price-only update', async () => {
    const { exchange, store, deleteRequest, postRequest } = createExchange();
    const [order] = mapOrder(exchange, {
      id: 'stop-order-id',
      type: 'market',
      side: 'sell',
      stop: 'down',
      stopPrice: '49000',
    });
    store.update({ orders: [order] });

    expect(order.type).toBe(OrderType.StopLoss);
    expect(order.amount).toBe(0.005);

    await exchange.updateOrder({ order, update: { price: 48_000 } });

    expect(deleteRequest).toHaveBeenCalledWith('/api/v1/orders/stop-order-id', {
      params: { symbol: 'XBTUSDTM' },
    });
    expect(postRequest).toHaveBeenCalledWith(
      '/api/v1/orders',
      expect.objectContaining({ symbol: 'XBTUSDTM', stopPrice: '48000', size: '5' }),
    );
  });

  it('re-places a compound order with native contracts after a virtual-leg price update', async () => {
    const { exchange, store, deleteRequest, postRequest } = createExchange();
    const orders = mapOrder(exchange, {
      id: 'compound-order-id',
      triggerStopDownPrice: '49000',
      triggerStopUpPrice: '52000',
    });
    const stopLossOrder = orders.find((order) => order.id.endsWith('__stop_loss'))!;
    store.update({ orders });

    expect(orders[0].amount).toBe(0.005);

    await exchange.updateOrder({ order: stopLossOrder, update: { price: 48_500 } });

    expect(deleteRequest).toHaveBeenCalledWith('/api/v1/orders/compound-order-id', {
      params: { symbol: 'XBTUSDTM' },
    });
    expect(postRequest).toHaveBeenCalledWith(
      '/api/v1/st-orders',
      expect.objectContaining({
        symbol: 'XBTUSDTM',
        size: '5',
        triggerStopDownPrice: '48500',
        triggerStopUpPrice: '52000',
      }),
    );
  });
});
