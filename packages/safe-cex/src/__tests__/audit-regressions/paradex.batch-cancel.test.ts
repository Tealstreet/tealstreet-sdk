import type { Axios } from 'axios';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { ENDPOINTS } from '../reference/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';
import type { Order } from '../../types';

const order = (id: string): Order => ({ id }) as Order;

const createExchange = () => {
  const exchange = new ParadexExchange(
    'paradex',
    { publicDataOnly: true, tuning: { consoleLogging: false } } as any,
    new DefaultStore(),
  );
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return exchange;
};

const acceptedResults = (orderIds: string[]) =>
  [...orderIds]
    .reverse()
    .map((id, index) => ({ id, status: index % 2 === 0 ? 'ALREADY_CLOSED' : 'QUEUED_FOR_CANCELLATION' }));

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Paradex native batch cancellation', () => {
  it('uses only exact two-ID DELETE batches and preserves a one-order single-cancel tail', async () => {
    const exchange = createExchange();
    const deleteRequest = vi.fn(async (endpoint: string, config?: any) => {
      if (endpoint === ENDPOINTS.CANCEL_ORDERS_BATCH) {
        return { data: { results: acceptedResults(config.data.order_ids) } };
      }
      return { data: undefined };
    });
    exchange.xhr = { delete: deleteRequest } as unknown as Axios;

    await expect(exchange.cancelOrders(['one', 'two', 'three', 'four', 'five'].map(order))).resolves.toBeUndefined();

    expect(deleteRequest).toHaveBeenCalledTimes(3);
    expect(deleteRequest.mock.calls[0]).toEqual([
      ENDPOINTS.CANCEL_ORDERS_BATCH,
      { data: { order_ids: ['one', 'two'] } },
    ]);
    expect(deleteRequest.mock.calls[1]).toEqual([
      ENDPOINTS.CANCEL_ORDERS_BATCH,
      { data: { order_ids: ['three', 'four'] } },
    ]);
    expect(deleteRequest.mock.calls[2]).toEqual([ENDPOINTS.CANCEL_ORDER.replace('{id}', 'five')]);
  });

  it('attempts later pairs and the existing single tail before propagating a correlated NOT_FOUND', async () => {
    const exchange = createExchange();
    const deleteRequest = vi.fn(async (endpoint: string, config?: any) => {
      if (endpoint !== ENDPOINTS.CANCEL_ORDERS_BATCH) return { data: undefined };
      const ids = config.data.order_ids as string[];
      if (ids[0] === 'one') {
        return {
          data: {
            results: [
              { id: 'two', status: 'QUEUED_FOR_CANCELLATION' },
              { id: 'one', status: 'NOT_FOUND' },
            ],
          },
        };
      }
      return { data: { results: acceptedResults(ids) } };
    });
    exchange.xhr = { delete: deleteRequest } as unknown as Axios;

    await expect(exchange.cancelOrders(['one', 'two', 'three', 'four', 'five'].map(order))).rejects.toThrow(
      'Paradex batch cancel rejected order IDs [one]: NOT_FOUND',
    );

    expect(deleteRequest.mock.calls.map(([endpoint]) => endpoint)).toEqual([
      ENDPOINTS.CANCEL_ORDERS_BATCH,
      ENDPOINTS.CANCEL_ORDERS_BATCH,
      ENDPOINTS.CANCEL_ORDER.replace('{id}', 'five'),
    ]);
  });

  it('keeps identity-less NOT_FOUND uncorrelated and reports the exact missing requested-ID set', async () => {
    const exchange = createExchange();
    exchange.xhr = {
      delete: vi.fn(async () => ({
        data: {
          results: [
            { id: 'two', status: 'QUEUED_FOR_CANCELLATION' },
            { status: 'NOT_FOUND' },
          ],
        },
      })),
    } as unknown as Axios;

    const promise = exchange.cancelOrders([order('one'), order('two')]);
    await expect(promise).rejects.toThrow('results[1] has uncorrelated NOT_FOUND without an order id');
    await expect(promise).rejects.toThrow('missing requested IDs [one]');
  });

  it.each([
    ['a missing envelope', () => undefined],
    ['a non-array results field', () => ({ results: {} })],
    [
      'a missing result',
      (ids: string[]) => ({ results: [{ id: ids[0], status: 'QUEUED_FOR_CANCELLATION' }] }),
    ],
    [
      'a duplicate identity',
      (ids: string[]) => ({
        results: [
          { id: ids[0], status: 'QUEUED_FOR_CANCELLATION' },
          { id: ids[0], status: 'ALREADY_CLOSED' },
        ],
      }),
    ],
    [
      'an unknown identity',
      (ids: string[]) => ({
        results: [
          { id: 'unknown', status: 'QUEUED_FOR_CANCELLATION' },
          { id: ids[1], status: 'ALREADY_CLOSED' },
        ],
      }),
    ],
    [
      'a non-object row',
      (ids: string[]) => ({ results: [null, { id: ids[1], status: 'QUEUED_FOR_CANCELLATION' }] }),
    ],
    [
      'an unsupported status',
      (ids: string[]) => ({
        results: [
          { id: ids[0], status: 'PENDING' },
          { id: ids[1], status: 'QUEUED_FOR_CANCELLATION' },
        ],
      }),
    ],
    [
      'a row without identity',
      (ids: string[]) => ({
        results: [
          { status: 'QUEUED_FOR_CANCELLATION' },
          { id: ids[1], status: 'ALREADY_CLOSED' },
        ],
      }),
    ],
  ])('rejects %s without positional correlation', async (_label, makeResponse) => {
    const exchange = createExchange();
    exchange.xhr = {
      delete: vi.fn(async (_endpoint: string, config: any) => ({ data: makeResponse(config.data.order_ids) })),
    } as unknown as Axios;

    await expect(exchange.cancelOrders([order('one'), order('two')])).rejects.toThrow(
      'Paradex batch cancel response malformed',
    );
  });

  it('aggregates exact chunk contexts only after every pair and tail has been attempted', async () => {
    const exchange = createExchange();
    const deleteRequest = vi.fn(async (endpoint: string, config?: any) => {
      if (endpoint !== ENDPOINTS.CANCEL_ORDERS_BATCH) throw new Error('tail transport failure');
      const ids = config.data.order_ids as string[];
      if (ids[0] === 'one') {
        return {
          data: {
            results: [
              { id: 'unknown', status: 'QUEUED_FOR_CANCELLATION' },
              { id: 'two', status: 'ALREADY_CLOSED' },
            ],
          },
        };
      }
      throw new Error('second batch transport failure');
    });
    exchange.xhr = { delete: deleteRequest } as unknown as Axios;

    const promise = exchange.cancelOrders(['one', 'two', 'three', 'four', 'five'].map(order));
    await expect(promise).rejects.toThrow('Paradex cancellation failed for 3 chunks after attempting all orders');
    await expect(promise).rejects.toThrow('[one, two]: Paradex batch cancel response malformed');
    await expect(promise).rejects.toThrow('[three, four]: second batch transport failure');
    await expect(promise).rejects.toThrow('[five]: tail transport failure');
    expect(deleteRequest).toHaveBeenCalledTimes(3);
  });

  it('rejects duplicate pair IDs before a batch write but still attempts the single tail', async () => {
    const exchange = createExchange();
    const deleteRequest = vi.fn(async () => ({ data: undefined }));
    exchange.xhr = { delete: deleteRequest } as unknown as Axios;

    await expect(exchange.cancelOrders([order('same'), order('same'), order('tail')])).rejects.toThrow(
      'Paradex native cancel batch received duplicate order IDs [same, same]',
    );
    expect(deleteRequest.mock.calls).toEqual([[ENDPOINTS.CANCEL_ORDER.replace('{id}', 'tail')]]);
  });
});
