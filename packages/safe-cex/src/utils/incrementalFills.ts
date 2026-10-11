export type IncrementalFill = { qty: number; value?: number };

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
    const qty = cumulativeQty - previous.qty;
    if (!(qty > 0)) return { qty: 0 };

    seen.delete(orderId);
    seen.set(orderId, { qty: cumulativeQty, value: cumulativeValue ?? previous.value });
    for (const id of seen.keys()) {
      if (seen.size <= maxOrders) break;
      seen.delete(id);
    }
    const value = cumulativeValue === undefined ? undefined : cumulativeValue - previous.value;
    return value !== undefined && value > 0 ? { qty, value } : { qty };
  };
};
