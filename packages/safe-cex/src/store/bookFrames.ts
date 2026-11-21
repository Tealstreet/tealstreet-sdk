import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { OrderBook } from '../generated/OrderBook';

export type FrameBook = { symbol: string; orderbook: OrderBook };
/** Wire `[price, amount, total]` rows from a full book, or nonzero levels sorted in book order. */
type SortedLevels = { prices: Float64Array; amounts: Float64Array };
type BookLevels = Float64Array | SortedLevels;
type BookState = { id: string; bids: BookLevels; asks: BookLevels };

const invalid = () => new Error('Order book snapshot required: invalid level or total');

function validateSnapshot(levels: Float64Array, descending: boolean): void {
  let previous = descending ? Infinity : -Infinity;
  for (let i = 0; i < levels.length; i += 3) {
    const price = levels[i];
    const amount = levels[i + 1];
    const total = levels[i + 2];
    if (!Number.isFinite(price) || !Number.isFinite(amount) || !Number.isFinite(total) || amount < 0 || total < 0) {
      throw invalid();
    }
    if (descending ? price >= previous : price <= previous) throw new Error('Order book levels must be sorted');
    previous = price;
  }
}

function sortedLevels(levels: BookLevels): SortedLevels {
  if (!(levels instanceof Float64Array)) return levels;
  const prices: number[] = [];
  const amounts: number[] = [];
  for (let i = 0; i < levels.length; i += 3) {
    if (levels[i + 1] === 0) continue;
    // A Map keyed these levels before; it stored -0 as 0.
    prices.push(levels[i] === 0 ? 0 : levels[i]);
    amounts.push(levels[i + 1]);
  }
  return { prices: Float64Array.from(prices), amounts: Float64Array.from(amounts) };
}

/**
 * Merge absolute-amount delta rows into sorted levels: O(levels + changes log changes)
 * instead of copying and re-sorting every level for each delta. A later row for the same
 * price wins, as the Map this replaced did; zero amounts delete.
 */
function mergeLevels(previous: SortedLevels, rows: Float64Array, descending: boolean): SortedLevels {
  const changes = rows.length / 2;
  if (changes === 0) return previous;
  const order: number[] = [];
  for (let i = 0; i < changes; i++) {
    const price = rows[i * 2];
    const amount = rows[i * 2 + 1];
    if (!Number.isFinite(price) || !Number.isFinite(amount) || amount < 0) throw invalid();
    order.push(i);
  }
  order.sort((a, b) => (descending ? rows[b * 2] - rows[a * 2] : rows[a * 2] - rows[b * 2]) || a - b);
  const before = (a: number, b: number) => (descending ? a > b : a < b);
  const { prices, amounts } = previous;
  const nextPrices = new Float64Array(prices.length + changes);
  const nextAmounts = new Float64Array(prices.length + changes);
  let count = 0;
  let level = 0;
  for (let next = 0; next < changes; next++) {
    const row = order[next];
    const price = rows[row * 2] === 0 ? 0 : rows[row * 2];
    // Equal prices sort by arrival; only the last one applies.
    if (next + 1 < changes && rows[order[next + 1] * 2] === price) continue;
    while (level < prices.length && before(prices[level], price)) {
      nextPrices[count] = prices[level];
      nextAmounts[count++] = amounts[level++];
    }
    if (level < prices.length && prices[level] === price) level++;
    const amount = rows[row * 2 + 1];
    if (amount !== 0) {
      nextPrices[count] = price;
      nextAmounts[count++] = amount;
    }
  }
  for (; level < prices.length; level++) {
    nextPrices[count] = prices[level];
    nextAmounts[count++] = amounts[level];
  }
  return { prices: nextPrices.subarray(0, count), amounts: nextAmounts.subarray(0, count) };
}

function side(levels: BookLevels): OrderBook['bids'] {
  const result: OrderBook['bids'] = [];
  if (levels instanceof Float64Array) {
    for (let i = 0; i < levels.length; i += 3) {
      const price = levels[i];
      const amount = levels[i + 1];
      const total = levels[i + 2];
      if (amount !== 0) result.push({ price, amount, total });
    }
    return result;
  }
  // BOOK_DELTA transmits absolute amounts without totals. Accumulation is part
  // of reconstructing that existing transport, not a quantity/unit conversion.
  let total = 0;
  for (let i = 0; i < levels.prices.length; i++) {
    const amount = levels.amounts[i];
    total += amount;
    if (!Number.isFinite(total)) throw invalid();
    result.push({ price: levels.prices[i], amount, total });
  }
  return result;
}

/** Reconstruct format-1 books atomically, keeping baselines off transferable buffers. */
export class BookFrames {
  private states = new Map<string, BookState>();
  private frames = new WeakMap<Frame, FrameBook[]>();
  private symbols?: readonly string[];
  private ids?: readonly string[];

  clear(): void {
    this.states.clear();
    this.frames = new WeakMap();
    this.symbols = undefined;
    this.ids = undefined;
  }

  read(frame: Frame, symbols: readonly string[], ids: readonly string[]): FrameBook[] {
    const cached = this.frames.get(frame);
    if (cached) return cached;
    const full = [...(frame.books?.() ?? [])];
    const deltas = [...(frame.bookDeltas?.() ?? [])];
    if (!full.length && !deltas.length && symbols === this.symbols && ids === this.ids) {
      const empty: FrameBook[] = [];
      this.frames.set(frame, empty);
      return empty;
    }
    const identities = new Map(symbols.map((symbol, i) => [symbol, ids[i]]));
    const states = new Map(this.states);
    for (const [symbol, state] of states) if (identities.get(symbol) !== state.id) states.delete(symbol);
    const changed = new Map<string, BookState>();
    const identity = (market: number) => {
      const symbol = symbols[market];
      const id = ids[market];
      if (!symbol || id === undefined) throw new Error('Order book snapshot required: unknown market');
      return { symbol, id };
    };
    for (const book of full) {
      const { symbol, id } = identity(book.market);
      validateSnapshot(book.bids, true);
      validateSnapshot(book.asks, false);
      const state = { id, bids: book.bids.slice(), asks: book.asks.slice() };
      states.set(symbol, state);
      changed.set(symbol, state);
    }
    for (const book of deltas) {
      const { symbol, id } = identity(book.market);
      const previous = states.get(symbol);
      if (!previous || previous.id !== id) throw new Error(`Order book snapshot required for ${symbol}`);
      const state = {
        id,
        bids: mergeLevels(sortedLevels(previous.bids), book.bids, true),
        asks: mergeLevels(sortedLevels(previous.asks), book.asks, false),
      };
      states.set(symbol, state);
      changed.set(symbol, state);
    }
    const books = [...changed].map(([symbol, state]) => {
      return { symbol, orderbook: { bids: side(state.bids), asks: side(state.asks) } };
    });
    this.states = states;
    this.symbols = symbols;
    this.ids = ids;
    this.frames.set(frame, books);
    return books;
  }
}
