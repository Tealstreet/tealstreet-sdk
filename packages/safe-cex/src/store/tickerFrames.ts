import type { Frame } from '@tealstreet/safe-cex-wasm/wire';
import type { Ticker } from '../generated/Ticker';
import type { Market } from '../types';
import type { PackedTickerDelta, StoreUpdate } from './store.interface';

import { TICKER_FIELDS } from '@tealstreet/safe-cex-wasm/wire';

/** Decode native columns; NaN is the packed absence marker only for midpoint. */
export function readTickerFrame(
  table: ReturnType<Frame['tickers']>,
  marketAt: (index: number) => Pick<Market, 'id' | 'symbol'> | undefined,
): Ticker[] | undefined {
  if (!table) return undefined;
  const rows: Ticker[] = [];
  for (let i = 0; i < table.count; i++) {
    const market = marketAt(table.market[i]);
    const identity = market ?? table.identity?.(i);
    if (!identity) throw new Error('Missing native ticker identity');
    const midPrice = table.midPrice?.[i];
    rows.push({
      id: identity.id,
      symbol: identity.symbol,
      bid: table.bid[i],
      ask: table.ask[i],
      last: table.last[i],
      mark: table.mark[i],
      index: table.index[i],
      ...(midPrice !== undefined && !Number.isNaN(midPrice) && { midPrice }),
      percentage: table.percentage[i],
      openInterest: table.openInterest[i],
      fundingRate: table.fundingRate[i],
      fundingTime: table.fundingTime[i],
      volume: table.volume[i],
      quoteVolume: table.quoteVolume[i],
    });
  }
  return rows;
}

export function readTickerDeltaFrame(frame: Frame, count: number, hasSnapshot = false): PackedTickerDelta[] | undefined {
  const deltas = frame.tickerDeltas();
  if (!deltas || hasSnapshot) return undefined;
  const packed: PackedTickerDelta[] = [];
  for (let i = 0; i < deltas.length;) {
    const row = deltas[i];
    const mask = deltas[i + 1];
    if (!Number.isInteger(row) || row < 0 || row >= count || !Number.isInteger(mask) || mask <= 0 || mask >= 1 << TICKER_FIELDS.length) {
      throw new Error('Invalid ticker delta row or mask');
    }
    let width = 0;
    for (let field = 0; field < TICKER_FIELDS.length; field++) if (mask & (1 << field)) width++;
    if (i + 2 + width > deltas.length) throw new Error('Truncated ticker delta');
    packed.push([row, mask, ...deltas.subarray(i + 2, i + 2 + width)]);
    i += 2 + width;
  }
  return packed;
}

export function readTickerObservationFrame(frame: Frame, rows: readonly Ticker[]): StoreUpdate['tickerObservations'] {
  const observations = frame.tickerObservations();
  if (!observations) return undefined;
  const times: NonNullable<StoreUpdate['tickerObservations']> = [];
  for (let i = 0; i < observations.length; i += 4) {
    const row = observations[i];
    if (!rows[row]) throw new Error('Invalid ticker observation row');
    times.push([row, [
      Number.isFinite(observations[i + 1]) ? observations[i + 1] : null,
      Number.isFinite(observations[i + 2]) ? observations[i + 2] : null,
      Number.isFinite(observations[i + 3]) ? observations[i + 3] : null,
    ]]);
  }
  return times;
}
