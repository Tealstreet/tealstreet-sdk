import type { OrderBook, OrderBookOrders } from '../types';

import { beforeEach, describe, expect, it } from 'vitest';

import { updateOrderBook, updateOrderBookByPrice } from './updateOrderBook';

// Helper to create order book entries
const createOrder = (price: number, amount: number): OrderBookOrders => ({
  price,
  amount,
  total: 0,
});

describe('updateOrderBook', () => {
  let orderBook: OrderBook;

  beforeEach(() => {
    // Create a sorted order book
    // Asks: ascending (low to high)
    // Bids: descending (high to low)
    orderBook = {
      asks: [createOrder(50000, 1), createOrder(50100, 2), createOrder(50200, 1.5)],
      bids: [createOrder(49900, 1), createOrder(49800, 2), createOrder(49700, 0.5)],
    };
  });

  describe('asks (ascending order)', () => {
    it('should update existing ask level', () => {
      const result = updateOrderBook(orderBook, 'asks', createOrder(50100, 5));

      expect(result).toBe(1); // Found at index 1
      expect(orderBook.asks[1].amount).toBe(5);
      expect(orderBook.asks.length).toBe(3); // No new entries
    });

    it('should remove ask level when amount is 0', () => {
      const result = updateOrderBook(orderBook, 'asks', createOrder(50100, 0));

      expect(result).toBe(-1); // Removed
      expect(orderBook.asks.length).toBe(2);
      expect(orderBook.asks.find((o) => o.price === 50100)).toBeUndefined();
    });

    it('should insert new ask level in sorted position', () => {
      const result = updateOrderBook(orderBook, 'asks', createOrder(50050, 3));

      expect(result).toBe(1); // Inserted at index 1 (between 50000 and 50100)
      expect(orderBook.asks.length).toBe(4);
      expect(orderBook.asks[0].price).toBe(50000);
      expect(orderBook.asks[1].price).toBe(50050);
      expect(orderBook.asks[2].price).toBe(50100);
      expect(orderBook.asks[1].amount).toBe(3);
    });

    it('should not insert when amount is 0 and level does not exist', () => {
      const result = updateOrderBook(orderBook, 'asks', createOrder(50050, 0));

      expect(result).toBe(-1);
      expect(orderBook.asks.length).toBe(3); // No change
    });

    it('should not insert when upsert is false and level does not exist', () => {
      const result = updateOrderBook(
        orderBook,
        'asks',
        createOrder(50050, 3),
        false, // upsert = false
      );

      expect(result).toBe(-1);
      expect(orderBook.asks.length).toBe(3); // No change
    });

    it('should update first ask level', () => {
      const result = updateOrderBook(orderBook, 'asks', createOrder(50000, 10));

      expect(result).toBe(0);
      expect(orderBook.asks[0].amount).toBe(10);
    });

    it('should update last ask level', () => {
      const result = updateOrderBook(orderBook, 'asks', createOrder(50200, 10));

      expect(result).toBe(2);
      expect(orderBook.asks[2].amount).toBe(10);
    });
  });

  describe('bids (descending order)', () => {
    it('should update existing bid level', () => {
      const result = updateOrderBook(orderBook, 'bids', createOrder(49800, 5));

      expect(result).toBe(1); // Found at index 1
      expect(orderBook.bids[1].amount).toBe(5);
      expect(orderBook.bids.length).toBe(3);
    });

    it('should remove bid level when amount is 0', () => {
      const result = updateOrderBook(orderBook, 'bids', createOrder(49800, 0));

      expect(result).toBe(-1); // Removed
      expect(orderBook.bids.length).toBe(2);
      expect(orderBook.bids.find((o) => o.price === 49800)).toBeUndefined();
    });

    it('should insert new bid level in sorted position', () => {
      const result = updateOrderBook(orderBook, 'bids', createOrder(49850, 3));

      expect(result).toBe(1); // Inserted at index 1 (between 49900 and 49800)
      expect(orderBook.bids.length).toBe(4);
      expect(orderBook.bids[0].price).toBe(49900);
      expect(orderBook.bids[1].price).toBe(49850);
      expect(orderBook.bids[2].price).toBe(49800);
      expect(orderBook.bids[1].amount).toBe(3);
    });

    it('should update first bid level (highest price)', () => {
      const result = updateOrderBook(orderBook, 'bids', createOrder(49900, 10));

      expect(result).toBe(0);
      expect(orderBook.bids[0].amount).toBe(10);
    });

    it('should update last bid level (lowest price)', () => {
      const result = updateOrderBook(orderBook, 'bids', createOrder(49700, 10));

      expect(result).toBe(2);
      expect(orderBook.bids[2].amount).toBe(10);
    });
  });

  describe('edge cases', () => {
    it('should handle empty asks', () => {
      orderBook.asks = [];

      const result = updateOrderBook(orderBook, 'asks', createOrder(50000, 1));

      expect(result).toBe(0); // Inserted at index 0
      expect(orderBook.asks.length).toBe(1);
      expect(orderBook.asks[0].price).toBe(50000);
    });

    it('should handle empty bids', () => {
      orderBook.bids = [];

      const result = updateOrderBook(orderBook, 'bids', createOrder(49000, 1));

      expect(result).toBe(0); // Inserted at index 0
      expect(orderBook.bids.length).toBe(1);
      expect(orderBook.bids[0].price).toBe(49000);
    });

    it('should handle single ask level', () => {
      orderBook.asks = [createOrder(50000, 1)];

      // Update existing
      updateOrderBook(orderBook, 'asks', createOrder(50000, 5));
      expect(orderBook.asks[0].amount).toBe(5);

      // Remove it
      updateOrderBook(orderBook, 'asks', createOrder(50000, 0));
      expect(orderBook.asks.length).toBe(0);
    });

    it('should handle single bid level', () => {
      orderBook.bids = [createOrder(49000, 1)];

      // Update existing
      updateOrderBook(orderBook, 'bids', createOrder(49000, 5));
      expect(orderBook.bids[0].amount).toBe(5);

      // Remove it
      updateOrderBook(orderBook, 'bids', createOrder(49000, 0));
      expect(orderBook.bids.length).toBe(0);
    });

    it('should handle very small decimal amounts', () => {
      const result = updateOrderBook(orderBook, 'asks', createOrder(50100, 0.00001));

      expect(result).toBe(1);
      expect(orderBook.asks[1].amount).toBe(0.00001);
    });

    it('should handle large order book', () => {
      // Create a large order book with 1000 levels
      orderBook.asks = Array.from({ length: 1000 }, (_, i) => createOrder(50000 + i, 1));
      orderBook.bids = Array.from({ length: 1000 }, (_, i) => createOrder(49999 - i, 1));

      // Find middle ask
      const resultAsk = updateOrderBook(orderBook, 'asks', createOrder(50500, 5));
      expect(resultAsk).toBe(500);
      expect(orderBook.asks[500].amount).toBe(5);

      // Find middle bid
      const resultBid = updateOrderBook(orderBook, 'bids', createOrder(49499, 5));
      expect(resultBid).toBe(500);
      expect(orderBook.bids[500].amount).toBe(5);
    });
  });

  describe('sorted insertion', () => {
    it('should maintain ask sort order after multiple inserts', () => {
      // Insert in random order
      updateOrderBook(orderBook, 'asks', createOrder(50300, 1));
      updateOrderBook(orderBook, 'asks', createOrder(49900, 2));
      updateOrderBook(orderBook, 'asks', createOrder(50150, 3));

      // Verify ascending order is maintained
      for (let i = 1; i < orderBook.asks.length; i++) {
        expect(orderBook.asks[i].price).toBeGreaterThan(orderBook.asks[i - 1].price);
      }
      expect(orderBook.asks.length).toBe(6);
    });

    it('should maintain bid sort order after multiple inserts', () => {
      // Insert in random order
      updateOrderBook(orderBook, 'bids', createOrder(49600, 1));
      updateOrderBook(orderBook, 'bids', createOrder(50000, 2));
      updateOrderBook(orderBook, 'bids', createOrder(49750, 3));

      // Verify descending order is maintained
      for (let i = 1; i < orderBook.bids.length; i++) {
        expect(orderBook.bids[i].price).toBeLessThan(orderBook.bids[i - 1].price);
      }
      expect(orderBook.bids.length).toBe(6);
    });

    it('should handle insert-then-delete within same delta correctly', () => {
      // Simulate a delta with insert then delete of the same price
      updateOrderBook(orderBook, 'asks', createOrder(50050, 5));
      expect(orderBook.asks.length).toBe(4);

      updateOrderBook(orderBook, 'asks', createOrder(50050, 0));
      expect(orderBook.asks.length).toBe(3);

      // Original levels should be intact
      expect(orderBook.asks[0].price).toBe(50000);
      expect(orderBook.asks[1].price).toBe(50100);
      expect(orderBook.asks[2].price).toBe(50200);
    });
  });
});

describe('updateOrderBookByPrice', () => {
  let orderBook: OrderBook;

  beforeEach(() => {
    orderBook = {
      asks: [createOrder(50000, 1), createOrder(50100, 2), createOrder(50200, 1.5)],
      bids: [createOrder(49900, 1), createOrder(49800, 2), createOrder(49700, 0.5)],
    };
  });

  it('should update existing ask level without allocating a new entry', () => {
    const before = orderBook.asks[1];
    const result = updateOrderBookByPrice(orderBook, 'asks', 50100, 5);
    expect(result).toBe(1);
    expect(orderBook.asks[1].amount).toBe(5);
    expect(orderBook.asks[1]).toBe(before); // same reference, in-place mutation
    expect(orderBook.asks).toHaveLength(3);
  });

  it('should remove existing level when amount is 0', () => {
    const result = updateOrderBookByPrice(orderBook, 'asks', 50100, 0);
    expect(result).toBe(-1);
    expect(orderBook.asks).toHaveLength(2);
    expect(orderBook.asks[0].price).toBe(50000);
    expect(orderBook.asks[1].price).toBe(50200);
  });

  it('should insert a new ask level at the correct sorted position', () => {
    const result = updateOrderBookByPrice(orderBook, 'asks', 50050, 3);
    expect(result).toBe(1);
    expect(orderBook.asks).toHaveLength(4);
    expect(orderBook.asks[0].price).toBe(50000);
    expect(orderBook.asks[1].price).toBe(50050);
    expect(orderBook.asks[1].amount).toBe(3);
    expect(orderBook.asks[1].total).toBe(0);
  });

  it('should insert a new bid level (descending) at the correct sorted position', () => {
    const result = updateOrderBookByPrice(orderBook, 'bids', 49850, 7);
    expect(result).toBe(1);
    expect(orderBook.bids).toHaveLength(4);
    expect(orderBook.bids[0].price).toBe(49900);
    expect(orderBook.bids[1].price).toBe(49850);
    expect(orderBook.bids[1].amount).toBe(7);
    expect(orderBook.bids[2].price).toBe(49800);
  });

  it('should skip insertion when upsert=false and price not found', () => {
    const result = updateOrderBookByPrice(orderBook, 'asks', 50050, 3, false);
    expect(result).toBe(-1);
    expect(orderBook.asks).toHaveLength(3);
  });

  it('should skip when amount=0 and price not found', () => {
    const result = updateOrderBookByPrice(orderBook, 'asks', 50050, 0);
    expect(result).toBe(-1);
    expect(orderBook.asks).toHaveLength(3);
  });
});
