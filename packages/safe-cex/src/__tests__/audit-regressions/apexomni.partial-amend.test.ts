import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ApexOmniExchange } from '../reference/apexomni/apexomni.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderStopTriggerType } from '../../types';
import { virtualClock } from '../../utils/virtualClock';

vi.mock('zklink-web-sdk-alpha', () => ({}));
vi.mock('../reference/apexomni/zklink/signer/utils', () => ({ initSdk: vi.fn() }));

const NOW = 1_700_000_000_123;
const row = (filled = 6, status = 'OPEN', updatedAt = NOW) => ({
  id: 'parent',
  symbol: 'BTC-USDT',
  side: 'BUY',
  type: 'LIMIT',
  status,
  size: '10',
  price: '60000',
  cumSuccessFillSize: `${filled}`,
  openSlParam: { triggerPrice: '50000', triggerPriceType: 'INDEX' },
  openTpParams: { triggerPrice: '65000', triggerPriceType: 'MARKET' },
  updatedAt,
});

const fixtureForTest = () => {
  const store = new DefaultStore();
  store.update({
    markets: [
      {
        id: 'BTC-USDT',
        symbol: 'BTCUSDT',
        base: 'BTC',
        quote: 'USDT',
        active: true,
        precision: { amount: 0.001, price: 0.1 },
        limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
      },
    ] as any,
  });
  const exchange = new ApexOmniExchange(
    'apexomni' as any,
    { key: 'key', secret: 'secret', passphrase: 'passphrase' } as any,
    store,
  );
  const websocket = exchange.privateWebsocket;
  vi.spyOn(websocket, 'throttleFetchOrders').mockImplementation(() => undefined);
  websocket.handleOrderTopic([row()] as any, true);
  const place = vi.spyOn(exchange, 'placeOrder').mockResolvedValue(['replacement']);
  let finish!: () => void;
  let fail!: () => void;
  const post = vi.fn(
    () =>
      new Promise((resolve, reject) => {
        finish = () => resolve({ data: { data: null } });
        fail = () => reject(new Error('cancel failed'));
      }),
  );
  exchange.unlimitedXHR = { post } as any;
  const start = async (id = 'parent', amount?: number) => {
    const order = store.orders.find((stored) => stored.id === id)!;
    const pending = exchange.updateOrder({
      order,
      update: { price: 59000, ...(amount === undefined ? {} : { amount }) },
    });
    await vi.waitFor(() => expect(post).toHaveBeenCalledTimes(1));
    expect(store.orders).toHaveLength(0);
    return { pending };
  };
  const fill = (createdAt = NOW + 1, size = 1) =>
    websocket.handleFillTopic([
      {
        id: 'execution',
        orderId: 'parent',
        symbol: 'BTC-USDT',
        side: 'BUY',
        size: `${size}`,
        price: '60000',
        fee: '0',
        createdAt,
      },
    ] as any);
  return { exchange, store, websocket, place, post, start, fill, finish: () => finish(), fail: () => fail() };
};

describe('ApeX Omni partial-fill cancellation and replacement', () => {
  beforeEach(() => vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(NOW));
  afterEach(() => vi.restoreAllMocks());

  it.each([{}, { price: NaN }, { price: Infinity }, { price: 0 }, { amount: 0 }])(
    'rejects unchanged or invalid amendments before cancellation: %s',
    async (update) => {
      const fixture = fixtureForTest();
      const order = fixture.store.orders.find((stored) => stored.id === 'parent')!;
      await expect(fixture.exchange.updateOrder({ order, update } as any)).rejects.toThrow();
      expect(fixture.post).not.toHaveBeenCalled();
      expect(fixture.store.orders).toHaveLength(3);
    },
  );

  it.each(['parent', 'parent__take_profit'])(
    'replaces only the unfilled four units and preserves siblings for %s',
    async (id) => {
      const fixture = fixtureForTest();
      const { pending } = await fixture.start(id);
      fixture.finish();
      await expect(pending).resolves.toEqual(['replacement']);
      expect(fixture.place).toHaveBeenCalledWith(
        expect.objectContaining({
          amount: 4,
          price: id === 'parent' ? 59000 : 60000,
          stopLoss: 50000,
          takeProfit: id === 'parent' ? 65000 : 59000,
          slTriggerType: OrderStopTriggerType.IndexPrice,
          tpTriggerType: OrderStopTriggerType.LastPrice,
        }),
      );
      expect(fixture.place.mock.calls[0][0].clientOrderId).toBeUndefined();
    },
  );

  it.each([true, false])('counts cumulative and execution updates once when fill-first is %s', async (fillFirst) => {
    const fixture = fixtureForTest();
    const { pending } = await fixture.start();
    if (fillFirst) fixture.fill();
    fixture.websocket.handleOrderTopic([row(7, 'OPEN', NOW + 1)] as any);
    if (!fillFirst) fixture.fill();
    fixture.finish();
    await pending;
    expect(fixture.place).toHaveBeenCalledWith(expect.objectContaining({ amount: 3 }));
  });

  it('uses an observed cancellation total even though cancellation already removed the order', async () => {
    const fixture = fixtureForTest();
    const { pending } = await fixture.start();
    fixture.websocket.handleOrderTopic([row(8, 'CANCELED')] as any);
    fixture.finish();
    await pending;
    expect(fixture.place).toHaveBeenCalledWith(expect.objectContaining({ amount: 2 }));
  });

  it('adds a distinct execution that occurs after a cumulative report', async () => {
    const fixture = fixtureForTest();
    const { pending } = await fixture.start();
    fixture.websocket.handleOrderTopic([row(7, 'OPEN', NOW + 1)] as any);
    fixture.fill(NOW + 2);
    fixture.finish();
    await pending;
    expect(fixture.place).toHaveBeenCalledWith(expect.objectContaining({ amount: 2 }));
  });

  it('does not subtract a delayed execution already included in the baseline fill', async () => {
    const fixture = fixtureForTest();
    const { pending } = await fixture.start();
    fixture.fill(NOW - 1, 6);
    fixture.finish();
    await pending;
    expect(fixture.place).toHaveBeenCalledWith(expect.objectContaining({ amount: 4 }));
  });

  it('counts a new genuine fill even when older synthetic coverage is still outstanding', async () => {
    const fixture = fixtureForTest();
    fixture.websocket.handleOrderTopic([row(7)] as any);
    const { pending } = await fixture.start();
    fixture.fill(NOW + 1);
    fixture.finish();
    await pending;
    expect(fixture.place).toHaveBeenCalledWith(expect.objectContaining({ amount: 2 }));
  });

  it.each(['order', 'fill'])('does not replace after a full fill observed through the %s topic', async (topic) => {
    const fixture = fixtureForTest();
    const { pending } = await fixture.start();
    if (topic === 'order') fixture.websocket.handleOrderTopic([row(10, 'FILLED')] as any);
    else fixture.fill(NOW + 1, 4);
    fixture.finish();
    await expect(pending).resolves.toEqual([]);
    expect(fixture.place).not.toHaveBeenCalled();
  });

  it('preserves an explicit replacement amount after additional fills', async () => {
    const fixture = fixtureForTest();
    const { pending } = await fixture.start('parent', 2.5);
    fixture.websocket.handleOrderTopic([row(7)] as any);
    fixture.finish();
    await pending;
    expect(fixture.place).toHaveBeenCalledWith(expect.objectContaining({ amount: 2.5 }));
  });

  it('does not replace after cancellation fails', async () => {
    const fixture = fixtureForTest();
    const { pending } = await fixture.start();
    const rejected = expect(pending).rejects.toThrow('cancel failed');
    fixture.fail();
    await rejected;
    expect(fixture.place).not.toHaveBeenCalled();
  });
});
