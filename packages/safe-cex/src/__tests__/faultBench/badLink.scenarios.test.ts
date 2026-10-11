import type { Market, Order, PlaceOrderOpts } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { OKXExchange } from '../../exchanges/okx/okx.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType, PositionMode } from '../../types';
import { deferred } from './deferred';
import { createFakeVenue } from './fakeVenue';
import { createFaultAdapter } from './faultAdapter';

// Known gaps are `it.fails`: they pass while the bug exists and fail loudly once
// a later phase of docs/plans/BAD_LINK_RESILIENCE.md fixes it — flip them to `it`.

const MARKET: Market = {
  id: 'BTC-USDT-SWAP',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 10 }, leverage: { min: 1, max: 100 } },
  info: {},
};

const BATCH_ORDERS = 'post /api/v5/trade/batch-orders';

const limit = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: MARKET.symbol,
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 50_000,
  ...overrides,
});

const exchanges: OKXExchange[] = [];

const setup = () => {
  const store = new DefaultStore();
  store.update({ markets: [MARKET] });
  const options = { key: 'key', secret: 'secret', passphrase: 'passphrase', tealstreetTunnel: false };
  const exchange = new OKXExchange('okx', options as any, store);
  exchanges.push(exchange);
  vi.spyOn(exchange as any, 'log').mockImplementation(() => undefined);
  (exchange as any)._accountPositionMode = PositionMode.OneWay;
  vi.spyOn(exchange, 'getOrFetchPositionMode').mockResolvedValue(PositionMode.OneWay as any);

  const accepted: Array<Record<string, any>> = [];
  const venue = createFakeVenue({
    [BATCH_ORDERS]: ({ body }) => {
      const orders = body as Array<Record<string, any>>;
      const data = orders.map((order) => {
        accepted.push(order);
        return { ordId: `okx-${accepted.length - 1}`, clOrdId: order.clOrdId, sCode: '0' };
      });
      return { status: 200, data: { code: '0', data } };
    },
  });
  const bench = createFaultAdapter(venue);
  bench.install((exchange as any).xhr);
  return { exchange, store, venue, bench, accepted };
};

const writesReceived = (venue: ReturnType<typeof createFakeVenue>) =>
  venue.received.filter((request) => request.key === BATCH_ORDERS);

const openOrder = (overrides: Partial<Order> = {}): Order => ({
  id: 'okx-a',
  symbol: MARKET.symbol,
  side: OrderSide.Buy,
  type: OrderType.Limit,
  status: OrderStatus.Open,
  price: 50_000,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: 0,
  ...overrides,
});

afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe('bad link: placing an order (OKX, real factory over the fault bench)', () => {
  it('places a delivered order once', async () => {
    const { exchange, venue } = setup();

    await expect(exchange.placeOrder(limit())).resolves.toEqual(['okx-0']);
    expect(writesReceived(venue)).toHaveLength(1);
  });

  // Gap 1, fixed by PR #7179: the retry layer used to resend it, placing it twice.
  it('never resends an order whose response was lost', async () => {
    const { exchange, venue, bench, accepted } = setup();
    bench.on(BATCH_ORDERS, 'loseResponse');

    await expect(exchange.placeOrder(limit())).rejects.toThrow();
    expect(writesReceived(venue)).toHaveLength(1);
    expect(accepted).toHaveLength(1);
  });

  // Gap 2's safe-cex half holds: a caller-minted id reaches the venue, so a lost
  // response can be looked up by it. The web order paths minting none is the gap.
  it('carries a caller client order id to the venue through a lost response', async () => {
    const { exchange, bench, accepted } = setup();
    bench.on(BATCH_ORDERS, 'loseResponse');

    await expect(exchange.placeOrderWithClientOrderId({ ...limit(), clientOrderId: 'tealLookup1' })).rejects.toThrow();
    expect(accepted.map((order) => order.clOrdId)).toEqual(['tealLookup1']);
  });
});

describe('bad link: a REST snapshot racing the private stream (real BaseExchange + DefaultStore)', () => {
  const frame = (state: string) => ({
    ordId: 'okx-a',
    instId: MARKET.id,
    side: 'buy',
    ordType: 'limit',
    px: '50000',
    sz: '1',
    accFillSz: state === 'filled' ? '1' : '0',
    fillSz: state === 'filled' ? '1' : '0',
    fillPx: state === 'filled' ? '50000' : '',
    state,
    uTime: '1001000',
  });

  // The private stream's own order handler, fed the frame OKX sends.
  const raceSnapshotAgainst = async (state: 'canceled' | 'filled') => {
    const { exchange, store } = setup();
    const order = openOrder();
    store.update({ orders: [order] });

    // A second apart, so a fix comparing times is not decided by a tie.
    vi.useFakeTimers({ toFake: ['Date'], now: 1_000_000 });
    const snapshot = deferred<Order[]>();
    // Phase 4 is expected in BaseExchange/the store; a fix inside OKX's own fetchOrders would bypass this spy.
    vi.spyOn(exchange, 'fetchOrders').mockReturnValue(snapshot.promise);
    const applying = exchange.fetchAndSetPrivateSnapshot();

    vi.setSystemTime(1_001_000);
    (exchange.privateWebsocket as any).handleOrderTopic({ data: [frame(state)] });
    // Guards the setup: the stream really did close the order before the snapshot landed.
    expect(store.orders.find((candidate) => candidate.id === order.id)?.status).not.toBe(OrderStatus.Open);
    // The venue's view when it was read: live, stamped before the stream closed it.
    const liveFrame = { ...frame(state), state: 'live', accFillSz: '0', fillSz: '0', fillPx: '', uTime: '1000000' };
    snapshot.resolve(exchange.mapOrders([liveFrame]));
    await applying;

    return store.orders.find((candidate) => candidate.id === order.id)?.status;
  };

  // Gap 4 (docs/plans/BAD_LINK_RESILIENCE.md phase 4): the snapshot read the
  // venue before the cancel, lands after it, and resurrects the order.
  it.fails('a snapshot taken before a cancel does not reopen the cancelled order', async () => {
    expect(await raceSnapshotAgainst('canceled')).not.toBe(OrderStatus.Open);
  });

  it.fails('a snapshot taken before a fill does not reopen the filled order', async () => {
    expect(await raceSnapshotAgainst('filled')).not.toBe(OrderStatus.Open);
  });
});
