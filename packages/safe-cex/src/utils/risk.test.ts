import { describe, expect, it } from 'vitest';

import { Order, OrderSide, OrderStatus, OrderType, Position, PositionMode, PositionSide } from '../types';
import {
  calculateRiskAtPrice,
  filterAndSortOrdersInLossDirection,
  filterAndSortOrdersInProfitDirection,
  formatPnl,
} from './risk';

// Helper to create mock orders
const createOrder = (overrides: Partial<Order> = {}): Order => ({
  id: 'order-1',
  status: OrderStatus.Open,
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Sell,
  price: 50000,
  amount: 1,
  filled: 0,
  remaining: 1,
  reduceOnly: false,
  lastUpdate: Date.now(),
  ...overrides,
});

const createPosition = (overrides: Partial<Position> = {}): Position => ({
  id: 'position-1',
  symbol: 'BTCUSDT',
  side: PositionSide.Long,
  entryPrice: 50000,
  notional: 50000,
  leverage: 10,
  positionMode: PositionMode.OneWay,
  unrealizedPnl: 0,
  realizedPnl: 0,
  contracts: 1,
  liquidationPrice: 40000,
  lastUpdate: Date.now(),
  ...overrides,
});

describe('formatPnl', () => {
  describe('without prefix', () => {
    it('should format positive pnl', () => {
      const result = formatPnl(1234.56);
      expect(result).toBe('1,234.56');
    });

    it('should format negative pnl', () => {
      const result = formatPnl(-1234.56);
      expect(result).toBe('-1,234.56');
    });

    it('should format zero pnl', () => {
      const result = formatPnl(0);
      expect(result).toBe('0.00');
    });

    it('should respect custom precision', () => {
      const result = formatPnl(1234.5678, 4);
      expect(result).toBe('1,234.5678');
    });

    it('should handle very small numbers', () => {
      const result = formatPnl(0.001, 3);
      expect(result).toBe('0.001');
    });

    it('should handle very large numbers', () => {
      const result = formatPnl(1234567890.12);
      expect(result).toBe('1,234,567,890.12');
    });
  });

  describe('with prefix', () => {
    it('should format positive pnl with prefix', () => {
      const result = formatPnl(1234.56, 2, '$');
      expect(result).toBe('$1,234.56');
    });

    it('should format negative pnl with prefix after minus sign', () => {
      const result = formatPnl(-1234.56, 2, '$');
      expect(result).toBe('-$1,234.56');
    });

    it('should format zero pnl with prefix (negative format)', () => {
      const result = formatPnl(0, 2, '$');
      expect(result).toBe('-$0.00');
    });

    it('should work with different currency prefixes', () => {
      expect(formatPnl(100, 2, 'EUR')).toBe('EUR100.00');
      expect(formatPnl(-100, 2, 'EUR')).toBe('-EUR100.00');
    });
  });
});

describe('filterAndSortOrdersInProfitDirection', () => {
  describe('for long positions', () => {
    const entryPrice = 50000;
    const positionSide = PositionSide.Long;

    it('should filter only open sell orders at or above entry price', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 52000, side: OrderSide.Sell, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 48000, side: OrderSide.Sell, status: OrderStatus.Open }), // below entry
        createOrder({ id: '3', price: 55000, side: OrderSide.Buy, status: OrderStatus.Open }), // wrong side
        createOrder({ id: '4', price: 53000, side: OrderSide.Sell, status: OrderStatus.Closed }), // closed
        createOrder({ id: '5', price: 50000, side: OrderSide.Sell, status: OrderStatus.Open }), // at entry
      ];

      const result = filterAndSortOrdersInProfitDirection(orders, entryPrice, positionSide);

      expect(result).toHaveLength(2);
      expect(result.map((o) => o.id)).toEqual(['5', '1']); // sorted low to high
    });

    it('should sort orders from lowest to highest price', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 55000, side: OrderSide.Sell, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 51000, side: OrderSide.Sell, status: OrderStatus.Open }),
        createOrder({ id: '3', price: 53000, side: OrderSide.Sell, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInProfitDirection(orders, entryPrice, positionSide);

      expect(result.map((o) => o.price)).toEqual([51000, 53000, 55000]);
    });

    it('should return empty array when no matching orders', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 48000, side: OrderSide.Sell, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 55000, side: OrderSide.Buy, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInProfitDirection(orders, entryPrice, positionSide);

      expect(result).toEqual([]);
    });

    it('should handle empty orders array', () => {
      const result = filterAndSortOrdersInProfitDirection([], entryPrice, positionSide);
      expect(result).toEqual([]);
    });
  });

  describe('for short positions', () => {
    const entryPrice = 50000;
    const positionSide = PositionSide.Short;

    it('should filter only open buy orders at or below entry price', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 48000, side: OrderSide.Buy, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 52000, side: OrderSide.Buy, status: OrderStatus.Open }), // above entry
        createOrder({ id: '3', price: 47000, side: OrderSide.Sell, status: OrderStatus.Open }), // wrong side
        createOrder({ id: '4', price: 45000, side: OrderSide.Buy, status: OrderStatus.Closed }), // closed
        createOrder({ id: '5', price: 50000, side: OrderSide.Buy, status: OrderStatus.Open }), // at entry
      ];

      const result = filterAndSortOrdersInProfitDirection(orders, entryPrice, positionSide);

      expect(result).toHaveLength(2);
      expect(result.map((o) => o.id)).toEqual(['5', '1']); // sorted high to low
    });

    it('should sort orders from highest to lowest price', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 45000, side: OrderSide.Buy, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 49000, side: OrderSide.Buy, status: OrderStatus.Open }),
        createOrder({ id: '3', price: 47000, side: OrderSide.Buy, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInProfitDirection(orders, entryPrice, positionSide);

      expect(result.map((o) => o.price)).toEqual([49000, 47000, 45000]);
    });
  });

  describe('edge cases', () => {
    it('should handle orders with same price', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 52000, side: OrderSide.Sell, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 52000, side: OrderSide.Sell, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInProfitDirection(orders, 50000, PositionSide.Long);

      expect(result).toHaveLength(2);
    });

    it('should filter out canceled orders', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 52000, side: OrderSide.Sell, status: OrderStatus.Canceled }),
      ];

      const result = filterAndSortOrdersInProfitDirection(orders, 50000, PositionSide.Long);

      expect(result).toEqual([]);
    });
  });
});

describe('filterAndSortOrdersInLossDirection', () => {
  describe('for long positions', () => {
    const entryPrice = 50000;
    const positionSide = PositionSide.Long;

    it('should filter sell orders or limit orders below entry price', () => {
      const orders: Order[] = [
        createOrder({
          id: '1',
          price: 48000,
          side: OrderSide.Sell,
          type: OrderType.StopLoss,
          status: OrderStatus.Open,
        }),
        createOrder({ id: '2', price: 47000, side: OrderSide.Buy, type: OrderType.Limit, status: OrderStatus.Open }),
        createOrder({ id: '3', price: 52000, side: OrderSide.Sell, status: OrderStatus.Open }), // above entry
        createOrder({ id: '4', price: 45000, side: OrderSide.Sell, status: OrderStatus.Closed }), // closed
      ];

      const result = filterAndSortOrdersInLossDirection(orders, entryPrice, positionSide);

      expect(result).toHaveLength(2);
      expect(result.map((o) => o.id)).toEqual(['1', '2']); // sorted high to low
    });

    it('should sort orders from highest to lowest price', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 45000, side: OrderSide.Sell, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 49000, side: OrderSide.Sell, status: OrderStatus.Open }),
        createOrder({ id: '3', price: 47000, side: OrderSide.Sell, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInLossDirection(orders, entryPrice, positionSide);

      expect(result.map((o) => o.price)).toEqual([49000, 47000, 45000]);
    });

    it('should include limit buy orders below entry (DCA orders)', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 48000, side: OrderSide.Buy, type: OrderType.Limit, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 46000, side: OrderSide.Buy, type: OrderType.Limit, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInLossDirection(orders, entryPrice, positionSide);

      expect(result).toHaveLength(2);
    });

    it('should return empty array when no matching orders', () => {
      const orders: Order[] = [createOrder({ id: '1', price: 52000, side: OrderSide.Sell, status: OrderStatus.Open })];

      const result = filterAndSortOrdersInLossDirection(orders, entryPrice, positionSide);

      expect(result).toEqual([]);
    });

    it('should handle empty orders array', () => {
      const result = filterAndSortOrdersInLossDirection([], entryPrice, positionSide);
      expect(result).toEqual([]);
    });
  });

  describe('for short positions', () => {
    const entryPrice = 50000;
    const positionSide = PositionSide.Short;

    it('should filter buy orders or limit orders above entry price', () => {
      const orders: Order[] = [
        createOrder({
          id: '1',
          price: 52000,
          side: OrderSide.Buy,
          type: OrderType.StopLoss,
          status: OrderStatus.Open,
        }),
        createOrder({ id: '2', price: 53000, side: OrderSide.Sell, type: OrderType.Limit, status: OrderStatus.Open }),
        createOrder({ id: '3', price: 48000, side: OrderSide.Buy, status: OrderStatus.Open }), // below entry
        createOrder({ id: '4', price: 55000, side: OrderSide.Buy, status: OrderStatus.Closed }), // closed
      ];

      const result = filterAndSortOrdersInLossDirection(orders, entryPrice, positionSide);

      expect(result).toHaveLength(2);
      expect(result.map((o) => o.id)).toEqual(['1', '2']); // sorted low to high
    });

    it('should sort orders from lowest to highest price', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 55000, side: OrderSide.Buy, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 51000, side: OrderSide.Buy, status: OrderStatus.Open }),
        createOrder({ id: '3', price: 53000, side: OrderSide.Buy, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInLossDirection(orders, entryPrice, positionSide);

      expect(result.map((o) => o.price)).toEqual([51000, 53000, 55000]);
    });

    it('should include limit sell orders above entry (DCA orders for shorts)', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 52000, side: OrderSide.Sell, type: OrderType.Limit, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 54000, side: OrderSide.Sell, type: OrderType.Limit, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInLossDirection(orders, entryPrice, positionSide);

      expect(result).toHaveLength(2);
    });
  });

  describe('edge cases', () => {
    it('should handle orders with same price', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 48000, side: OrderSide.Sell, status: OrderStatus.Open }),
        createOrder({ id: '2', price: 48000, side: OrderSide.Sell, status: OrderStatus.Open }),
      ];

      const result = filterAndSortOrdersInLossDirection(orders, 50000, PositionSide.Long);

      expect(result).toHaveLength(2);
    });

    it('should filter out creating/updating orders', () => {
      const orders: Order[] = [
        createOrder({ id: '1', price: 48000, side: OrderSide.Sell, status: OrderStatus.Creating }),
        createOrder({ id: '2', price: 47000, side: OrderSide.Sell, status: OrderStatus.Updating }),
      ];

      const result = filterAndSortOrdersInLossDirection(orders, 50000, PositionSide.Long);

      expect(result).toEqual([]);
    });

    it('should handle entry price of zero', () => {
      const orders: Order[] = [createOrder({ id: '1', price: 100, side: OrderSide.Sell, status: OrderStatus.Open })];

      // For long with entry 0, any price is above so this should be filtered out
      const result = filterAndSortOrdersInLossDirection(orders, 0, PositionSide.Long);

      expect(result).toEqual([]);
    });
  });
});

describe('calculateRiskAtPrice', () => {
  it('returns crosshair pnl in hedged mode when only one side is open', () => {
    const positions: Position[] = [
      createPosition({
        id: 'long-open',
        side: PositionSide.Long,
        contracts: 2,
        entryPrice: 50000,
        positionMode: PositionMode.Hedged,
      }),
      createPosition({
        id: 'short-flat',
        side: PositionSide.Short,
        contracts: 0,
        entryPrice: 51000,
        positionMode: PositionMode.Hedged,
      }),
    ];

    const result = calculateRiskAtPrice(52000, 'BTCUSDT', {}, positions, []);

    expect(result).not.toBeNull();
    expect(result).toMatchObject({
      positionSize: 2,
      positionSide: PositionSide.Long,
      unrealizedPnl: 4000,
      realizedPnl: 0,
    });
  });

  it('returns positive and negative projected pnl for an unopened limit buy', () => {
    const positions: Position[] = [
      createPosition({
        contracts: 0,
        entryPrice: 0,
      }),
    ];
    const orders: Order[] = [
      createOrder({
        id: 'entry-buy',
        side: OrderSide.Buy,
        price: 50000,
        remaining: 2,
        reduceOnly: false,
      }),
    ];

    expect(calculateRiskAtPrice(49000, 'BTCUSDT', {}, positions, orders)).toMatchObject({
      positionSize: 2,
      positionSide: PositionSide.Long,
      unrealizedPnl: -2000,
    });
    expect(calculateRiskAtPrice(51000, 'BTCUSDT', {}, positions, orders)).toMatchObject({
      positionSize: 2,
      positionSide: PositionSide.Long,
      unrealizedPnl: 2000,
    });
  });

  it('returns projected pnl for an unopened limit buy when the exchange has no position row', () => {
    const orders: Order[] = [
      createOrder({
        id: 'entry-buy',
        side: OrderSide.Buy,
        price: 50000,
        remaining: 2,
        reduceOnly: false,
      }),
    ];

    expect(calculateRiskAtPrice(51000, 'BTCUSDT', {}, [], orders)).toMatchObject({
      positionSize: 2,
      positionSide: PositionSide.Long,
      unrealizedPnl: 2000,
    });
  });

  it('returns positive and negative projected pnl for an unopened limit sell', () => {
    const positions: Position[] = [
      createPosition({
        contracts: 0,
        entryPrice: 0,
      }),
    ];
    const orders: Order[] = [
      createOrder({
        id: 'entry-sell',
        side: OrderSide.Sell,
        price: 50000,
        remaining: 2,
        reduceOnly: false,
      }),
    ];

    expect(calculateRiskAtPrice(51000, 'BTCUSDT', {}, positions, orders)).toMatchObject({
      positionSize: 2,
      positionSide: PositionSide.Short,
      unrealizedPnl: -2000,
    });
    expect(calculateRiskAtPrice(49000, 'BTCUSDT', {}, positions, orders)).toMatchObject({
      positionSize: 2,
      positionSide: PositionSide.Short,
      unrealizedPnl: 2000,
    });
  });

  it('returns projected pnl for an unopened limit sell when the exchange has no position row', () => {
    const orders: Order[] = [
      createOrder({
        id: 'entry-sell',
        side: OrderSide.Sell,
        price: 50000,
        remaining: 2,
        reduceOnly: false,
      }),
    ];

    expect(calculateRiskAtPrice(49000, 'BTCUSDT', {}, [], orders)).toMatchObject({
      positionSize: 2,
      positionSide: PositionSide.Short,
      unrealizedPnl: 2000,
    });
  });

  it('returns null in hedged mode when both sides are open', () => {
    const positions: Position[] = [
      createPosition({
        id: 'long-open',
        side: PositionSide.Long,
        contracts: 2,
        positionMode: PositionMode.Hedged,
      }),
      createPosition({
        id: 'short-open',
        side: PositionSide.Short,
        contracts: 1,
        positionMode: PositionMode.Hedged,
      }),
    ];

    const result = calculateRiskAtPrice(52000, 'BTCUSDT', {}, positions, []);

    expect(result).toBeNull();
  });
});
