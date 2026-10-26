import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import type { ORDER_RESPONSE } from '../reference/paradex/paradex.types';
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

  exchange.xhr = { get } as unknown as Axios;
  exchange.jwtManager = {} as any;
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  (exchange.privateWebsocket as any).throttledFetchOrders = vi.fn();

  return { exchange, store, get };
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

  it('maps the current flags-based reduce-only response contract from REST and WebSocket', async () => {
    const { exchange, store, get } = createExchange();
    const currentResponse = {
      ...orderResponse('STOP_LOSS_MARKET'),
      reduce_only: undefined,
      flags: ['REDUCE_ONLY'],
      instruction: 'GTC',
    };
    get.mockResolvedValue({ data: { results: [currentResponse] } });

    await expect(exchange.fetchOrders()).resolves.toEqual([expect.objectContaining({ reduceOnly: true })]);

    (exchange.privateWebsocket as any).handleOrderUpdate({ data: currentResponse });
    expect(store.orders).toContainEqual(expect.objectContaining({ id: currentResponse.id, reduceOnly: true }));
  });

});
