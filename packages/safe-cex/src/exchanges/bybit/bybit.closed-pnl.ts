import type { ClosedPnlEntry } from '../../types';

import { PositionSide } from '../../types';

/**
 * Bybit `/v5/position/closed-pnl` row -> `ClosedPnlEntry`.
 *
 * Pure and exported so it can be tested — the exchange method is an instance field and
 * is unreachable from a test.
 *
 * `ClosedPnlEntry` is typed loosely (`string | number` on every numeric) and the sole
 * consumer, `apps/web/src/components/openPositions/ClosedPositions.tsx`, casts the result
 * `as unknown[] as ClosedPosition[]`. That cast means TypeScript cannot catch ANY shape
 * mistake here, so the runtime contract has to be honoured deliberately:
 *
 * - **`side` is the CLOSING side and inverts.** Bybit reports the side of the closing
 *   order, so a `Sell` close means the position was Long. Getting this backwards flips
 *   every row in the UI and still typechecks.
 * - **Numbers, not strings.** Bybit returns every numeric as a decimal string. The grid
 *   sorts with `a.notional - b.notional` and `calculateRelativeReturns` divides by
 *   `entryPrice` and multiplies `leverage`, so a passed-through string yields NaN.
 * - **`account` must be populated** — the Account column and `SymbolDisplay` read it, and
 *   it is the one thing `opts.accountName` is still needed for now that this calls the
 *   venue directly rather than our own API.
 * - **`fees` is real.** The dead route this replaces hardcoded `fees: 0`; bybit reports
 *   `openFee` and `closeFee` per row.
 *
 * Note `closedPnl` is NET of open and close fees. It is therefore NOT directly comparable
 * to the ledger's `realized_pnl` category, which comes from `cashFlow` with fees emitted
 * as separate records — reconciling the two means adding the fee term back.
 */

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

const toTimestamp = (value: unknown) => {
  const parsed = parseInt(String(value ?? '0'), 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
};

export const mapBybitClosedPnl = (
  row: Record<string, any>,
  accountName: string,
  /**
   * Inverse contracts are quoted in USD and settled in coin, so the two size fields swap
   * meaning: `closedSize` is USD contracts and `cumExitValue` is coin. `mapPosition` on
   * `BybitInverseExchange` stores `contracts` in coin and `notional` in USD, and the
   * Closed Positions grid renders both alongside open positions — so a raw passthrough
   * shows the same trade as `1000 BTC` here and `0.01 BTC` there, and sorts inverse rows
   * on a coin scale against linear rows on a USD scale.
   */
  isInverse = false,
): ClosedPnlEntry | null => {
  const symbol = String(row.symbol || '');
  const createdTime = toTimestamp(row.createdTime);
  if (!symbol || !createdTime) {
    return null;
  }

  const openFee = toNumber(row.openFee);
  const closeFee = toNumber(row.closeFee);

  const closedSize = toNumber(row.closedSize);
  const exitPrice = toNumber(row.avgExitPrice);
  // Match mapPosition's inverse convention: contracts in coin, notional in USD.
  const contracts = isInverse && exitPrice > 0 ? Math.round((closedSize / exitPrice) * 1e8) / 1e8 : closedSize;
  const notional = isInverse ? closedSize : toNumber(row.cumExitValue);

  return {
    // The venue reported this directly, so it can serve as an independent check on the
    // ledger axis — see utils/resolveClosedPositions.ts.
    provenance: 'venue',
    symbol,
    notional,
    contracts,
    entryPrice: toNumber(row.avgEntryPrice),
    exitPrice: exitPrice,
    // The field is named `unrealizedPnl` on the shared type but carries realized PnL for
    // a closed position; renaming it would break the UI's `ClosedPosition` shape.
    unrealizedPnl: toNumber(row.closedPnl),
    side: row.side === 'Sell' ? PositionSide.Long : PositionSide.Short,
    fees: openFee + closeFee,
    time: createdTime,
    closedTime: toTimestamp(row.updatedTime) || createdTime,
    account: accountName,
    leverage: toNumber(row.leverage),
  };
};
