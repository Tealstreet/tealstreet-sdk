import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { FrameBook } from './bookFrames';
import type { FrameCandle } from './candleFrames';

type Section = 'frame' | 'books' | 'candles' | 'fills';
type CandleStream = Pick<FrameCandle, 'symbol' | 'interval'>;
export type WorkerFrameRejection = {
  error: unknown;
  books?: readonly string[];
  candles?: readonly CandleStream[];
};
const candleKey = ({ symbol, interval }: CandleStream) => JSON.stringify([symbol, interval]);

/** Keep failed stream identities until matching data proves their recovery. */
export class WorkerFrameRecoveryScopes {
  private readonly sections = new Set<Section>();
  private readonly books = new Set<string>();
  private readonly candles = new Set<string>();

  get size() {
    return this.sections.size + this.books.size + this.candles.size;
  }

  clear() {
    this.sections.clear();
    this.books.clear();
    this.candles.clear();
  }

  reject(section: Section, error: unknown, frame?: Frame, symbols: readonly string[] = []): WorkerFrameRejection {
    try {
      if (section === 'books' && frame) {
        const books = [...(frame.books?.() ?? []), ...(frame.bookDeltas?.() ?? [])].map(({ market }) => {
          const symbol = symbols[market];
          if (!symbol) throw new Error('Unknown rejected book identity');
          return symbol;
        });
        if (books.length) {
          for (const symbol of books) this.books.add(symbol);
          return { error, books };
        }
      }
      if (section === 'candles' && frame) {
        const rows = frame.candles();
        const candles: CandleStream[] = [];
        if (rows) {
          for (let i = 0; i < rows.count; i++) {
            const symbol = symbols[rows.market[i]];
            if (!symbol) throw new Error('Unknown rejected candle identity');
            candles.push({ symbol, interval: rows.interval(i) });
          }
        }
        if (candles.length) {
          for (const stream of candles) this.candles.add(candleKey(stream));
          return { error, candles };
        }
      }
    } catch {
      // Unidentifiable stream failures retain their cooldown until the mirror resets.
    }
    this.sections.add(section);
    return { error };
  }

  acceptMarketSnapshot() {
    this.sections.delete('frame');
  }

  acceptBooks(rows: readonly FrameBook[]) {
    for (const row of rows) this.books.delete(row.symbol);
  }

  acceptCandles(rows: readonly FrameCandle[]) {
    for (const row of rows) this.candles.delete(candleKey(row));
  }

  acceptFills(nonempty: boolean) {
    if (nonempty) this.sections.delete('fills');
  }
}
