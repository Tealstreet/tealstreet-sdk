import { beforeEach, describe, expect, it, vi, type MockInstance } from 'vitest';

import { exchangeMetadata } from '../exchangeMetadata';
import { BitmexExchange } from '../exchanges/bitmex/bitmex.exchange';
import type { BitmexStrategy } from '../exchanges/bitmex/bitmex.types';
import type { BITMEX_POSITION } from '../exchanges/bitmex/types';
import { DefaultStore } from '../store/store.base';
import { OrderSide, OrderTimeInForce, OrderType, PositionMode, PositionSide } from '../types';

const makeRawPosition = (strategy: BitmexStrategy, currentQty: number): BITMEX_POSITION => ({
  account: 1,
  symbol: 'XBTUSDT',
  currency: 'USDt',
  rebalancedPnl: 0,
  openOrderBuyQty: 0,
  openOrderBuyCost: 0,
  currentQty,
  currentCost: 0,
  unrealisedCost: 0,
  markPrice: 50_000,
  markValue: 0,
  riskValue: 0,
  homeNotional: 0,
  foreignNotional: 0,
  posCost: 0,
  posComm: 0,
  posMargin: 0,
  posMaint: 0,
  initMargin: 0,
  maintMargin: 0,
  unrealisedPnl: 0,
  unrealisedPnlPcnt: 0,
  unrealisedRoePcnt: 0,
  avgCostPrice: 50_000,
  avgEntryPrice: 50_000,
  liquidationPrice: 0,
  bankruptPrice: 0,
  leverage: 10,
  crossMargin: true,
  timestamp: '2026-07-11T00:00:00.000Z',
  strategy,
});

const createExchange = () => {
  const store = new DefaultStore();
  // This focused fixture intentionally supplies only fields consumed by BitMEX order/position mapping.
  store.update({
    markets: [
      {
        symbol: 'XBTUSDT',
        precision: { price: 0.5, amount: 0.001 },
        limits: { leverage: { min: 1, max: 100 } },
        info: { isQuanto: false, underlyingToPositionMultiplier: 1 },
        isInverse: false,
      } as unknown as (typeof store.markets)[number],
    ],
    tickers: [{ symbol: 'XBTUSDT', last: 50_000, mark: 50_000 } as unknown as (typeof store.tickers)[number]],
  });

  const exchange = new BitmexExchange(
    'bitmex',
    { key: 'key', secret: 'secret', testnet: false },
    store,
  );
  exchange.convertToMicroValue = vi.fn((value: number) => value);
  vi.spyOn(exchange, 'fetchBalance').mockResolvedValue({ used: 0, free: 0, total: 0, upnl: 0 });
  vi.spyOn(exchange, 'fetchPositions').mockResolvedValue([]);
  const orderPost = vi.spyOn(exchange.unlimitedXHR, 'post').mockResolvedValue({ data: { orderID: 'order-id' } });

  return { exchange, store, orderPost };
};

const limitOrder = {
  symbol: 'XBTUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  amount: 1,
  price: 50_000,
};

describe('BitmexExchange hedge mode', () => {
  let exchange: BitmexExchange;
  let store: DefaultStore;
  let orderPost: MockInstance;

  beforeEach(() => {
    ({ exchange, store, orderPost } = createExchange());
  });

  it('omits strategy on the wire for OneWay mode', async () => {
    exchange.observePositionMode([{ strategy: 'OneWay' }]);

    await exchange.placeOrder(limitOrder);

    expect(orderPost).toHaveBeenCalledWith('/order', {
      symbol: 'XBTUSDT',
      side: 'Buy',
      ordType: 'Limit',
      orderQty: '1',
      price: '50000',
      timeInForce: OrderTimeInForce.GoodTillCancel,
      text: 'Tealstreet',
    });
  });

  it.each([
    { side: OrderSide.Buy, reduceOnly: false, strategy: 'Long' },
    { side: OrderSide.Sell, reduceOnly: false, strategy: 'Short' },
    { side: OrderSide.Buy, reduceOnly: true, strategy: 'Short' },
    { side: OrderSide.Sell, reduceOnly: true, strategy: 'Long' },
  ] as const)('targets the $strategy hedge leg for $side reduceOnly=$reduceOnly', async ({ side, reduceOnly, strategy }) => {
    exchange.observePositionMode([{ strategy: 'Long' }]);

    await exchange.placeOrder({ ...limitOrder, side, reduceOnly });

    expect(orderPost).toHaveBeenCalledWith(
      '/order',
      expect.objectContaining({
        side: side === OrderSide.Buy ? 'Buy' : 'Sell',
        strategy,
        ...(reduceOnly ? { execInst: 'ReduceOnly' } : {}),
      }),
    );
  });

  it('propagates the parent hedge leg to both bracket children', async () => {
    exchange.observePositionMode([{ strategy: 'Short' }]);

    await exchange.placeOrder({ ...limitOrder, stopLoss: 49_000, takeProfit: 51_000 });

    expect(orderPost.mock.calls.map(([endpoint, body]) => [endpoint, body])).toEqual([
      [
        '/order',
        expect.objectContaining({ side: 'Buy', strategy: 'Long', ordType: 'Limit' }),
      ],
      [
        '/order',
        expect.objectContaining({ side: 'Sell', strategy: 'Long', ordType: 'StopMarket', execInst: 'ReduceOnly' }),
      ],
      [
        '/order',
        expect.objectContaining({
          side: 'Sell',
          strategy: 'Long',
          ordType: 'MarketIfTouched',
          execInst: 'ReduceOnly',
        }),
      ],
    ]);
  });

  it('detects MultiWay responses and maps strategy-aware sides and IDs', () => {
    const long = makeRawPosition('Long', 2);
    const short = makeRawPosition('Short', 2);

    exchange.observePositionMode([long, short]);
    const mappedLong = exchange.mapPosition(long);
    const mappedShort = exchange.mapPosition(short);

    expect(store.options.isHedged).toBe(true);
    expect(mappedLong).toMatchObject({
      id: 'XBTUSDT:long',
      side: PositionSide.Long,
      positionMode: PositionMode.Hedged,
    });
    expect(mappedShort).toMatchObject({
      id: 'XBTUSDT:short',
      side: PositionSide.Short,
      positionMode: PositionMode.Hedged,
    });
  });

  it('preserves signed-quantity OneWay mapping and symbol-only IDs', () => {
    const position = makeRawPosition('OneWay', -2);

    exchange.observePositionMode([position]);
    const mapped = exchange.mapPosition(position);

    expect(store.options.isHedged).toBe(false);
    expect(mapped).toMatchObject({
      id: 'XBTUSDT',
      side: PositionSide.Short,
      positionMode: PositionMode.OneWay,
    });
  });

  it('resolves mode from /position before the first order is submitted', async () => {
    const positionGet = vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [makeRawPosition('Long', 0)] });

    await exchange.placeOrder(limitOrder);

    expect(positionGet).toHaveBeenCalledWith('/position');
    expect(positionGet.mock.invocationCallOrder[0]).toBeLessThan(orderPost.mock.invocationCallOrder[0]);
    expect(orderPost).toHaveBeenCalledWith('/order', expect.objectContaining({ strategy: 'Long' }));
  });

  it('uses the official OneWay default when an authenticated account has no position rows', async () => {
    const positionGet = vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [] });

    await exchange.placeOrder(limitOrder);

    expect(positionGet).toHaveBeenCalledWith('/position');
    expect(orderPost.mock.calls[0][1]).not.toHaveProperty('strategy');
  });

  it('negotiates an empty Hedge account from an omitted strategy to each payload target leg', async () => {
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [] });
    orderPost.mockImplementation(async (_endpoint, payload) => {
      if (payload.strategy === undefined) {
        throw new Error('Invalid strategy: null');
      }
      return { data: { orderID: `${payload.strategy}-order` } };
    });

    await exchange.placeOrders([
      limitOrder,
      { ...limitOrder, side: OrderSide.Sell },
    ]);

    expect(orderPost.mock.calls.map(([, payload]) => payload.strategy)).toEqual([
      undefined,
      undefined,
      'Long',
      'Short',
    ]);
    expect(store.options.isHedged).toBe(true);

    orderPost.mockClear();
    orderPost.mockResolvedValue({ data: { orderID: 'observed-mode-order' } });
    await exchange.placeOrder(limitOrder);
    expect(orderPost).toHaveBeenCalledWith('/order', expect.objectContaining({ strategy: 'Long' }));
  });

  it('falls back from a rejected hedge strategy to omitted OneWay and persists the observed mode', async () => {
    exchange.observePositionMode([{ strategy: 'Long' }]);
    orderPost
      .mockRejectedValueOnce(new Error('Invalid strategy: Long'))
      .mockResolvedValueOnce({ data: { orderID: 'one-way-order' } });

    await exchange.placeOrder(limitOrder);

    expect(orderPost.mock.calls.map(([, payload]) => payload.strategy)).toEqual(['Long', undefined]);
    expect(store.options.isHedged).toBe(false);

    orderPost.mockClear();
    orderPost.mockResolvedValue({ data: { orderID: 'observed-mode-order' } });
    await exchange.placeOrder(limitOrder);
    expect(orderPost.mock.calls[0][1]).not.toHaveProperty('strategy');
  });

  it.each([
    { side: OrderSide.Buy, strategy: 'Short' },
    { side: OrderSide.Sell, strategy: 'Long' },
  ] as const)('maps a rejected omitted Close payload for $side to $strategy', async ({ side, strategy }) => {
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [] });
    orderPost
      .mockRejectedValueOnce(new Error('Invalid strategy: null'))
      .mockResolvedValueOnce({ data: { orderID: 'close-order' } });

    await exchange.placeOrder({
      ...limitOrder,
      side,
      type: OrderType.StopLoss,
      price: 49_000,
      reduceOnly: true,
    });

    expect(orderPost.mock.calls.map(([, payload]) => ({
      strategy: payload.strategy,
      execInst: payload.execInst,
    }))).toEqual([
      { strategy: undefined, execInst: 'Close' },
      { strategy, execInst: 'Close' },
    ]);
  });

  it('retries only rejected bracket payloads and retains their parent hedge leg', async () => {
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [] });
    orderPost.mockImplementation(async (_endpoint, payload) => {
      if (payload.strategy === undefined && payload.ordType !== 'Limit') {
        throw new Error('Invalid strategy: null');
      }
      return { data: { orderID: `${payload.ordType}-${payload.strategy ?? 'OneWay'}` } };
    });

    await exchange.placeOrder({ ...limitOrder, stopLoss: 49_000, takeProfit: 51_000 });

    const calls = orderPost.mock.calls.map(([, payload]) => ({
      ordType: payload.ordType,
      strategy: payload.strategy,
    }));
    expect(calls.filter(({ ordType }) => ordType === 'Limit')).toEqual([
      { ordType: 'Limit', strategy: undefined },
    ]);
    expect(calls.filter(({ ordType }) => ordType === 'StopMarket')).toEqual([
      { ordType: 'StopMarket', strategy: undefined },
      { ordType: 'StopMarket', strategy: 'Long' },
    ]);
    expect(calls.filter(({ ordType }) => ordType === 'MarketIfTouched')).toEqual([
      { ordType: 'MarketIfTouched', strategy: undefined },
      { ordType: 'MarketIfTouched', strategy: 'Long' },
    ]);
  });

  it('does not retry an unrelated order error', async () => {
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [] });
    const unrelatedError = new Error('Insufficient Available Balance');
    orderPost.mockRejectedValueOnce(unrelatedError);

    await expect(exchange.placeOrder(limitOrder)).rejects.toBe(unrelatedError);

    expect(orderPost).toHaveBeenCalledTimes(1);
    expect(store.options.isHedged).toBe(false);
  });


  it('retains an explicitly selected hedge mode across an empty position response', async () => {
    vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: {} });
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({ data: [] });

    await exchange.changePositionMode(true);
    await exchange.fetchOpenPositions();
    await exchange.placeOrder(limitOrder);

    expect(store.options.isHedged).toBe(true);
    expect(orderPost).toHaveBeenCalledWith('/order', expect.objectContaining({ strategy: 'Long' }));
  });

  it('uses the official account-level position-mode mutation bodies', async () => {
    const modePost = vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: {} });

    await exchange.changePositionMode(true);
    await exchange.changePositionMode(false);

    expect(modePost.mock.calls).toEqual([
      ['/user/positionMode', { positionMode: 'MultiWay' }],
      ['/user/positionMode', {}],
    ]);
    expect(store.options.isHedged).toBe(false);
  });

  it('replaces stale OneWay identity after switching to Hedged', async () => {
    const oldPosition = exchange.mapPosition(makeRawPosition('OneWay', 2));
    const authoritativePosition = exchange.mapPosition(makeRawPosition('Long', 2));
    expect(oldPosition).toBeDefined();
    expect(authoritativePosition).toBeDefined();
    store.update({ positions: [oldPosition!] });
    vi.mocked(exchange.fetchPositions).mockResolvedValue([authoritativePosition!]);
    vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: {} });

    await exchange.changePositionMode(true);

    expect(store.positions.map((position) => position.id)).toEqual(['XBTUSDT:long']);
    expect(store.positions[0].positionMode).toBe(PositionMode.Hedged);
  });

  it('replaces stale hedge-leg identities after switching to OneWay', async () => {
    const oldLong = exchange.mapPosition(makeRawPosition('Long', 2));
    const oldShort = exchange.mapPosition(makeRawPosition('Short', -2));
    const authoritativePosition = exchange.mapPosition(makeRawPosition('OneWay', -2));
    expect(oldLong).toBeDefined();
    expect(oldShort).toBeDefined();
    expect(authoritativePosition).toBeDefined();
    store.update({ positions: [oldLong!, oldShort!] });
    vi.mocked(exchange.fetchPositions).mockResolvedValue([authoritativePosition!]);
    vi.spyOn(exchange.xhr, 'post').mockResolvedValue({ data: {} });

    await exchange.changePositionMode(false);

    expect(store.positions.map((position) => position.id)).toEqual(['XBTUSDT']);
    expect(store.positions[0].positionMode).toBe(PositionMode.OneWay);
  });


  it('advertises both supported BitMEX position modes', () => {
    expect(exchangeMetadata.bitmex.positionModes).toEqual([PositionMode.OneWay, PositionMode.Hedged]);
    expect(exchangeMetadata.bitmextestnet.positionModes).toEqual([PositionMode.OneWay, PositionMode.Hedged]);
  });
});
