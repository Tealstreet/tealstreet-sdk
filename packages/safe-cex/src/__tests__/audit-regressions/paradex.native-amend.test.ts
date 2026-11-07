import type { Axios } from 'axios';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { ParadexRequestSigner } from '../reference/paradex/paradex.sdk';
import { ENDPOINTS } from '../reference/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';
import { Market, Order, OrderSide, OrderStatus, OrderType } from '../../types';

const MARKET: Market = {
  id: 'BTC-USD-PERP',
  symbol: 'BTC-USD-PERP',
  base: 'BTC',
  quote: 'USD',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 50 } },
};

const createLimitOrder = (overrides: Partial<Order> = {}): Order => ({
  id: 'paradex-order-1',
  status: OrderStatus.Open,
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 50_000,
  amount: 1.25,
  filled: 0.25,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: 1_784_505_600_000,
  info: {
    id: 'paradex-order-1',
    market: MARKET.symbol,
    side: 'BUY',
    type: 'LIMIT',
    status: 'OPEN',
    size: '1.25',
    remaining_size: '1',
    price: '50000',
    instruction: 'POST_ONLY',
    flags: [],
    stp: 'EXPIRE_MAKER',
  },
  ...overrides,
});

const createTriggerOrder = (
  type: 'STOP_MARKET' | 'STOP_LOSS_MARKET' | 'TAKE_PROFIT_MARKET' | 'STOP_LIMIT' | 'STOP_LOSS_LIMIT' | 'TAKE_PROFIT_LIMIT',
  overrides: Partial<Order> = {},
): Order => {
  const isTakeProfit = type.startsWith('TAKE_PROFIT');
  const isLimit = type.endsWith('_LIMIT');
  return {
    id: `paradex-${type.toLowerCase()}-1`,
    status: OrderStatus.Open,
    symbol: MARKET.symbol,
    type: isTakeProfit ? OrderType.TakeProfit : OrderType.StopLoss,
    side: OrderSide.Sell,
    price: 49_000,
    amount: 0.8,
    filled: 0,
    remaining: 0.8,
    reduceOnly: true,
    lastUpdate: 1_784_505_600_000,
    info: {
      id: `paradex-${type.toLowerCase()}-1`,
      market: MARKET.symbol,
      side: 'SELL',
      type,
      status: 'UNTRIGGERED',
      size: '0.8',
      remaining_size: '0.8',
      price: isLimit ? '48950' : '0',
      trigger_price: '49000',
      instruction: 'GTC',
      flags: ['REDUCE_ONLY'],
      stp: 'EXPIRE_TAKER',
    },
    ...overrides,
  };
};

const createExchange = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const exchange = new ParadexExchange(
    'paradex',
    { publicDataOnly: true, tuning: { consoleLogging: false } } as any,
    store,
  );
  const put = vi.fn(async (_endpoint: string, payload: any) => ({ data: { id: payload.id } }));
  const deleteRequest = vi.fn();
  const post = vi.fn();
  const signModifyOrder = vi.fn().mockResolvedValue({
    signature: '["r","s"]',
    signature_timestamp: 1_784_505_600_123,
  });
  exchange.xhr = { put, delete: deleteRequest, post } as unknown as Axios;
  exchange.requestSigner = { signModifyOrder } as any;
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

  return { exchange, put, deleteRequest, post, signModifyOrder };
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Paradex native order amendment', () => {
  it('amends an exact post-only LIMIT in place and omits server-preserved immutable attributes', async () => {
    const { exchange, put, deleteRequest, post, signModifyOrder } = createExchange();
    const order = createLimitOrder();

    await expect(exchange.updateOrder({ order, update: { price: 50_100.04 } })).resolves.toEqual([order.id]);

    expect(signModifyOrder).toHaveBeenCalledWith({
      id: order.id,
      market: MARKET.symbol,
      side: 'BUY',
      type: 'LIMIT',
      size: '1.25',
      price: '50100',
    });
    expect(put).toHaveBeenCalledWith(ENDPOINTS.MODIFY_ORDER.replace('{id}', order.id), {
      id: order.id,
      market: MARKET.symbol,
      side: 'BUY',
      type: 'LIMIT',
      size: '1.25',
      price: '50100',
      signature: '["r","s"]',
      signature_timestamp: 1_784_505_600_123,
    });
    expect(put.mock.calls[0][1]).not.toHaveProperty('instruction');
    expect(put.mock.calls[0][1]).not.toHaveProperty('flags');
    expect(put.mock.calls[0][1]).not.toHaveProperty('stp');
    expect(deleteRequest).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('sends a requested total base size after a partial fill and preserves the stable order id', async () => {
    const { exchange, put } = createExchange();
    const order = createLimitOrder({
      reduceOnly: true,
      info: { ...createLimitOrder().info, flags: ['REDUCE_ONLY'] },
    });

    await expect(exchange.updateOrder({ order, update: { amount: 0.7504 } })).resolves.toEqual([order.id]);

    expect(put).toHaveBeenCalledWith(
      `/orders/${order.id}`,
      expect.objectContaining({ id: order.id, size: '0.75', price: '50000', type: 'LIMIT' }),
    );
  });

  it.each([
    ['STOP_MARKET', '0'],
    ['STOP_LOSS_MARKET', '0'],
    ['TAKE_PROFIT_MARKET', '0'],
    ['STOP_LIMIT', '48950'],
    ['STOP_LOSS_LIMIT', '48950'],
    ['TAKE_PROFIT_LIMIT', '48950'],
  ] as const)('preserves exact %s semantics and changes only the untriggered TPSL trigger', async (type, price) => {
    const { exchange, put, signModifyOrder } = createExchange();
    const order = createTriggerOrder(type);

    await expect(exchange.updateOrder({ order, update: { price: 49_100.04 } })).resolves.toEqual([order.id]);

    expect(signModifyOrder).toHaveBeenCalledWith(
      expect.objectContaining({ id: order.id, type, price, size: '0.8' }),
    );
    expect(put).toHaveBeenCalledWith(
      `/orders/${order.id}`,
      expect.objectContaining({ id: order.id, type, price, trigger_price: '49100' }),
    );
  });

  it.each([
    {
      name: 'already-triggered TPSL',
      order: createTriggerOrder('STOP_LOSS_LIMIT', {
        info: { ...createTriggerOrder('STOP_LOSS_LIMIT').info, status: 'OPEN' },
      }),
      update: { price: 49_100 },
      error: 'is unsupported',
    },
    {
      name: 'unreconciled exact upstream type',
      order: createLimitOrder({ info: { ...createLimitOrder().info, type: 'STOP_LOSS_MARKET' } }),
      update: { price: 50_100 },
      error: 'is unsupported',
    },
    {
      name: 'unreconciled reduce-only state',
      order: createLimitOrder({ reduceOnly: true }),
      update: { price: 50_100 },
      error: 'reduce-only state is not reconciled',
    },
    {
      name: 'isolated-margin route',
      order: createLimitOrder({ info: { ...createLimitOrder().info, on_behalf_of_account: 'isolated-1' } }),
      update: { price: 50_100 },
      error: 'isolated-margin order routing is unsupported',
    },
    {
      name: 'quantity at or below already-filled base size',
      order: createLimitOrder(),
      update: { amount: 0.1 },
      error: 'must exceed the already-filled base size',
    },
  ])('rejects $name before signing or writing', async ({ order, update, error }) => {
    const { exchange, put, deleteRequest, post, signModifyOrder } = createExchange();

    await expect(exchange.updateOrder({ order, update } as any)).rejects.toThrow(error);

    expect(signModifyOrder).not.toHaveBeenCalled();
    expect(put).not.toHaveBeenCalled();
    expect(deleteRequest).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it.each([
    {
      name: 'HTTP rejection',
      response: () =>
        Promise.reject(Object.assign(new Error('Modify rejected'), { response: { data: { message: 'Bad size' } } })),
      error: 'Modify rejected',
    },
    {
      name: 'mismatched response identity',
      response: () => Promise.resolve({ data: { id: 'different-order-id' } }),
      error: 'did not confirm stable order id',
    },
  ])('surfaces $name without canceling or placing a replacement', async ({ response, error }) => {
    const { exchange, put, deleteRequest, post } = createExchange();
    put.mockImplementation(response as any);

    await expect(exchange.updateOrder({ order: createLimitOrder(), update: { price: 50_100 } })).rejects.toThrow(
      error,
    );

    expect(deleteRequest).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('emits a signing failure and performs no exchange write or replacement fallback', async () => {
    const { exchange, put, deleteRequest, post, signModifyOrder } = createExchange();
    const signingError = new Error('ModifyOrder signer unavailable');
    signModifyOrder.mockRejectedValue(signingError);
    const emitAndLogError = vi.spyOn(exchange, 'emitAndLogError');

    await expect(exchange.updateOrder({ order: createLimitOrder(), update: { price: 50_100 } })).rejects.toBe(
      signingError,
    );

    expect(emitAndLogError).toHaveBeenCalledWith('ModifyOrder signer unavailable');
    expect(put).not.toHaveBeenCalled();
    expect(deleteRequest).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });
});

describe('Paradex ModifyOrder signing contract', () => {
  it('signs the official ModifyOrder typed data with stable id and chain-quantized values', async () => {
    vi.spyOn(Date, 'now').mockReturnValue(1_784_505_600_123);
    const signMessage = vi.fn().mockResolvedValue([123n, 456n]);
    const signer = new ParadexRequestSigner({
      account: { signMessage },
      config: { paradexChainId: 'PRIVATE_SN_PARACLEAR_MAINNET' },
      provider: {},
    });

    await expect(
      signer.signModifyOrder({
        id: 'paradex-order-1',
        market: MARKET.symbol,
        side: 'SELL',
        type: 'STOP_LOSS_LIMIT',
        size: '0.7504',
        price: '48950.1',
      }),
    ).resolves.toEqual({ signature: '["123","456"]', signature_timestamp: 1_784_505_600_123 });

    expect(signMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        primaryType: 'ModifyOrder',
        message: {
          timestamp: '1784505600123',
          market: MARKET.symbol,
          side: '2',
          orderType: 'STOP_LOSS_LIMIT',
          size: '75040000',
          price: '4895010000000',
          id: 'paradex-order-1',
        },
      }),
    );
  });
});
