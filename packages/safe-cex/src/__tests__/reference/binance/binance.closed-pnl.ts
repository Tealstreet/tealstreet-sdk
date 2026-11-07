import type { ClosedPnlEntry } from '../../../types';

import { PositionSide } from '../../../types';

/**
 * Folds binance `/fapi/v1/userTrades` rows into closed positions.
 *
 * **This is a fold, but it is still an independent check on the ledger** — and those are
 * two different properties. The ledger reads `/fapi/v1/income`; this reads
 * `/fapi/v1/userTrades`. Different endpoints, so comparing them is a real cross-check,
 * unlike hyperliquid where both come from `userFillsByTime` and agreement is guaranteed.
 * See `utils/resolveClosedPositions.ts`.
 *
 * Row shape verified live (sheet row 2): `symbol, id, orderId, pair, side, price, qty,
 * realizedPnl, quoteQty, commission, commissionAsset, time, positionSide, baseQty,
 * marginAsset, buyer, maker`.
 *
 * **`realizedPnl` here is GROSS of fees** — `commission` is a separate field on the same
 * row. That is a third convention: bybit's `closedPnl` is net of fees, okx's `realizedPnl`
 * is net of fees AND funding. Anything comparing a closed-PnL figure to a ledger has to
 * know which of the three a venue uses, or it reports a residual the exact size of the
 * term it got wrong and blames the mapper.
 *
 * Only fills that actually closed something are emitted — a `realizedPnl` of 0 is an
 * opening fill and is not a closed position. `entryPrice` is not knowable from a fill, so
 * it is left 0 rather than approximated; see the note on the hyperliquid equivalent.
 */

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const deriveBinanceClosedPositions = (
  rows: Array<Record<string, any>>,
  accountName: string,
): ClosedPnlEntry[] => {
  const entries: ClosedPnlEntry[] = [];

  for (const row of rows) {
    const realizedPnl = toNumber(row.realizedPnl);
    if (realizedPnl === 0) continue;

    const time = Number(row.time ?? 0);
    const symbol = String(row.symbol ?? '');
    if (!time || !symbol) continue;

    const price = toNumber(row.price);
    const qty = toNumber(row.qty);

    entries.push({
      provenance: 'derived',
      symbol,
      notional: toNumber(row.quoteQty) || price * qty,
      contracts: qty,
      // Not knowable from a fill. Deliberately 0, not a guess — a plausible wrong entry
      // silently corrupts every return percentage downstream.
      entryPrice: 0,
      exitPrice: price,
      // Legacy field name; carries realized PnL. GROSS of the commission on this row.
      unrealizedPnl: realizedPnl,
      // A closing SELL means the position was long.
      side: String(row.side ?? '').toUpperCase() === 'SELL' ? PositionSide.Long : PositionSide.Short,
      fees: toNumber(row.commission),
      time,
      closedTime: time,
      account: accountName,
      leverage: 0,
    });
  }

  return entries;
};
