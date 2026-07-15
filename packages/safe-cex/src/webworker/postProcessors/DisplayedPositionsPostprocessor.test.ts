import type {
  ActiveChaser,
  ActiveTWAP,
  Balance,
  ExchangeName,
  ExchangeOptions,
  Market,
  Order,
  Position,
  StoreData,
  Ticker,
  Writable,
} from '../../types';

import { describe, expect, it } from 'vitest';

import { OrderSide, OrderStatus, OrderType, PositionSide } from '../../types';
import { TwapStatus } from '../twap/TwapStatus';
import { DisplayedPositionsPostprocessor } from './DisplayedPositionsPostprocessor';

const balance: Balance = { used: 10, free: 90, total: 100, upnl: 5 };

const createMarket = (overrides?: Partial<Market>): Market =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.001, price: 0.01 },
    limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
    ...overrides,
  }) as Market;

const createTicker = (overrides?: Partial<Ticker>): Ticker =>
  ({
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    bid: 109,
    ask: 111,
    last: 110,
    mark: 110,
    index: 110,
    ...overrides,
  }) as Ticker;

const createPosition = (overrides?: Partial<Position>): Position =>
  ({
    id: 'position-1',
    symbol: 'BTCUSDT',
    side: PositionSide.Long,
    contracts: 2,
    entryPrice: 100,
    notional: 200,
    unrealizedPnl: 7,
    leverage: 10,
    liquidationPrice: 50,
    ...overrides,
  }) as Position;

const createActiveChaser = (overrides?: Partial<ActiveChaser>): ActiveChaser => ({
  id: 'chaser-1',
  symbol: 'BTCUSDT',
  size: 1,
  side: OrderSide.Buy,
  maxAttempts: 3,
  attempts: 1,
  isStopped: false,
  stopReason: '',
  currentOrderIds: [],
  ...overrides,
});

const createActiveTwap = (overrides?: Partial<ActiveTWAP>): ActiveTWAP =>
  ({
    id: 'twap-1',
    accountName: 'test-account',
    activeMarket: createMarket(),
    side: OrderSide.Buy,
    size: 1,
    status: TwapStatus.Running,
    ...overrides,
  }) as ActiveTWAP;

const createOrder = (overrides?: Partial<Order>): Order =>
  ({
    id: 'order-1',
    symbol: 'BTCUSDT',
    side: OrderSide.Sell,
    type: OrderType.StopLoss,
    status: OrderStatus.Open,
    amount: 2,
    price: 90,
    ...overrides,
  }) as Order;

const createStore = (overrides?: Partial<StoreData>): Writable<StoreData> =>
  ({
    balance,
    markets: [createMarket()],
    tickers: [createTicker()],
    positions: [],
    displayedPositions: [],
    activeChasers: [],
    activeTWAPs: [],
    ...overrides,
  }) as Writable<StoreData>;

const createProcessor = (options: ExchangeOptions = {} as ExchangeOptions) =>
  new DisplayedPositionsPostprocessor('test-account', 'bybit' as ExchangeName, options);

describe('DisplayedPositionsPostprocessor', () => {
  it('builds displayed positions with account data and active flags in one pass', () => {
    const market = createMarket();
    const ticker = createTicker();
    const store = createStore({
      markets: [market],
      tickers: [ticker],
      positions: [createPosition({ contracts: -2 })],
      activeChasers: [createActiveChaser()],
      activeTWAPs: [createActiveTwap({ activeMarket: market })],
    });

    createProcessor().process(store);

    expect(store.displayedPositions).toHaveLength(1);
    expect(store.displayedPositions[0]).toMatchObject({
      id: 'position-1',
      account: 'test-account',
      exchangeName: 'bybit',
      contracts: 2,
      unrealizedPnl: 20,
      hasActiveChaser: true,
      hasActiveTWAP: true,
      oneMinUpnl: 20,
      fiveMinUpnl: 20,
    });
    expect(store.displayedPositions[0].ticker).toBe(ticker);
    expect(store.displayedPositions[0].market).toBe(market);
    expect(store.displayedPositions[0].balance).toBe(balance);
  });

  it('preserves reduce-only chaser side semantics for active flags', () => {
    const store = createStore({
      positions: [createPosition({ side: PositionSide.Short })],
      activeChasers: [createActiveChaser({ side: OrderSide.Buy, reduceOnly: true })],
    });

    createProcessor().process(store);

    expect(store.displayedPositions[0].hasActiveChaser).toBe(true);
  });

  it('calculates unrealized pnl from ticker last when tuning requests it', () => {
    const store = createStore({
      positions: [createPosition({ contracts: 3, entryPrice: 100, unrealizedPnl: 0 })],
      tickers: [createTicker({ last: 112 })],
    });

    createProcessor({ tuning: { calculateUpnlFromRealtimeLastPrice: true } } as ExchangeOptions).process(store);

    expect(store.displayedPositions[0].unrealizedPnl).toBe(36);
  });

  it('does not let a zero-contract placeholder suppress a fake chaser position', () => {
    const store = createStore({
      positions: [createPosition({ id: 'flat-position', contracts: 0 })],
      activeChasers: [
        createActiveChaser({ id: 'btc-chaser', symbol: 'BTCUSDT', side: OrderSide.Buy, size: 4 }),
        createActiveChaser({ id: 'eth-chaser', symbol: 'ETHUSDT', side: OrderSide.Sell, size: 2 }),
      ],
      markets: [createMarket(), createMarket({ id: 'ETHUSDT', symbol: 'ETHUSDT', base: 'ETH' })],
      tickers: [createTicker(), createTicker({ id: 'ETHUSDT', symbol: 'ETHUSDT', last: 2000 })],
    });

    createProcessor().process(store);

    expect(store.displayedPositions).toHaveLength(2);
    expect(store.displayedPositions.find((position) => position.id === 'fake-chaser-btc-chaser')).toMatchObject({
      symbol: 'BTCUSDT',
      side: PositionSide.Long,
      isFakePosition: true,
      chaserSize: 4,
    });
    expect(store.displayedPositions.find((position) => position.id === 'fake-chaser-eth-chaser')).toMatchObject({
      id: 'fake-chaser-eth-chaser',
      symbol: 'ETHUSDT',
      side: PositionSide.Short,
      isFakePosition: true,
      chaserSize: 2,
      hasActiveChaser: true,
      hasFullStop: false,
      hasFullTP: false,
    });
  });

  it('marks full stop coverage when a close-entire-position stop exists (amount === 0)', () => {
    const store = createStore({
      positions: [createPosition({ contracts: 2 })],
      orders: [createOrder({ side: OrderSide.Sell, type: OrderType.StopLoss, amount: 0 })],
    });

    createProcessor().process(store);

    expect(store.displayedPositions[0].hasFullStop).toBe(true);
    expect(store.displayedPositions[0].hasFullTP).toBe(false);
  });

  it('marks full stop coverage when partial stop amounts sum to at least the position size', () => {
    const store = createStore({
      positions: [createPosition({ contracts: 2 })],
      orders: [
        createOrder({ id: 'sl-a', side: OrderSide.Sell, type: OrderType.StopLoss, amount: 1 }),
        createOrder({ id: 'sl-b', side: OrderSide.Sell, type: OrderType.StopLoss, amount: 1 }),
      ],
    });

    createProcessor().process(store);

    expect(store.displayedPositions[0].hasFullStop).toBe(true);
  });

  it('leaves stop coverage incomplete when partial stop amounts are below the position size', () => {
    const store = createStore({
      positions: [createPosition({ contracts: 2 })],
      orders: [createOrder({ side: OrderSide.Sell, type: OrderType.StopLoss, amount: 1 })],
    });

    createProcessor().process(store);

    expect(store.displayedPositions[0].hasFullStop).toBe(false);
  });

  it('treats limit orders as take-profit coverage', () => {
    const store = createStore({
      positions: [createPosition({ contracts: 2 })],
      orders: [createOrder({ side: OrderSide.Sell, type: OrderType.Limit, amount: 2 })],
    });

    createProcessor().process(store);

    expect(store.displayedPositions[0].hasFullTP).toBe(true);
    expect(store.displayedPositions[0].hasFullStop).toBe(false);
  });

  it('keeps long and short coverage independent for a hedged symbol', () => {
    const store = createStore({
      positions: [
        createPosition({ id: 'long', side: PositionSide.Long, contracts: 2 }),
        createPosition({ id: 'short', side: PositionSide.Short, contracts: 2 }),
      ],
      // Sell stop closes the long; no order closes the short.
      orders: [createOrder({ side: OrderSide.Sell, type: OrderType.StopLoss, amount: 0 })],
    });

    createProcessor().process(store);

    const long = store.displayedPositions.find((p) => p.id === 'long');
    const short = store.displayedPositions.find((p) => p.id === 'short');
    expect(long?.hasFullStop).toBe(true);
    expect(short?.hasFullStop).toBe(false);
  });

  it('ignores non-open orders for coverage', () => {
    const store = createStore({
      positions: [createPosition({ contracts: 2 })],
      orders: [createOrder({ side: OrderSide.Sell, type: OrderType.StopLoss, amount: 0, status: OrderStatus.Closed })],
    });

    createProcessor().process(store);

    expect(store.displayedPositions[0].hasFullStop).toBe(false);
  });

  it('evicts rolling pnl queues after a position closes', () => {
    const processor = createProcessor();
    const store = createStore({ positions: [createPosition()] });

    processor.process(store);
    expect((processor as any)._lastMinutePnl.size).toBe(1);
    expect((processor as any)._lastFiveMinutesPnl.size).toBe(1);

    store.positions = [createPosition({ contracts: 0 })];
    processor.process(store);

    expect((processor as any)._lastMinutePnl.size).toBe(0);
    expect((processor as any)._lastFiveMinutesPnl.size).toBe(0);
  });
});
