import type { ClosedPnlEntry, Market } from '../../types';

import { PositionSide } from '../../types';

/**
 * OKX `/api/v5/account/positions-history` row -> `ClosedPnlEntry`.
 *
 * Field shape verified against a live account (sheet row 13):
 * `cTime, ccy, closeAvgPx, closeTotalPos, direction, fee, fundingFee, instId, instType,
 *  lever, liqPenalty, mgnMode, nonSettleAvgPx, openAvgPx, openMaxPos, pnl, pnlRatio,
 *  posId, posSide, realizedPnl, settledPnl, triggerPx, type, uTime, uly`.
 *
 * **`realizedPnl` is NET of both fee AND funding** — measured on a live row as
 * `pnl(-0.0014) + fee(-0.0100359) + fundingFee(0) = realizedPnl(-0.0114359)`. This differs
 * from bybit, whose `closedPnl` is net of fees but excludes funding, so any cross-check
 * against the ledger must know which convention a venue uses. Getting that wrong produces
 * a residual exactly the size of the funding term and looks like a mapping defect.
 *
 * `fee` is negative when charged here (the opposite of bybit), so `fees` takes its
 * absolute value to match the shared type's meaning of "fees paid".
 *
 * Sizes are in CONTRACTS. `closeTotalPos` must go through the exchange's
 * `contractsToBase` before it means anything to the UI, which is why that conversion is
 * injected rather than done here — it needs `ctVal` off the market.
 */

const toNumber = (value: unknown) => {
  const parsed = parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export const mapOkxClosedPosition = (
  row: Record<string, any>,
  market: Market | undefined,
  contractsToBase: (contracts: number, market: Market, price: number) => number,
  accountName: string,
): ClosedPnlEntry | null => {
  const openTime = parseInt(String(row.cTime ?? '0'), 10);
  const instId = String(row.instId ?? '');
  if (!openTime || !instId) {
    return null;
  }

  const closeAvgPx = toNumber(row.closeAvgPx);
  const nativeSize = toNumber(row.closeTotalPos);
  const contracts = market && closeAvgPx > 0 ? contractsToBase(nativeSize, market, closeAvgPx) : nativeSize;

  return {
    provenance: 'venue',
    symbol: market?.symbol || instId,
    notional: contracts * closeAvgPx,
    contracts,
    entryPrice: toNumber(row.openAvgPx),
    exitPrice: closeAvgPx,
    // Legacy field name; carries realized PnL on a closed row. Net of fee AND funding.
    unrealizedPnl: toNumber(row.realizedPnl),
    side: String(row.direction ?? '').toLowerCase() === 'short' ? PositionSide.Short : PositionSide.Long,
    // Charged fees arrive negative on this endpoint; the shared type means "fees paid".
    fees: Math.abs(toNumber(row.fee)),
    time: openTime,
    closedTime: parseInt(String(row.uTime ?? '0'), 10) || openTime,
    account: accountName,
    leverage: toNumber(row.lever),
  };
};
