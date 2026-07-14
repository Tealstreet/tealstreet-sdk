import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../../exchanges/paradex/paradex.exchange';
import type { ORDER_RESPONSE } from '../../exchanges/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';
import { OrderStatus, OrderType } from '../../types';

const ORDER_VARIANTS = [
  ['STOP_MARKET', OrderType.StopLoss],
  ['STOP_LIMIT', OrderType.StopLoss],
  ['STOP_LOSS_MARKET', OrderType.StopLoss],
  ['STOP_LOSS_LIMIT', OrderType.StopLoss],
  ['TAKE_PROFIT_MARKET', OrderType.TakeProfit],
  ['TAKE_PROFIT_LIMIT', OrderType.TakeProfit],
] as const;

const orderResponse = (type: ORDER_RESPONSE['type']): ORDER_RESPONSE =>
  ({
    id: `${type.toLowerCase()}-id`,
    client_order_id: '',
    market: 'BTC-USD-PERP',
    side: 'SELL',
    size: '2',
    remaining_size: '2',
    price: '87',
    average_fill_price: '0',
    status: 'UNTRIGGERED',
    type,
    time_in_force: 'GTT',
    created_at: '2026-07-14T00:00:00.000Z',
    updated_at: '2026-07-14T00:00:00.000Z',
    expires_at: '',
    trigger_price: '91',
    post_only: false,
    reduce_only: true,
  }) as ORDER_RESPONSE;

const createExchange = () => {
  const store = new DefaultStore();
  const exchange = new ParadexExchange('paradex', { publicDataOnly: true } as any, store);
  const get = vi.fn();
  const post = vi.fn().mockResolvedValue({ data: { id: 'replacement-order-id' } });
  const deleteRequest = vi.fn().mockResolvedValue({ data: {} });
  const signOrder = vi.fn().mockResolvedValue({ signature: '["r","s"]', signature_timestamp: 1_700_000_000_000 });

  exchange.xhr = { get, post, delete: deleteRequest } as unknown as Axios;
  exchange.jwtManager = {} as any;
  exchange.requestSigner = { signOrder } as any;
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  (exchange.privateWebsocket as any).throttledFetchOrders = vi.fn();

  return { exchange, store, get, post, deleteRequest, signOrder };
};

describe('Paradex protective stop order regression', () => {
  it('maps every documented upstream stop and take-profit variant from REST readback', async () => {
    const { exchange, get } = createExchange();
    get.mockResolvedValue({ data: { results: ORDER_VARIANTS.map(([type]) => orderResponse(type)) } });

    const orders = await exchange.fetchOrders();

    expect(orders).toHaveLength(ORDER_VARIANTS.length);
    for (const [type, expectedType] of ORDER_VARIANTS) {
      const order = orders.find((candidate) => candidate.id === `${type.toLowerCase()}-id`);
      expect(order).toMatchObject({ type: expectedType, status: OrderStatus.Open, price: 91, reduceOnly: true });
    }
  });

  it.each(ORDER_VARIANTS)('maps %s from an orders.ALL WebSocket update', (type, expectedType) => {
    const { exchange, store } = createExchange();

    (exchange.privateWebsocket as any).handleOrderUpdate({ data: orderResponse(type) });

    expect(store.orders).toContainEqual(
      expect.objectContaining({
        id: `${type.toLowerCase()}-id`,
        type: expectedType,
        status: OrderStatus.Open,
        price: 91,
        reduceOnly: true,
      }),
    );
  });

  it.each(ORDER_VARIANTS)(
    'recreates %s as a protective trigger order after a price edit',
    async (type, expectedType) => {
      const { exchange, post, deleteRequest, signOrder } = createExchange();
      const order = (exchange as any).mapOrder(orderResponse(type));

      await expect(exchange.updateOrder({ order, update: { price: 92 } })).resolves.toEqual(['replacement-order-id']);

      expect(deleteRequest).toHaveBeenCalledWith(`/orders/${type.toLowerCase()}-id`);
      expect(signOrder).toHaveBeenCalledWith(
        expect.objectContaining({
          market: 'BTC-USD-PERP',
          side: 'SELL',
          type: expectedType === OrderType.StopLoss ? 'STOP_MARKET' : 'TAKE_PROFIT_MARKET',
        }),
      );
      expect(post).toHaveBeenCalledWith(
        '/orders',
        expect.objectContaining({
          type: expectedType === OrderType.StopLoss ? 'STOP_MARKET' : 'TAKE_PROFIT_MARKET',
          trigger_price: '92',
          flags: ['REDUCE_ONLY'],
        }),
      );
      expect(post).not.toHaveBeenCalledWith('/orders', expect.objectContaining({ type: 'LIMIT' }));
    },
  );
});
