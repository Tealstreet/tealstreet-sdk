import type { OrderBook, OrderBookOrders } from '../types';

import { describe, expect, it } from 'vitest';

import { calcOrderBookTotal, sortOrderBook } from './orderbook';

// Helper to create order book entries
const createOrder = (price: number, amount: number): OrderBookOrders => ({
  price,
  amount,
  total: 0,
});

describe('orderbook', () => {
  describe('sortOrderBook', () => {
    it('should sort asks in ascending order by price', () => {
      const orderBook: OrderBook = {
        asks: [createOrder(50000, 1), createOrder(49500, 2), createOrder(51000, 0.5)],
        bids: [],
      };

      sortOrderBook(orderBook);

      expect(orderBook.asks[0].price).toBe(49500);
      expect(orderBook.asks[1].price).toBe(50000);
      expect(orderBook.asks[2].price).toBe(51000);
    });

    it('should sort bids in descending order by price', () => {
      const orderBook: OrderBook = {
        asks: [],
        bids: [createOrder(49000, 1), createOrder(50000, 2), createOrder(48500, 0.5)],
      };

      sortOrderBook(orderBook);

      expect(orderBook.bids[0].price).toBe(50000);
      expect(orderBook.bids[1].price).toBe(49000);
      expect(orderBook.bids[2].price).toBe(48500);
    });

    it('should handle empty order book', () => {
      const orderBook: OrderBook = {
        asks: [],
        bids: [],
      };

      sortOrderBook(orderBook);

      expect(orderBook.asks).toEqual([]);
      expect(orderBook.bids).toEqual([]);
    });

    it('should handle single order on each side', () => {
      const orderBook: OrderBook = {
        asks: [createOrder(50000, 1)],
        bids: [createOrder(49000, 1)],
      };

      sortOrderBook(orderBook);

      expect(orderBook.asks.length).toBe(1);
      expect(orderBook.bids.length).toBe(1);
    });

    it('should sort both sides at once', () => {
      const orderBook: OrderBook = {
        asks: [createOrder(50100, 1), createOrder(50000, 1)],
        bids: [createOrder(49000, 1), createOrder(49500, 1)],
      };

      sortOrderBook(orderBook);

      expect(orderBook.asks[0].price).toBe(50000);
      expect(orderBook.asks[1].price).toBe(50100);
      expect(orderBook.bids[0].price).toBe(49500);
      expect(orderBook.bids[1].price).toBe(49000);
    });
  });

  describe('calcOrderBookTotal', () => {
    it('should calculate running totals for asks', () => {
      const orderBook: OrderBook = {
        asks: [createOrder(50000, 1), createOrder(50100, 2), createOrder(50200, 1.5)],
        bids: [],
      };

      calcOrderBookTotal(orderBook);

      expect(orderBook.asks[0].total).toBe(1);
      expect(orderBook.asks[1].total).toBe(3); // 1 + 2
      expect(orderBook.asks[2].total).toBe(4.5); // 3 + 1.5
    });

    it('should calculate running totals for bids', () => {
      const orderBook: OrderBook = {
        asks: [],
        bids: [createOrder(49500, 0.5), createOrder(49000, 1), createOrder(48500, 2)],
      };

      calcOrderBookTotal(orderBook);

      expect(orderBook.bids[0].total).toBe(0.5);
      expect(orderBook.bids[1].total).toBe(1.5); // 0.5 + 1
      expect(orderBook.bids[2].total).toBe(3.5); // 1.5 + 2
    });

    it('should handle empty order book', () => {
      const orderBook: OrderBook = {
        asks: [],
        bids: [],
      };

      calcOrderBookTotal(orderBook);

      expect(orderBook.asks).toEqual([]);
      expect(orderBook.bids).toEqual([]);
    });

    it('should handle single order', () => {
      const orderBook: OrderBook = {
        asks: [createOrder(50000, 1.5)],
        bids: [createOrder(49000, 2.5)],
      };

      calcOrderBookTotal(orderBook);

      expect(orderBook.asks[0].total).toBe(1.5);
      expect(orderBook.bids[0].total).toBe(2.5);
    });

    it('should accumulate fractional amounts via plain addition', () => {
      const orderBook: OrderBook = {
        asks: [createOrder(50000, 0.1), createOrder(50100, 0.2)],
        bids: [],
      };

      calcOrderBookTotal(orderBook);

      // `total` is consumed only by depth-chart / orderbook bar-fill rendering
      // and `useSmoothOrderbook` interpolation — relative magnitude matters,
      // not exact decimal precision. Float drift is invisible to consumers.
      expect(orderBook.asks[1].total).toBeCloseTo(0.3, 10);
    });

    it('should calculate totals for both sides independently', () => {
      const orderBook: OrderBook = {
        asks: [createOrder(50000, 1), createOrder(50100, 1)],
        bids: [createOrder(49500, 2), createOrder(49000, 3)],
      };

      calcOrderBookTotal(orderBook);

      expect(orderBook.asks[0].total).toBe(1);
      expect(orderBook.asks[1].total).toBe(2);
      expect(orderBook.bids[0].total).toBe(2);
      expect(orderBook.bids[1].total).toBe(5);
    });
  });
});
