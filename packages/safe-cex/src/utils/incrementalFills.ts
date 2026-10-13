export type IncrementalFill = { qty: number; value?: number };

/**
 * Drops binary float noise from arithmetic on venue numbers (a cumulative
 * difference, value / qty): 83578.79999999999 -> 83578.8. Fifteen significant
 * digits is all a double holds exactly (DBL_DIG); the noise sits past it.
 */
export const cleanFloat = (n: number): number => (Number.isFinite(n) ? Number(n.toPrecision(15)) : n);

const decimalsOf = (n: number): number | undefined => {
  const text = String(n);
  if (text.includes('e')) return undefined;
  return (text.split('.')[1] ?? '').length;
};

/**
 * `a - b` for two venue decimals. A small fill off a large running total keeps
 * noise past fifteen digits (100000.1 - 100000 = 0.100000000005821), so round
 * to the inputs' own decimal places, which is exact for numbers parsed from
 * the venue's strings.
 */
const subtractDecimals = (a: number, b: number): number => {
  const da = decimalsOf(a);
  const db = decimalsOf(b);
  if (da === undefined || db === undefined) return cleanFloat(a - b);
  return Number((a - b).toFixed(Math.min(Math.max(da, db), 20)));
};

/**
 * Turns a venue's CUMULATIVE per-order fill (filled so far, and optionally the
 * traded value so far) into what THIS frame executed. A replayed or older frame
 * yields nothing. Starts empty, so after a restart the first frame of an order
 * that was already partly filled reports everything filled so far, once.
 */
export const createIncrementalFills = (maxOrders = 500) => {
  const seen = new Map<string, { qty: number; value: number }>();

  return (orderId: string, cumulativeQty: number, cumulativeValue?: number): IncrementalFill => {
    const previous = seen.get(orderId) ?? { qty: 0, value: 0 };
    const qty = subtractDecimals(cumulativeQty, previous.qty);
    if (!(qty > 0)) return { qty: 0 };

    seen.delete(orderId);
    seen.set(orderId, { qty: cumulativeQty, value: cumulativeValue ?? previous.value });
    for (const id of seen.keys()) {
      if (seen.size <= maxOrders) break;
      seen.delete(id);
    }
    const value = cumulativeValue === undefined ? undefined : subtractDecimals(cumulativeValue, previous.value);
    return value !== undefined && value > 0 ? { qty, value } : { qty };
  };
};
