import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { OrderBook } from '../generated/OrderBook';

export type FrameBook = { symbol: string; orderbook: OrderBook };
type BookLevels = Float64Array | Map<number, number>;
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

function levelMap(levels: BookLevels): Map<number, number> {
  if (levels instanceof Map) return new Map(levels);
  const result = new Map<number, number>();
  for (let i = 0; i < levels.length; i += 3) if (levels[i + 1] !== 0) result.set(levels[i], levels[i + 1]);
  return result;
}

function side(levels: BookLevels, descending: boolean): OrderBook['bids'] {
  if (levels instanceof Float64Array) {
    const result: OrderBook['bids'] = [];
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
  return [...levels]
    .sort(([a], [b]) => (descending ? b - a : a - b))
    .map(([price, amount]) => {
      total += amount;
      if (!Number.isFinite(total)) throw invalid();
      return { price, amount, total };
    });
}

/** Reconstruct format-1 books atomically, keeping baselines off transferable buffers. */
export class BookFrames {
  private states = new Map<string, BookState>();
  private readonly frames = new WeakMap<Frame, FrameBook[]>();
  private symbols?: readonly string[];
  private ids?: readonly string[];

  clear(): void {
    this.states.clear();
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
      const state = { id, bids: levelMap(previous.bids), asks: levelMap(previous.asks) };
      const updates: Array<[Map<number, number>, Float64Array]> = [
        [state.bids, book.bids],
        [state.asks, book.asks],
      ];
      for (const [levels, rows] of updates) {
        for (let i = 0; i < rows.length; i += 2) {
          const price = rows[i];
          const amount = rows[i + 1];
          if (!Number.isFinite(price) || !Number.isFinite(amount) || amount < 0) throw invalid();
          if (amount === 0) levels.delete(price);
          else levels.set(price, amount);
        }
      }
      states.set(symbol, state);
      changed.set(symbol, state);
    }
    const books = [...changed].map(([symbol, state]) => {
      return { symbol, orderbook: { bids: side(state.bids, true), asks: side(state.asks, false) } };
    });
    this.states = states;
    this.symbols = symbols;
    this.ids = ids;
    this.frames.set(frame, books);
    return books;
  }
}
