import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Candle } from '../generated/Candle';

export type FrameCandle = { symbol: string; interval: string; candle: Candle };

/** Decode six native columns verbatim; timestamp units remain milliseconds. */
export function readCandleFrame(frame: Frame, symbols: readonly string[]): FrameCandle[] {
  const rows = frame.candles();
  const candles: FrameCandle[] = [];
  if (!rows) return candles;
  for (let i = 0; i < rows.count; i++) {
    const symbol = symbols[rows.market[i]];
    if (!symbol) throw new Error('Candle identity requires a market snapshot');
    const at = i * 6;
    const values = rows.rows.subarray(at, at + 6);
    if (!values.every(Number.isFinite)) throw new Error('Invalid candle values');
    candles.push({
      symbol,
      interval: rows.interval(i),
      candle: {
        timestamp: values[0],
        open: values[1],
        high: values[2],
        low: values[3],
        close: values[4],
        volume: values[5],
      },
    });
  }
  return candles;
}
