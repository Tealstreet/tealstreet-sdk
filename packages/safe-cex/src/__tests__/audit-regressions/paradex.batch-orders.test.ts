import type { Axios } from 'axios';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { uuidMock } = vi.hoisted(() => ({ uuidMock: vi.fn() }));
vi.mock('../../utils/uuid', () => ({ uuid: uuidMock }));

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { ENDPOINTS } from '../reference/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';
import type { PlaceOrderOpts } from '../../types';
import { OrderSide, OrderTimeInForce, OrderType } from '../../types';

const regularOrder = (index: number, overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTC-USD-PERP',
  type: OrderType.Limit,
  side: index % 2 === 0 ? OrderSide.Buy : OrderSide.Sell,
  amount: 0.01 * (index + 1),
  price: 50_000 + index,
  ...overrides,
});

const createExchange = () => {
  const exchange = new ParadexExchange(
    'paradex',
    { publicDataOnly: true, tuning: { consoleLogging: false } } as any,
    new DefaultStore(),
  );
  let signed = 0;
  const signOrder = vi.fn(async () => {
    signed += 1;
    return { signature: `["r-${signed}","s-${signed}"]`, signature_timestamp: 1_784_505_600_000 + signed };
  });
  exchange.requestSigner = { signOrder } as any;
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return { exchange, signOrder };
};

let uuidIndex = 0;
beforeEach(() => {
  uuidIndex = 0;
  uuidMock.mockImplementation(() => `batchclient${++uuidIndex}`);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Paradex native regular-order batching', () => {
  it('chunks 21 plain orders as 10/10/1 and restores input order from out-of-order client identities', async () => {
    const { exchange, signOrder } = createExchange();
    const requestedIds: string[] = [];
    const post = vi.fn(async (endpoint: string, payloads: any[]) => {
      expect(endpoint).toBe(ENDPOINTS.PLACE_ORDERS_BATCH);
      const rows = payloads.map((payload) => {
        const inputIndex = requestedIds.length;
        requestedIds.push(`order-${inputIndex}`);
        return { id: `order-${inputIndex}`, client_id: payload.client_id };
      });
      return { data: { errors: [], orders: rows.reverse() } };
    });
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrders(Array.from({ length: 21 }, (_, index) => regularOrder(index)))).resolves.toEqual(
      Array.from({ length: 21 }, (_, index) => `order-${index}`),
    );

    expect(post).toHaveBeenCalledTimes(3);
    expect(post.mock.calls.map(([, payloads]) => payloads.length)).toEqual([10, 10, 1]);
    expect(signOrder).toHaveBeenCalledTimes(21);
    const payloads = post.mock.calls.flatMap(([, rows]) => rows);
    expect(new Set(payloads.map(({ client_id }) => client_id)).size).toBe(21);
    expect(payloads.every(({ client_id }) => /^ts_[A-Za-z0-9]+$/.test(client_id) && client_id.length <= 64)).toBe(
      true,
    );
    expect(new Set(payloads.map(({ signature_timestamp }) => signature_timestamp)).size).toBe(21);
  });

  it('preserves mixed-family execution order and keeps MARKET, conditional, and attached orders on dedicated single paths', async () => {
    const { exchange } = createExchange();
    let singleId = 0;
    const endpoints: string[] = [];
    const post = vi.fn(async (endpoint: string, body: any) => {
      endpoints.push(endpoint);
      if (endpoint === ENDPOINTS.PLACE_ORDERS_BATCH) {
        return {
          data: {
            errors: [],
            orders: body.map((row: any, index: number) => ({ id: `batch-${endpoints.length}-${index}`, client_id: row.client_id })),
          },
        };
      }
      singleId += 1;
      return { data: { id: `single-${singleId}`, client_id: body.client_id } };
    });
    exchange.xhr = { post } as unknown as Axios;

    const ids = await exchange.placeOrders([
      regularOrder(0),
      regularOrder(1, { type: OrderType.Market, price: undefined }),
      regularOrder(2),
      regularOrder(3, { type: OrderType.Conditional, triggerPrice: 49_000 }),
      regularOrder(4),
      regularOrder(5, { type: OrderType.Limit, side: OrderSide.Buy, price: 50_000, takeProfit: 52_000 }),
      regularOrder(6, { reduceOnly: true, timeInForce: OrderTimeInForce.PostOnly }),
    ]);

    expect(endpoints).toEqual([
      ENDPOINTS.PLACE_ORDERS_BATCH,
      ENDPOINTS.PLACE_ORDER,
      ENDPOINTS.PLACE_ORDERS_BATCH,
      ENDPOINTS.PLACE_ORDER,
      ENDPOINTS.PLACE_ORDERS_BATCH,
      ENDPOINTS.PLACE_ORDER,
      ENDPOINTS.PLACE_ORDER,
      ENDPOINTS.PLACE_ORDERS_BATCH,
    ]);
    expect(ids).toEqual([
      'batch-1-0',
      'single-1',
      'batch-3-0',
      'single-2',
      'batch-5-0',
      'single-3',
      'single-4',
      'batch-8-0',
    ]);
    const batchRows = post.mock.calls
      .filter(([endpoint]) => endpoint === ENDPOINTS.PLACE_ORDERS_BATCH)
      .flatMap(([, rows]) => rows);
    expect(batchRows).toHaveLength(4);
    expect(batchRows[3]).toMatchObject({ flags: ['REDUCE_ONLY'], instruction: 'POST_ONLY' });
    expect(batchRows.every((row) => row.trigger_price === undefined)).toBe(true);
  });

  it('preflights and signs the whole local chunk before any network write', async () => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as unknown as Axios;

    await expect(
      exchange.placeOrders([regularOrder(0), regularOrder(1, { price: undefined }), regularOrder(2)]),
    ).rejects.toThrow('finite positive price');

    expect(signOrder).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('performs no network write when any independent signature fails', async () => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn();
    signOrder.mockResolvedValueOnce({ signature: '["r","s"]', signature_timestamp: 1 }).mockRejectedValueOnce(
      new Error('signer unavailable'),
    );
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrders([regularOrder(0), regularOrder(1), regularOrder(2)])).rejects.toThrow(
      'signer unavailable',
    );
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects duplicate generated client identities before signing or writing', async () => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn();
    uuidMock.mockReturnValue('duplicate');
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrders([regularOrder(0), regularOrder(1)])).rejects.toThrow(
      'invalid or duplicate client_id',
    );
    expect(signOrder).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('fails a partially accepted chunk with the exact missing identity set and unpaired official errors', async () => {
    const { exchange } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    let missingClientIds: string[] = [];
    exchange.xhr = {
      post: vi.fn(async (_endpoint: string, payloads: any[]) => {
        missingClientIds = [payloads[1].client_id, payloads[3].client_id];
        return {
          data: {
            orders: [
              { id: 'order-3', client_id: payloads[2].client_id },
              { id: 'order-1', client_id: payloads[0].client_id },
            ],
            errors: [
              { error: 'INSUFFICIENT_MARGIN', message: 'risk rejected' },
              { error: 'PRICE_OUT_OF_RANGE', message: 'price rejected' },
            ],
          },
        };
      }),
    } as unknown as Axios;

    const promise = exchange.placeOrders([regularOrder(0), regularOrder(1), regularOrder(2), regularOrder(3)]);
    await expect(promise).rejects.toThrow('INSUFFICIENT_MARGIN: risk rejected; PRICE_OUT_OF_RANGE: price rejected');

    const errorMessage = emitAndLogError.mock.calls.at(-1)?.[0] as string;
    expect(errorMessage).toContain(`[${missingClientIds.join(', ')}]`);
    expect(errorMessage).not.toContain(`${missingClientIds[0]}: INSUFFICIENT_MARGIN`);
    expect(errorMessage).not.toContain(`${missingClientIds[1]}: PRICE_OUT_OF_RANGE`);
  });

  it.each([
    ['a non-object envelope', () => []],
    ['a non-array orders field', () => ({ orders: {}, errors: [] })],
    ['a non-array errors field', () => ({ orders: [], errors: {} })],
    [
      'an unknown success identity',
      () => ({ orders: [{ id: 'order-1', client_id: 'unknown-client' }], errors: [{ error: 'REJECTED' }] }),
    ],
    [
      'a duplicate success identity',
      (ids: string[]) => ({
        orders: [
          { id: 'order-1', client_id: ids[0] },
          { id: 'order-2', client_id: ids[0] },
        ],
        errors: [],
      }),
    ],
    [
      'a successful row without an order id',
      (ids: string[]) => ({ orders: [{ client_id: ids[0] }], errors: [{ error: 'REJECTED' }] }),
    ],
    [
      'a missing result without an error',
      (ids: string[]) => ({ orders: [{ id: 'order-1', client_id: ids[0] }], errors: [] }),
    ],
    [
      'an error-count mismatch',
      (ids: string[]) => ({
        orders: [{ id: 'order-1', client_id: ids[0] }],
        errors: [{ error: 'ONE' }, { error: 'TWO' }],
      }),
    ],
    [
      'a non-string error field',
      (ids: string[]) => ({ orders: [{ id: 'order-1', client_id: ids[0] }], errors: [{ error: 123 }] }),
    ],
  ])('rejects %s without fabricating IDs', async (_label, makeResponse) => {
    const { exchange } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    exchange.xhr = {
      post: vi.fn(async (_endpoint: string, payloads: any[]) => ({
        data: makeResponse(payloads.map(({ client_id }) => client_id)),
      })),
    } as unknown as Axios;

    await expect(exchange.placeOrders([regularOrder(0), regularOrder(1)])).rejects.toThrow(
      'Paradex batch order response malformed',
    );
    expect(emitAndLogError).toHaveBeenCalledOnce();
  });
});
