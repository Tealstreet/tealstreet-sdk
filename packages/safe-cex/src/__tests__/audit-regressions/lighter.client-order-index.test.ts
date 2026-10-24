import type { PlaceOrderOpts } from '../../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { LighterExchange } from '../../exchanges/lighter/lighter.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';

vi.mock('@tealstreet/poseidon-crypto', () => ({
  initWasm: vi.fn(),
  isWasmReady: vi.fn(() => true),
  getCryptoApi: vi.fn(() => ({})),
}));
vi.mock('@tealstreet/lighter-sdk', () => ({
  LighterApiClient: class {},
  OrderApi: class {},
  CandlestickApi: class {},
}));
vi.mock('../../exchanges/lighter/lighter.api', () => ({
  createLighterSDKClient: vi.fn(() => ({ get: vi.fn() })),
}));
vi.mock('../../exchanges/lighter/lighter.ws-public', () => ({
  LighterPublicWebsocket: class {
    dispose = vi.fn();
  },
}));
vi.mock('../../exchanges/lighter/lighter.ws-private', () => ({
  LighterPrivateWebsocket: class {
    throttleFetchOrders = vi.fn();
    dispose = vi.fn();
  },
}));

const MAX_INDEX = (1n << 48n) - 1n;
const order: PlaceOrderOpts = {
  symbol: 'BTC/USDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 100,
  amount: 1,
};
const exchanges: LighterExchange[] = [];
const setup = (maxSize = 100) => {
  const exchange = new LighterExchange(
    'lighter',
    { key: '', secret: '', apiKeyIndex: '1', applicationId: '123', tuning: { consoleLogging: false } },
    new DefaultStore(),
  );
  exchanges.push(exchange);
  exchange.store.update({
    markets: [
      {
        id: '1',
        symbol: order.symbol,
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: maxSize }, leverage: { min: 1, max: 50 } },
        info: { amountDecimals: 3, priceDecimals: 1, min_quote_amount: '1' },
      },
    ],
  });
  const createOrder = vi.fn().mockResolvedValue('accepted');
  const createOrders = vi.fn().mockResolvedValue(['accepted']);
  vi.spyOn(exchange as any, 'ensureSignerAdapter').mockResolvedValue({ createOrder, createOrders });
  vi.spyOn(exchange as any, 'ensurePartnerAttributionTierEligible').mockResolvedValue(undefined);
  vi.spyOn(exchange as any, 'ensurePartnerAttributionApproved').mockResolvedValue(undefined);
  return { exchange, createOrder, createOrders };
};
const assertUnique = (ids: bigint[]) => {
  expect(new Set(ids).size).toBe(ids.length);
  for (const id of ids) expect(id > 0n && id <= MAX_INDEX).toBe(true);
};

beforeEach(() => {
  vi.spyOn(Date, 'now').mockReturnValue(1_700_000_000_000);
  vi.spyOn(Math, 'random').mockReturnValue(0);
});
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  vi.restoreAllMocks();
});

describe('Lighter generated clientOrderIndex reservation', () => {
  it('gives the entry, stop-loss and take-profit distinct IDs in the same millisecond', async () => {
    const { exchange, createOrder } = setup();
    const ids = await exchange.placeOrder({ ...order, stopLoss: 80, takeProfit: 120 });
    expect(createOrder).toHaveBeenCalledTimes(3);
    expect(createOrder.mock.calls.map(([request]) => request.orderType).sort()).toEqual([
      'limit',
      'stop_loss',
      'take_profit',
    ]);
    assertUnique(createOrder.mock.calls.map(([request]) => request.orderId));
    expect(ids).toEqual(createOrder.mock.calls.map(([request]) => request.orderId.toString()));
  });

  it('reserves split legs independently when another order runs during a send', async () => {
    const { exchange, createOrder } = setup(2);
    let release!: () => void;
    createOrder.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        }),
    );
    const split = exchange.placeOrder({ ...order, amount: 4 });
    await vi.waitFor(() => expect(createOrder).toHaveBeenCalledTimes(1));
    await exchange.placeOrder(order);
    release();
    await split;
    expect(createOrder).toHaveBeenCalledTimes(3);
    assertUnique(createOrder.mock.calls.map(([request]) => request.orderId));
  });

  it('shares one namespace across simultaneous batches and singles', async () => {
    const { exchange, createOrder, createOrders } = setup();
    await Promise.all([
      exchange.placeOrders([order, { ...order, price: 101 }]),
      exchange.placeOrders([order, { ...order, price: 102 }]),
      exchange.placeOrder(order),
    ]);
    const ids = [
      ...createOrders.mock.calls.flatMap(([requests]) =>
        requests.map((request: { orderId: bigint }) => request.orderId),
      ),
      ...createOrder.mock.calls.map(([request]) => request.orderId),
    ];
    expect(ids).toHaveLength(5);
    assertUnique(ids);
  });

  it('never reuses a failed send reservation when the clock moves backward', async () => {
    const { exchange, createOrder } = setup();
    createOrder.mockRejectedValueOnce(new Error('response unavailable'));
    await expect(exchange.placeOrder(order)).rejects.toThrow();
    vi.mocked(Date.now).mockReturnValue(1_699_999_999_000);
    await exchange.placeOrder(order);
    const ids = createOrder.mock.calls.map(([request]) => request.orderId);
    assertUnique(ids);
    expect(ids[1]).toBeGreaterThan(ids[0]);
  });

  it('fails closed at the positive 48-bit boundary instead of wrapping or sending zero', async () => {
    const { exchange, createOrder } = setup();
    (exchange as any).lastClientOrderIndex = MAX_INDEX - 1n;
    await exchange.placeOrder(order);
    expect(createOrder.mock.calls[0][0].orderId).toBe(MAX_INDEX);
    const onError = vi.fn();
    exchange.emitter.on('error', onError);
    await expect(exchange.placeOrder(order)).rejects.toThrow(/client order index.*exhausted/i);
    expect(createOrder).toHaveBeenCalledTimes(1);
    expect(onError).toHaveBeenCalledOnce();
  });
});
