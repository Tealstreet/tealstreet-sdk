import { PositionSide } from '../types';

/**
 * Venues spell one-way (netted) mode a dozen different ways, and consumers index
 * a `{ both, long, short }` map directly with this value — an unmapped string
 * yields `undefined` and crashes on the first fill. Anything not recognised is
 * one-way, which is the safe default: it is the mode every venue uses unless
 * hedging was explicitly enabled.
 *
 * This is the POSITION leg, not the order side. `"Close Long"` belongs to the
 * long leg even though the fill itself sells, so order-side words (`buy`/`sell`)
 * are deliberately NOT accepted — a venue that reports only order side is
 * telling us nothing about the leg, and must fall through to `'both'`.
 */
export const normalizeExecutionPositionSide = (raw: unknown): PositionSide | 'both' => {
  const value = `${raw ?? ''}`.trim().toLowerCase();

  if (value === 'long' || value === 'open long' || value === 'close long') {
    return PositionSide.Long;
  }

  if (value === 'short' || value === 'open short' || value === 'close short') {
    return PositionSide.Short;
  }

  return 'both';
};
