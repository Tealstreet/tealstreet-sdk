import type { Axios } from 'axios';
import type { PlaceOrderOpts } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../../exchanges/paradex/paradex.exchange';
import { ENDPOINTS } from '../../exchanges/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const { uuidMock } = vi.hoisted(() => ({ uuidMock: vi.fn() }));
vi.mock('../../utils/uuid', () => ({ uuid: uuidMock }));

const marketOrder = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTC-USD-PERP',
  type: OrderType.Market,
  side: OrderSide.Buy,
  amount: 0.01,
  ...overrides,
});

const createExchange = () => {
  const exchange = new ParadexExchange(
    'paradex',
    { publicDataOnly: true, tuning: { consoleLogging: false } } as any,
    new DefaultStore(),
  );
  const signOrder = vi.fn(async () => ({
    signature: '["r","s"]',
    signature_timestamp: 1_784_505_600_000,
  }));
  exchange.requestSigner = { signOrder } as any;
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return { exchange, signOrder };
};

beforeEach(() => {
  uuidMock.mockReset();
  uuidMock.mockReturnValue('singleclient1');
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Paradex single-order client identity', () => {
  it('generates and signs a validated client_id and requires its exact response echo', async () => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn(async (endpoint: string, payload: any) => {
      expect(endpoint).toBe(ENDPOINTS.PLACE_ORDER);
      expect(payload.client_id).toBe('ts_singleclient1');
      return { data: { id: 'paradex-order-1', client_id: payload.client_id } };
    });
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder())).resolves.toEqual(['paradex-order-1']);

    expect(uuidMock).toHaveBeenCalledOnce();
    expect(signOrder).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledOnce();
  });

  it('uses a caller-provided client_id for regular single-native orders', async () => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn(async (endpoint: string, payload: any) => {
      expect(endpoint).toBe(ENDPOINTS.PLACE_ORDER);
      expect(payload.client_id).toBe('caller-client-id');
      return { data: { id: 'paradex-order-1', client_id: payload.client_id } };
    });
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder({ clientOrderId: 'caller-client-id' }))).resolves.toEqual([
      'paradex-order-1',
    ]);

    expect(uuidMock).not.toHaveBeenCalled();
    expect(signOrder).toHaveBeenCalledOnce();
    expect(post).toHaveBeenCalledOnce();
  });

  it.each(['', 'x'.repeat(62)])('rejects invalid generated identity %j before signing or writing', async (value) => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn();
    uuidMock.mockReturnValue(value);
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder())).rejects.toThrow('Paradex generated an invalid client_id');

    expect(signOrder).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects invalid caller-provided client IDs before signing or writing', async () => {
    const { exchange, signOrder } = createExchange();
    const post = vi.fn();
    exchange.xhr = { post } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder({ clientOrderId: '' }))).rejects.toThrow(
      'Paradex clientOrderId must be a non-empty string of at most 64 characters',
    );

    expect(signOrder).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it.each([
    ['a missing envelope', () => undefined],
    ['a non-object envelope', () => []],
    ['an empty canonical id', (clientId: string) => ({ id: '   ', client_id: clientId })],
    ['a missing client echo', () => ({ id: 'paradex-order-1' })],
    ['a mismatched client echo', () => ({ id: 'paradex-order-1', client_id: 'ts_different' })],
  ])('fails closed on %s', async (_label, makeResponse) => {
    const { exchange } = createExchange();
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => undefined as any);
    exchange.xhr = {
      post: vi.fn(async (_endpoint: string, payload: any) => ({ data: makeResponse(payload.client_id) })),
    } as unknown as Axios;

    await expect(exchange.placeOrder(marketOrder())).rejects.toThrow('Paradex order response malformed');
    expect(emitAndLogError).toHaveBeenCalledOnce();
  });

  it('keeps attached child placement on its existing lifecycle after the correlated main order', async () => {
    const { exchange, signOrder } = createExchange();
    const payloads: any[] = [];
    exchange.xhr = {
      post: vi.fn(async (_endpoint: string, payload: any) => {
        payloads.push(payload);
        if (payloads.length === 1) {
          return { data: { id: 'main-order', client_id: payload.client_id } };
        }
        return { data: { id: 'take-profit-child' } };
      }),
    } as unknown as Axios;

    await expect(
      exchange.placeOrder(
        marketOrder({ type: OrderType.Limit, price: 50_000, takeProfit: 52_000, clientOrderId: 'ignored-client-id' }),
      ),
    ).resolves.toEqual(['main-order', 'take-profit-child']);

    expect(signOrder).toHaveBeenCalledTimes(2);
    expect(payloads).toHaveLength(2);
    expect(payloads[0].client_id).toBe('ts_singleclient1');
    expect(payloads[1]).not.toHaveProperty('client_id');
  });
});
