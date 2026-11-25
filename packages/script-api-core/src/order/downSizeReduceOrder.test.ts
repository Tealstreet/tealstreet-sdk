import { describe, expect, it, vi, beforeEach } from 'vitest';
import {
  Market,
  OrderSide,
  OrderStatus,
  OrderType,
  Position,
  PositionMode,
  PositionSide,
  PlaceOrderOpts,
  type Order,
} from '@tealstreet/safe-cex/src/types';

import { downSizeReduceOrder, ReduceOrderLockedError } from './downSizeReduceOrder';

/** Was `@/components/openOrders/OpenOrder.type` before this moved into the
 *  package; the function reads none of the `account` field, so the shape is
 *  declared locally rather than dragging a web type across. */
type OpenOrder = Order & { account: string };

// Mock roundToNearestMultiple for deterministic output
vi.mock('@tealstreet/math/src/roundToNearestMultiple', () => ({
  roundToNearestMultiple: (n: number, m: number) => Math.round(n / m) * m,
}));

const mockMarket: Market = {
  id: 'BTCUSD',
  symbol: 'BTCUSD',
  base: 'BTC',
  quote: 'USD',
  active: true,
  isSpot: false,
  info: {},
  precision: { amount: 0.001, price: 0.1 },
  limits: {
    amount: { min: 0.001, max: 100000 },
    leverage: { min: 1, max: 100 },
  },
};

const createPosition = (overrides: Partial<Position> = {}): Position => ({
  id: 'pos1',
  symbol: 'BTCUSD',
  side: PositionSide.Long,
  entryPrice: 50000,
  notional: 5000,
  leverage: 10,
  unrealizedPnl: 100,
  contracts: 0.1,
  liquidationPrice: 45000,
  positionMode: PositionMode.OneWay,
  ...overrides,
});

const createOrder = (overrides: Partial<PlaceOrderOpts> = {}): PlaceOrderOpts => ({
  symbol: 'BTCUSD',
  type: OrderType.Limit,
  side: OrderSide.Sell,
  amount: 0.05,
  price: 51000,
  reduceOnly: true,
  ...overrides,
});

const createOpenOrder = (overrides: Partial<OpenOrder> = {}): OpenOrder => ({
  id: 'order1',
  symbol: 'BTCUSD',
  type: OrderType.Limit,
  side: OrderSide.Sell,
  price: 52000,
  amount: 0.02,
  filled: 0,
  remaining: 0.02,
  status: OrderStatus.Open,
  reduceOnly: true,
  lastUpdate: Date.now(),
  account: 'test-account',
  ...overrides,
});

describe('downSizeReduceOrder', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('early returns', () => {
    it('returns order unchanged if reduceOnly is false', () => {
      const order = createOrder({ reduceOnly: false });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 51000);
      expect(result).toEqual(order);
    });

    it('returns order unchanged if reduceOnly is undefined', () => {
      const order = createOrder({ reduceOnly: undefined });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 51000);
      expect(result).toEqual(order);
    });

    it('returns order unchanged if price is 0', () => {
      const order = createOrder();
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 0);
      expect(result).toEqual(order);
    });

    it('returns order unchanged if no matching positions found', () => {
      const order = createOrder({ side: OrderSide.Sell });
      // Short position doesn't match sell order (sell reduces long, not short)
      const position = createPosition({ side: PositionSide.Short, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 51000);
      expect(result).toEqual(order);
    });

    it('returns order unchanged if positions have zero contracts', () => {
      const order = createOrder({ side: OrderSide.Sell });
      const position = createPosition({ side: PositionSide.Long, contracts: 0 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 51000);
      expect(result).toEqual(order);
    });
  });

  describe('position side matching', () => {
    it('matches sell order to long position', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.05 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 51000);
      expect(result.amount).toBe(0.05);
    });

    it('matches buy order to short position', () => {
      const order = createOrder({ side: OrderSide.Buy, amount: 0.05 });
      const position = createPosition({ side: PositionSide.Short, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 49000);
      expect(result.amount).toBe(0.05);
    });
  });

  describe('size reduction based on position', () => {
    it('limits order size to position size if order exceeds position', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.2 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 51000);
      expect(result.amount).toBe(0.1);
    });

    it('keeps order size unchanged if smaller than position', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.05 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 51000);
      expect(result.amount).toBe(0.05);
    });
  });

  describe('orders ahead calculation for sell orders', () => {
    it('subtracts existing sell orders at or below the new order price', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1, price: 52000 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 51000, // below 52000
        amount: 0.03,
        status: OrderStatus.Open,
        reduceOnly: true,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 52000);
      // 0.1 (position) - 0.03 (existing order) = 0.07
      expect(result.amount).toBe(0.07);
    });

    it('ignores sell orders above the new order price', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1, price: 50000 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 52000, // above 50000
        amount: 0.03,
        status: OrderStatus.Open,
        reduceOnly: true,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 50000);
      // Existing order is at higher price, not counted as "ahead"
      expect(result.amount).toBe(0.1);
    });
  });

  describe('orders ahead calculation for buy orders', () => {
    it('subtracts existing buy orders at or above the new order price', () => {
      const order = createOrder({ side: OrderSide.Buy, amount: 0.1, price: 49000 });
      const position = createPosition({ side: PositionSide.Short, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Buy,
        price: 50000, // above 49000
        amount: 0.03,
        status: OrderStatus.Open,
        reduceOnly: true,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 49000);
      // 0.1 (position) - 0.03 (existing order) = 0.07
      expect(result.amount).toBe(0.07);
    });

    it('ignores buy orders below the new order price', () => {
      const order = createOrder({ side: OrderSide.Buy, amount: 0.1, price: 50000 });
      const position = createPosition({ side: PositionSide.Short, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Buy,
        price: 48000, // below 50000
        amount: 0.03,
        status: OrderStatus.Open,
        reduceOnly: true,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 50000);
      // Existing order is at lower price, not counted as "ahead"
      expect(result.amount).toBe(0.1);
    });
  });

  describe('order status filtering', () => {
    it('includes orders with Open status', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.03,
        status: OrderStatus.Open,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 51000);
      expect(result.amount).toBe(0.07);
    });

    it('includes orders with Creating status', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.03,
        status: OrderStatus.Creating,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 51000);
      expect(result.amount).toBe(0.07);
    });

    it('ignores orders with Closed status', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.03,
        status: OrderStatus.Closed,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 51000);
      expect(result.amount).toBe(0.1);
    });

    it('ignores orders with Canceled status', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.03,
        status: OrderStatus.Canceled,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 51000);
      expect(result.amount).toBe(0.1);
    });
  });

  describe('order type filtering', () => {
    it('only considers Limit orders', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const limitOrder = createOpenOrder({
        type: OrderType.Limit,
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.03,
      });
      const marketOrder = createOpenOrder({
        type: OrderType.Market,
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.02,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [limitOrder, marketOrder], 51000);
      // Only limit order is considered
      expect(result.amount).toBe(0.07);
    });

    it('ignores StopLoss orders', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const stopLossOrder = createOpenOrder({
        type: OrderType.StopLoss,
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.03,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [stopLossOrder], 51000);
      expect(result.amount).toBe(0.1);
    });
  });

  describe('hedged position mode', () => {
    it('only considers reduceOnly orders in hedged mode', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({
        side: PositionSide.Long,
        contracts: 0.1,
        positionMode: PositionMode.Hedged,
      });
      const reduceOnlyOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.03,
        reduceOnly: true,
      });
      const nonReduceOnlyOrder = createOpenOrder({
        id: 'order2',
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.02,
        reduceOnly: false,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [reduceOnlyOrder, nonReduceOnlyOrder], 51000);
      // Only reduceOnly order is considered in hedged mode
      expect(result.amount).toBe(0.07);
    });

    it('considers all orders in one-way mode', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({
        side: PositionSide.Long,
        contracts: 0.1,
        positionMode: PositionMode.OneWay,
      });
      const reduceOnlyOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.03,
        reduceOnly: true,
      });
      const nonReduceOnlyOrder = createOpenOrder({
        id: 'order2',
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.02,
        reduceOnly: false,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [reduceOnlyOrder, nonReduceOnlyOrder], 51000);
      // Both orders are considered in one-way mode
      expect(result.amount).toBe(0.05);
    });
  });

  describe('reduced size <= 0', () => {
    it('returns original order if reduced size is zero', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.05 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.1, // fully covers position
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 51000);
      expect(result).toEqual(order);
    });

    it('returns original order if reduced size is negative', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.05 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.15, // exceeds position
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 51000);
      expect(result).toEqual(order);
    });
  });

  describe('precision rounding', () => {
    it('rounds amount to market precision', () => {
      const market = { ...mockMarket, precision: { amount: 0.01, price: 0.1 } };
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.033, // leaves 0.067
      });
      const result = downSizeReduceOrder(market, order, [position], [existingOrder], 51000);
      // 0.1 - 0.033 = 0.067, rounded to 0.07
      expect(result.amount).toBe(0.07);
    });

    it('uses default precision when market precision is not available', () => {
      const market = { ...mockMarket, precision: undefined } as unknown as Market;
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const result = downSizeReduceOrder(market, order, [position], [], 51000);
      expect(result.amount).toBe(0.1);
    });
  });

  describe('multiple positions', () => {
    it('uses first matching position when multiple exist', () => {
      const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});
      const order = createOrder({ side: OrderSide.Sell, amount: 0.2 });
      const position1 = createPosition({ id: 'pos1', side: PositionSide.Long, contracts: 0.05 });
      const position2 = createPosition({ id: 'pos2', side: PositionSide.Long, contracts: 0.15 });
      const result = downSizeReduceOrder(mockMarket, order, [position1, position2], [], 51000);
      // Uses first position (0.05)
      expect(result.amount).toBe(0.05);
      expect(consoleSpy).toHaveBeenCalledWith('autoDownSizeReduceOrders: found multiple positions, using first');
      consoleSpy.mockRestore();
    });
  });

  describe('symbol filtering', () => {
    it('ignores positions with different symbol', () => {
      const order = createOrder({ symbol: 'BTCUSD', side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ symbol: 'ETHUSD', side: PositionSide.Long, contracts: 0.1 });
      const result = downSizeReduceOrder(mockMarket, order, [position], [], 51000);
      expect(result).toEqual(order);
    });

    it('ignores orders with different symbol', () => {
      const order = createOrder({ symbol: 'BTCUSD', side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ symbol: 'BTCUSD', side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        symbol: 'ETHUSD',
        side: OrderSide.Sell,
        price: 50000,
        amount: 0.05,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 51000);
      // Different symbol order is ignored
      expect(result.amount).toBe(0.1);
    });
  });

  describe('order side filtering', () => {
    it('ignores orders with different side', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 0.1 });
      const position = createPosition({ side: PositionSide.Long, contracts: 0.1 });
      const existingOrder = createOpenOrder({
        side: OrderSide.Buy,
        price: 50000,
        amount: 0.05,
      });
      const result = downSizeReduceOrder(mockMarket, order, [position], [existingOrder], 51000);
      // Different side order is ignored
      expect(result.amount).toBe(0.1);
    });
  });

  describe('MEXC-style fractional-lot linear futures', () => {
    // MEXC BTC_USDT: contractSize=0.0001, volUnit=1. `position.contracts` is stored in
    // base-asset units (0.006 BTC) while `order.amount` from createMarketOrder is native
    // contract volume (60). Historically the clamp compared these directly, collapsing
    // the outgoing amount to 0.006 which MEXC's placeOrder then rounded down to `vol: 1`
    // (the "close only closes 1 lot" bug). The clamp now runs in base units and converts
    // back to native, so full-close and partial-close intent survives.
    const mexcMarket: Market = {
      id: 'BTC_USDT',
      symbol: 'BTCUSDT',
      base: 'BTC',
      quote: 'USDT',
      active: true,
      isSpot: false,
      isInverse: false,
      info: { volUnit: 1 },
      precision: { amount: 0.0001, price: 0.1 },
      limits: {
        amount: { min: 0.0001, max: 100000 },
        leverage: { min: 1, max: 100 },
      },
      contractSize: 0.0001,
    };

    it('preserves the full native close amount when order matches the whole position', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 60, price: 50000 });
      const position = createPosition({
        side: PositionSide.Long,
        contracts: 0.006,
        extra: { contractsNative: 60 },
      });
      const result = downSizeReduceOrder(mexcMarket, order, [position], [], 50000);
      expect(result.amount).toBe(60);
    });

    it('clamps an oversized native close down to the native position size', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 90, price: 50000 });
      const position = createPosition({
        side: PositionSide.Long,
        contracts: 0.006,
        extra: { contractsNative: 60 },
      });
      const result = downSizeReduceOrder(mexcMarket, order, [position], [], 50000);
      expect(result.amount).toBe(60);
    });

    it('preserves a smaller partial close in native units', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 45, price: 50000 });
      const position = createPosition({
        side: PositionSide.Long,
        contracts: 0.006,
        extra: { contractsNative: 60 },
      });
      const result = downSizeReduceOrder(mexcMarket, order, [position], [], 50000);
      expect(result.amount).toBe(45);
    });

    it('subtracts orders-ahead expressed in base-asset units and returns native', () => {
      const order = createOrder({ side: OrderSide.Sell, amount: 60, price: 50000 });
      const position = createPosition({
        side: PositionSide.Long,
        contracts: 0.006,
        extra: { contractsNative: 60 },
      });
      // Existing reduce-only sell at or below the new price for 0.002 BTC == 20 native
      // contracts; the outgoing native amount should be 60 - 20 = 40.
      const existingOrder = createOpenOrder({
        side: OrderSide.Sell,
        price: 49000,
        amount: 0.002,
        status: OrderStatus.Open,
        reduceOnly: true,
      });
      const result = downSizeReduceOrder(mexcMarket, order, [position], [existingOrder], 50000);
      expect(result.amount).toBe(40);
    });
  });
});

describe('a remainder that rounds to zero', () => {
  // MEXC-style fractional lots: native contracts of 0.0001 BTC, base-step precision.
  const mexcMarket: Market = { ...mockMarket, contractSize: 0.0001, precision: { amount: 0.0001, price: 0.1 } };

  it('keeps the placed order instead of returning amount 0 when orders ahead cover the position', () => {
    const order = createOrder({ type: OrderType.Market, side: OrderSide.Buy, amount: 200, price: undefined });
    // 0.05 - 0.02 - 0.03 is 3.5e-18 in floating point, not 0.
    const position = createPosition({ side: PositionSide.Short, contracts: 0.05 });
    const ahead = [
      createOpenOrder({ id: 'a', side: OrderSide.Buy, price: 60000, amount: 0.02 }),
      createOpenOrder({ id: 'b', side: OrderSide.Buy, price: 60000, amount: 0.03 }),
    ];
    const result = downSizeReduceOrder(mexcMarket, order, [position], ahead, 50000);
    expect(result.amount).toBe(200);
  });

  it('keeps the placed order when the reducible remainder is below half a lot', () => {
    const order = createOrder({ type: OrderType.Market, side: OrderSide.Buy, amount: 200, price: undefined });
    const position = createPosition({ side: PositionSide.Short, contracts: 0.00004 });
    const result = downSizeReduceOrder(mexcMarket, order, [position], [], 50000);
    expect(result.amount).toBe(200);
  });
});

// Bitget classic: sized close plans and reduce-only limits hold position size; only the free part closes.
describe('a venue whose resting close orders lock position size', () => {
  const locks = { locksPositionSize: true };
  const plan = (overrides: Partial<OpenOrder> = {}) =>
    createOpenOrder({ type: OrderType.TakeProfit, price: 60000, amount: 0.1451, remaining: 0.1451, ...overrides });
  const close = (amount: number) => createOrder({ type: OrderType.Market, amount, price: undefined });
  const position = (contracts: number) => [createPosition({ contracts })];

  // AGT-44: this used to expect 0.197 (TP + SL summed as held); a TP and an SL on one slice are
  // alternatives, so the pair holds 0.1451 and 0.3421 is free.
  it('caps a close to the free part when sized TP/SL plans hold the rest, and says so', () => {
    const onLocked = vi.fn();
    const plans = [plan({ id: 'tp' }), plan({ id: 'sl', type: OrderType.StopLoss, price: 40000 })];
    const result = downSizeReduceOrder(mockMarket, close(0.4872), position(0.4872), plans, 50000, {
      ...locks,
      onLocked,
    });
    expect(result.amount).toBeCloseTo(0.342, 9);
    expect(onLocked).toHaveBeenCalledOnce();
    expect(onLocked.mock.calls[0][0]).toMatchObject({ symbol: 'BTCUSD', total: 0.4872, orders: 2 });
    expect(onLocked.mock.calls[0][0].locked).toBeCloseTo(0.1451, 9);
    expect(onLocked.mock.calls[0][0].sent).toBeCloseTo(0.342, 9);
  });

  it('does not count position-level TP/SL, which carry no size', () => {
    const onLocked = vi.fn();
    const positionLevel = [plan({ id: 'ptp', amount: 0, remaining: 0 }), plan({ id: 'psl', amount: 0, remaining: 0 })];
    const result = downSizeReduceOrder(mockMarket, close(0.1), position(0.1), positionLevel, 50000, {
      ...locks,
      onLocked,
    });
    expect(result.amount).toBeCloseTo(0.1, 9);
    expect(onLocked).not.toHaveBeenCalled();
  });

  it('counts a reduce-only limit at any price, behind the close as well as ahead of it', () => {
    const behind = createOpenOrder({ id: 'far', price: 90000, amount: 0.04, remaining: 0.04 });
    const result = downSizeReduceOrder(mockMarket, close(0.1), position(0.1), [behind], 50000, locks);
    expect(result.amount).toBeCloseTo(0.06, 9);
  });

  it('counts only the unfilled part of a partially filled close order', () => {
    const partly = plan({ id: 'tp', amount: 0.05, filled: 0.03, remaining: 0.02 });
    const result = downSizeReduceOrder(mockMarket, close(0.1), position(0.1), [partly], 50000, locks);
    expect(result.amount).toBeCloseTo(0.08, 9);
  });

  // 10-30 USELESSUSDT: a 20-order reduce-only ladder sized to the whole position froze it.
  it('refuses a close when a ladder holds the whole position, naming how much and by how many orders', () => {
    const market = { ...mockMarket, id: 'USELESSUSDT', symbol: 'USELESSUSDT', precision: { amount: 1, price: 0.0001 } };
    const rung = 143916 / 20;
    const ladder = Array.from({ length: 20 }, (_, i) =>
      createOpenOrder({
        id: `rung-${i}`,
        symbol: 'USELESSUSDT',
        price: 0.3 + i * 0.01,
        amount: rung,
        remaining: rung,
      }),
    );
    const order = createOrder({ symbol: 'USELESSUSDT', type: OrderType.Market, amount: 143916, price: undefined });
    const held = [createPosition({ symbol: 'USELESSUSDT', contracts: 143916 })];
    const run = () => downSizeReduceOrder(market, order, held, ladder, 0.2, locks);
    expect(run).toThrow(ReduceOrderLockedError);
    expect(run).toThrow('143916 of your USELESSUSDT position is reserved by 20 open close orders; cancel them first');
  });

  it('never says more is reserved than the position holds', () => {
    const oversized = [plan({ id: 'tp', amount: 0.15, remaining: 0.15 })];
    expect(() => downSizeReduceOrder(mockMarket, close(0.1), position(0.1), oversized, 50000, locks)).toThrow(
      '0.1 of your BTCUSD position is reserved by 1 open close order; cancel them first',
    );
  });

  it('leaves an unflagged venue exactly as before for the same orders', () => {
    const plans = [plan({ id: 'tp' }), plan({ id: 'sl', type: OrderType.StopLoss, price: 40000 })];
    const behind = createOpenOrder({ id: 'far', price: 90000, amount: 0.04, remaining: 0.04 });
    const onLocked = vi.fn();
    // Today's result: the position cap at the market's precision, nothing held back for close orders.
    for (const [amount, contracts, expected] of [
      [0.4872, 0.4872, 0.487],
      [0.1, 0.1, 0.1],
    ] as const) {
      const args = [mockMarket, close(amount), position(contracts), [...plans, behind], 50000] as const;
      const unflagged = downSizeReduceOrder(...args, { onLocked });
      expect(unflagged).toEqual(downSizeReduceOrder(...args));
      expect(unflagged.amount).toBeCloseTo(expected, 9);
    }
    expect(onLocked).not.toHaveBeenCalled();
  });
});

// AGT-44 (bitget-t, 2026-11-19): long 0.0004 with a partial SL 0.0002 and a partial TP 0.0002; a
// reduce-only market close of 0.0002 was refused as "0.0004 ... reserved by 2 open close orders".
describe('a take profit and a stop loss on the same slice are alternatives, not a sum', () => {
  const locks = { locksPositionSize: true };
  const market = { ...mockMarket, id: 'BTCUSDT', symbol: 'BTCUSDT', precision: { amount: 0.0001, price: 0.1 } };
  const long = (contracts: number, positionMode = PositionMode.OneWay) =>
    createPosition({ symbol: 'BTCUSDT', side: PositionSide.Long, contracts, positionMode });
  const short = (contracts: number, positionMode = PositionMode.OneWay) =>
    createPosition({ symbol: 'BTCUSDT', side: PositionSide.Short, contracts, positionMode });
  const exit = (id: string, type: OrderType, amount: number, price: number, side = OrderSide.Sell) =>
    createOpenOrder({ id, symbol: 'BTCUSDT', side, type, price, amount, remaining: amount });
  const close = (amount: number, side = OrderSide.Sell) =>
    createOrder({ symbol: 'BTCUSDT', side, type: OrderType.Market, amount, price: undefined });
  const longTp = (amount: number) => exit('tp', OrderType.TakeProfit, amount, 66924.8);
  const longSl = (amount: number) => exit('sl', OrderType.StopLoss, amount, 60551);

  it('closes the unprotected half when a TP and an SL each cover the other half', () => {
    const onLocked = vi.fn();
    const result = downSizeReduceOrder(market, close(0.0002), [long(0.0004)], [longSl(0.0002), longTp(0.0002)], 63000, {
      ...locks,
      onLocked,
    });
    expect(result.amount).toBeCloseTo(0.0002, 12);
    expect(onLocked).not.toHaveBeenCalled();
  });

  it('cuts a full close to the part neither side holds, and reports the larger side as held', () => {
    const onLocked = vi.fn();
    const result = downSizeReduceOrder(market, close(0.0004), [long(0.0004)], [longSl(0.0002), longTp(0.0002)], 63000, {
      ...locks,
      onLocked,
    });
    expect(result.amount).toBeCloseTo(0.0002, 12);
    expect(onLocked).toHaveBeenCalledOnce();
    expect(onLocked.mock.calls[0][0].locked).toBeCloseTo(0.0002, 12);
    expect(onLocked.mock.calls[0][0].orders).toBe(2);
  });

  it('still refuses when either side alone covers the whole position', () => {
    const run = () =>
      downSizeReduceOrder(market, close(0.0002), [long(0.0004)], [longSl(0.0004), longTp(0.0004)], 63000, locks);
    expect(run).toThrow(ReduceOrderLockedError);
    expect(run).toThrow('0.0004 of your BTCUSDT position is reserved by 2 open close orders; cancel them first');
  });

  it('sums the exits on one side: a reduce-only limit counts with the take profits', () => {
    const limit = exit('limit', OrderType.Limit, 0.0002, 65000);
    const run = () => downSizeReduceOrder(market, close(0.0002), [long(0.0004)], [limit, longTp(0.0002)], 63000, locks);
    expect(run).toThrow(ReduceOrderLockedError);
  });

  it('holds the larger of the take-profit total and the stop total', () => {
    const limit = exit('limit', OrderType.Limit, 0.0001, 65000);
    const trailing = exit('trail', OrderType.TrailingStopLoss, 0.0001, 61000);
    const result = downSizeReduceOrder(
      market,
      close(0.0004),
      [long(0.0004)],
      [limit, longTp(0.0001), longSl(0.0002), trailing],
      63000,
      locks,
    );
    expect(result.amount).toBeCloseTo(0.0001, 12);
  });

  it('counts a close order of unknown direction outright, never as an alternative', () => {
    const conditional = exit('cond', OrderType.Conditional, 0.0002, 61000);
    const run = () =>
      downSizeReduceOrder(market, close(0.0002), [long(0.0004)], [longSl(0.0002), conditional], 63000, locks);
    expect(run).toThrow(ReduceOrderLockedError);
  });

  describe('in hedge mode, each position side takes only its own exits', () => {
    const positions = [long(0.0004, PositionMode.Hedged), short(0.0004, PositionMode.Hedged)];
    const shortTp = exit('stp', OrderType.TakeProfit, 0.0004, 59000, OrderSide.Buy);
    const shortSl = exit('ssl', OrderType.StopLoss, 0.0004, 67000, OrderSide.Buy);
    const resting = [longSl(0.0002), longTp(0.0002), shortTp, shortSl];

    it('closes the unprotected half of the long beside a fully protected short', () => {
      const result = downSizeReduceOrder(market, close(0.0002), positions, resting, 63000, locks);
      expect(result.amount).toBeCloseTo(0.0002, 12);
    });

    it('refuses the short, whose TP and SL each cover all of it', () => {
      const run = () => downSizeReduceOrder(market, close(0.0002, OrderSide.Buy), positions, resting, 63000, locks);
      expect(run).toThrow('0.0004 of your BTCUSDT position is reserved by 2 open close orders; cancel them first');
    });
  });
});

// AGT-70 (bitget-t, 2026-11-19): a whole-position SL refused every partial close as "0.02 ... reserved
// by 1 open close order". The Bitget venue maps a whole-position plan (pos_loss/pos_profit) with amount
// 0, because it closes whatever is open when it triggers; this pins that amount 0 holds nothing.
describe('a whole-position TP/SL holds no size', () => {
  const market = { ...mockMarket, id: 'ETHUSDT', symbol: 'ETHUSDT', precision: { amount: 0.01, price: 0.01 } };
  const long = createPosition({ symbol: 'ETHUSDT', side: PositionSide.Long, contracts: 0.02 });
  const wholeSl = createOpenOrder({
    id: 'sl',
    symbol: 'ETHUSDT',
    side: OrderSide.Sell,
    type: OrderType.StopLoss,
    price: 1605.69,
    amount: 0,
    remaining: 0,
  });

  it('closes part of the position while a whole-position SL rests', () => {
    const onLocked = vi.fn();
    const close = createOrder({ symbol: 'ETHUSDT', side: OrderSide.Sell, type: OrderType.Market, amount: 0.01 });
    const result = downSizeReduceOrder(market, { ...close, price: undefined }, [long], [wholeSl], 1700, {
      locksPositionSize: true,
      onLocked,
    });
    expect(result.amount).toBeCloseTo(0.01, 12);
    expect(onLocked).not.toHaveBeenCalled();
  });
});
