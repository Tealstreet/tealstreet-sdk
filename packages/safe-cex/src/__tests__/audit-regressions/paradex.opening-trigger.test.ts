import type { Axios } from 'axios';
import type { PlaceOrderOpts } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { getExchangeMetadata } from '../../exchangeMetadata';
import { ParadexExchange } from '../reference/paradex/paradex.exchange';
import { ENDPOINTS } from '../reference/paradex/paradex.types';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

const { uuidMock } = vi.hoisted(() => ({ uuidMock: vi.fn() }));
vi.mock('../../utils/uuid', () => ({ uuid: uuidMock }));

const openingTrigger = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTC-USD-PERP',
  type: OrderType.Conditional,
  side: OrderSide.Buy,
  amount: 0.25,
  price: 50_100,
  triggerPrice: 50_000,
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
  const post = vi.fn(async (endpoint: string, payload: any) => ({
    data: { id: 'opening-trigger-1', client_id: payload.client_id },
  }));
  exchange.requestSigner = { signOrder } as any;
  exchange.xhr = { post } as unknown as Axios;
  vi.spyOn(exchange, 'log').mockImplementation(() => undefined);
  return { exchange, signOrder, post };
};

beforeEach(() => {
  uuidMock.mockReset();
  uuidMock.mockReturnValue('openingtrigger1');
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('Paradex opening trigger orders', () => {
  it('signs and sends an exact non-reduce-only STOP_LIMIT with explicit default STP', async () => {
    const { exchange, signOrder, post } = createExchange();

    await expect(exchange.placeOrder(openingTrigger())).resolves.toEqual(['opening-trigger-1']);

    expect(signOrder).toHaveBeenCalledWith({
      market: 'BTC-USD-PERP',
      side: 'BUY',
      type: 'STOP_LIMIT',
      size: '0.25',
      price: '50100',
    });
    expect(post).toHaveBeenCalledWith(
      ENDPOINTS.PLACE_ORDER,
      expect.objectContaining({
        market: 'BTC-USD-PERP',
        side: 'BUY',
        type: 'STOP_LIMIT',
        size: '0.25',
        price: '50100',
        trigger_price: '50000',
        instruction: 'GTC',
        stp: 'EXPIRE_TAKER',
        client_id: 'ts_openingtrigger1',
      }),
    );
    expect(post.mock.calls[0][1]).not.toHaveProperty('flags');
    expect(getExchangeMetadata('paradex').supportsOpeningTriggerOrders).toBe(true);
  });

  it.each([
    ['missing execution price', { price: undefined }, 'execution price'],
    ['invalid execution price', { price: Number.NaN }, 'execution price'],
    ['missing trigger price', { triggerPrice: undefined }, 'trigger price'],
    ['invalid trigger price', { triggerPrice: 0 }, 'trigger price'],
    ['reduce-only routing', { reduceOnly: true }, 'cannot be reduce-only'],
    ['attached take profit', { takeProfit: 52_000 }, 'cannot safely attach TP/SL'],
    ['attached stop loss', { stopLoss: 48_000 }, 'cannot safely attach TP/SL'],
  ])('rejects %s before signing or writing', async (_label, overrides, message) => {
    const { exchange, signOrder, post } = createExchange();

    await expect(exchange.placeOrder(openingTrigger(overrides as Partial<PlaceOrderOpts>))).rejects.toThrow(message);
    expect(signOrder).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('rejects the unsupported trailing-stop shape instead of silently degrading it to STOP_MARKET', async () => {
    const { exchange, signOrder, post } = createExchange();

    await expect(
      exchange.placeOrder(openingTrigger({ type: OrderType.TrailingStopLoss, triggerPrice: undefined })),
    ).rejects.toThrow('trailing-stop orders are not supported');
    expect(signOrder).not.toHaveBeenCalled();
    expect(post).not.toHaveBeenCalled();
  });

  it('classifies non-reduce STOP_LIMIT readback as Conditional and protective STOP_LIMIT as StopLoss', () => {
    const { exchange } = createExchange();
    const raw = {
      id: 'stop-limit-1',
      market: 'BTC-USD-PERP',
      side: 'BUY',
      size: '0.25',
      remaining_size: '0.25',
      price: '50100',
      trigger_price: '50000',
      status: 'UNTRIGGERED',
      type: 'STOP_LIMIT',
      created_at: '1784505600000',
      updated_at: '1784505600000',
      expires_at: '0',
    } as any;

    expect((exchange as any).mapOrder({ ...raw, flags: [] })).toMatchObject({
      type: OrderType.Conditional,
      reduceOnly: false,
      price: 50_000,
    });
    expect((exchange as any).mapOrder({ ...raw, flags: ['REDUCE_ONLY'] })).toMatchObject({
      type: OrderType.StopLoss,
      reduceOnly: true,
      price: 50_000,
    });
  });
});
