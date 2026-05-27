import { OrderBook } from '../types';

/**
 * Binary search to find index of a price level in the orderbook.
 * Returns the index if found, or -(insertionPoint + 1) if not found,
 * where insertionPoint is the index at which the price would be inserted
 * to maintain sorted order.
 *
 * @param levels - Array of orderbook levels (must be sorted)
 * @param price - Price to search for
 * @param isDescending - true for bids (descending), false for asks (ascending)
 */
const binarySearchPrice = (levels: OrderBook['asks'], price: number, isDescending: boolean): number => {
  let left = 0;
  let right = levels.length - 1;

  while (left <= right) {
    const mid = (left + right) >>> 1; // Unsigned right shift for integer division
    const midPrice = levels[mid].price;

    if (midPrice === price) {
      return mid;
    }

    if (isDescending) {
      // Bids: sorted high to low
      if (midPrice > price) {
        left = mid + 1;
      } else {
        right = mid - 1;
      }
    } else {
      // Asks: sorted low to high
      if (midPrice < price) {
        left = mid + 1;
      } else {
        right = mid - 1;
      }
    }
  }

  return -(left + 1);
};

/**
 * upserts orderbook level in place, maintaining sorted order.
 * returns index of update or -1 if removed/skipped
 * @param orderBook
 * @param side
 * @param update
 * @param upsert
 */
export const updateOrderBook = (
  orderBook: OrderBook,
  side: 'bids' | 'asks',
  update: OrderBook['asks'][0],
  upsert = true,
): number => {
  const levels = orderBook[side];
  const isDescending = side === 'bids';
  const index = binarySearchPrice(levels, update.price, isDescending);

  if (index < 0) {
    // Not found
    if (!upsert) return -1;
    if (update.amount === 0) return -1;

    // Insert at correct sorted position
    const insertAt = -(index + 1) || 0;
    levels.splice(insertAt, 0, update);
    return insertAt;
  } else {
    // Found
    if (update.amount === 0) {
      levels.splice(index, 1);
      return -1;
    } else {
      levels[index].amount = update.amount;
      return index;
    }
  }
};

/**
 * Primitive-arg variant of `updateOrderBook`. Use this on hot orderbook
 * delta paths (Bybit V5, etc.) where ~95% of incoming levels are
 * existing-level mutations or removals — the prior shape allocated a
 * `{ price, amount, total: 0 }` wrapper per delta level just to read
 * `.price` for the binary search and `.amount` for the dispatch, then
 * discarded it. Here the wrapper is only allocated on the insertion
 * path, where it actually persists in the book.
 */
export const updateOrderBookByPrice = (
  orderBook: OrderBook,
  side: 'bids' | 'asks',
  price: number,
  amount: number,
  upsert = true,
): number => {
  const levels = orderBook[side];
  const isDescending = side === 'bids';
  const index = binarySearchPrice(levels, price, isDescending);

  if (index < 0) {
    if (!upsert) return -1;
    if (amount === 0) return -1;

    const insertAt = -(index + 1) || 0;
    levels.splice(insertAt, 0, { price, amount, total: 0 });
    return insertAt;
  } else {
    if (amount === 0) {
      levels.splice(index, 1);
      return -1;
    } else {
      levels[index].amount = amount;
      return index;
    }
  }
};
