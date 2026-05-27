import type { OrderBook } from '../types';

export const sortOrderBook = (orderBook: OrderBook) => {
  orderBook.asks.sort((a, b) => a.price - b.price);
  orderBook.bids.sort((a, b) => b.price - a.price);
};

// Hot path: runs on every orderbook frame across ~20 exchanges (30–60 Hz/symbol)
// over 1000-level books. Indexed for-loops with plain `+` over the prior
// `Object.values + forEach + safe-math.add` shape: drops the entries array,
// 4 closure frames per call, and ~2000 `safe-math.add` invocations per frame.
// Decimal-aware accumulation isn't needed — the `total` field is consumed by
// `Orderbook.tsx` / `useSmoothOrderbook` / depth chart for relative bar-fill
// rendering only.
export const calcOrderBookTotal = (orderBook: OrderBook) => {
  const { bids, asks } = orderBook;

  let bidTotal = 0;
  for (let i = 0; i < bids.length; i++) {
    bidTotal += bids[i].amount;
    bids[i].total = bidTotal;
  }

  let askTotal = 0;
  for (let i = 0; i < asks.length; i++) {
    askTotal += asks[i].amount;
    asks[i].total = askTotal;
  }
};
