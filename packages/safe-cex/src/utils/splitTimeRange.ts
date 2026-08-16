/**
 * Split a time range into windows no wider than the venue accepts.
 *
 * WHY THIS EXISTS AS A SHARED HELPER. Several venues cap a history query's time span, and
 * the two failure modes look nothing alike:
 *
 * - **Reject** (binance `/fapi/v1/userTrades`, bybit `/v5/order/history`): an over-wide
 *   range is an ERROR, not a clamp. Every one of these call sites wraps the request in a
 *   try/catch that returns `[]`, so the request fails, the error is swallowed, and the
 *   caller reads "this account has no history" — a data finding — instead of a failed
 *   request. Nothing surfaces. A 30-day ask returns ZERO rows, not 7 days' worth.
 * - **Clamp/ignore** (okx `positions-history`, blofin `fills-history`): fewer rows than
 *   asked for, which at least still looks like data.
 *
 * The reject case has now been found three times in this repo — bybit `fetchOrderHistory`
 * (hedge legs silently degraded to `both`), bybit `fetchIncomeHistory`, and binance
 * `fetchExecutions`/`fetchHistoricalTrades` — each time as an independent inline loop or a
 * missing one. Binance already carried a correct copy inside `deriveClosedPositions` while
 * the two methods beside it had none.
 *
 * Windows are ASCENDING, inclusive at both ends, and never share a boundary millisecond:
 * consecutive windows advance past it. Sharing it returns the boundary row twice, and
 * deduping that away is not a fix — bybit's `tranId` is not unique (hedge-mode settlement
 * rows share every identity component), so a Map keyed on it deletes real value.
 */
export const splitTimeRange = (
  from: number,
  to: number,
  maxSpanMs: number,
): Array<{ startTime: number; endTime: number }> => {
  if (!Number.isFinite(from) || !Number.isFinite(to) || to < from) return [];
  // A non-positive cap would loop forever. Treat it as "no cap" rather than hanging.
  if (!Number.isFinite(maxSpanMs) || maxSpanMs <= 0) return [{ startTime: from, endTime: to }];

  const windows: Array<{ startTime: number; endTime: number }> = [];
  let start = from;

  while (start <= to) {
    const end = Math.min(start + maxSpanMs, to);
    windows.push({ startTime: start, endTime: end });
    if (end >= to) break;
    start = end + 1;
  }

  return windows;
};
