/** A return percentage needs a known nonzero valuation; it never infers one from quantity or entry. */
export function calculatePositionPnlPercent(notional: number | null, unrealizedPnl: number): number | null {
  if (typeof notional !== 'number' || !Number.isFinite(notional) || notional === 0 || !Number.isFinite(unrealizedPnl))
    return null;
  const percent = (unrealizedPnl / Math.abs(notional)) * 100;
  return Number.isFinite(percent) ? percent : null;
}
