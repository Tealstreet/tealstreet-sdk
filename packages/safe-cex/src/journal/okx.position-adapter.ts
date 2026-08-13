import type { PositionRecord } from './position.types';

import { PositionSide } from '../types';

/**
 * OKX `/api/v5/account/positions-history` -> canonical `PositionRecord`.
 *
 * OKX's row is already the canonical unit: one row per ROUND TRIP. A measured ladder
 * (open 3x, close in thirds) produced a single row with `qty` = the max size (420, not the
 * last partial's 140), `exit` = the size-weighted average of the three closes, and a true
 * open time. So this adapter converts conventions rather than folding anything.
 *
 * Two conversions are load-bearing:
 *
 * 1. **`realizedPnl` is NET here and canonical is GROSS.** Measured: `fillPnl -0.0014`
 *    on the fills axis against `realizedPnl -0.0112889` on this one, differing by exactly
 *    the `0.0098889` fee. OKX's figure is additionally net of funding, so both terms are
 *    added back.
 * 2. **Sizes are CONTRACTS.** `0.28` contracts is `280` DOGE. The caller supplies the
 *    conversion because it needs the market, and a record emitting a raw contract count
 *    would be wrong by the contract size with nothing downstream able to notice.
 */

const num = (value: unknown) => {
  const parsed = Number.parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

/** OKX `type`: 1 close-long, 2 close-short, 3 liquidation, 4 partial liquidation, 5 ADL. */
const CLOSE_REASON: Record<string, PositionRecord['closeReason']> = {
  '1': 'user',
  '2': 'user',
  '3': 'liquidation',
  '4': 'liquidation',
  '5': 'adl',
};

export type OkxPositionAdapterDeps = {
  account: string;
  /** Contracts -> base units, using the market's contract size. */
  toBase: (contracts: number, instId: string) => number;
  /** Venue instId -> normalized symbol; `undefined` when the market is unknown. */
  resolveSymbol: (instId: string) => string | undefined;
  /** Settle currency for the market. On linear this is the quote currency. */
  resolveSettleCurrency: (instId: string) => string | undefined;
};

export const mapOkxPositionHistory = (
  row: Record<string, any>,
  deps: OkxPositionAdapterDeps,
): PositionRecord | null => {
  const instId = String(row.instId ?? '');
  const symbol = deps.resolveSymbol(instId);
  // Emitting the raw venue id would silently poison every downstream group-by, and the
  // contract conversion below is meaningless without the market anyway.
  if (!symbol) return null;

  const direction = String(row.direction ?? '').toLowerCase();
  const side = direction === 'short' ? PositionSide.Short : PositionSide.Long;

  const closedAt = num(row.uTime) || undefined;
  const openedAt = num(row.cTime) || undefined;

  // `closeTotalPos` is cumulative CLOSED volume, not the position's peak size. OKX also
  // reports `openMaxPos`, which is the peak — they differ whenever a position was added
  // to mid-life, so they are carried as separate fields rather than conflated.
  const closedSize = deps.toBase(Math.abs(num(row.closeTotalPos)), instId);
  if (!(closedSize > 0)) return null;
  const openMaxPos = Math.abs(num(row.openMaxPos));
  const maxSize = openMaxPos > 0 ? deps.toBase(openMaxPos, instId) : undefined;

  // OKX's `fee` is NEGATIVE when charged and POSITIVE on a maker rebate, so canonical
  // `fees` (positive = cost) is its negation, NOT its absolute value. `Math.abs` would
  // report a rebate as a cost and then add it with the wrong sign, an error of twice the
  // rebate — and it typechecks, reconciles in aggregate, and only shows up on maker fills.
  const fees = -num(row.fee);
  const funding = num(row.fundingFee);
  // Measured relation (okx/CLAUDE.md): realizedPnl = gross + fee + fundingFee, with both
  // raw terms signed. So gross = realizedPnl - fee - fundingFee = realizedPnl + fees - funding.
  const realizedPnl = num(row.realizedPnl) + fees - funding;

  // `posSide` is 'net' in one-way mode and 'long'/'short' when hedging.
  const rawLeg = String(row.posSide ?? '').toLowerCase();
  const positionSide =
    rawLeg === 'long' ? PositionSide.Long : rawLeg === 'short' ? PositionSide.Short : ('both' as const);

  return {
    // OKX REUSES `posId` across separate open/close cycles in net mode — a measured
    // 30-day window returned 87 rows across only 3 posIds — so the id alone is not an
    // identity. The close timestamp plus the leg disambiguates the cycles.
    key: `${deps.account}|${symbol}|${positionSide}|${row.posId ?? 'nopos'}|${closedAt ?? 0}`,
    account: deps.account,
    symbol,
    // The row carries its own settlement currency; falling back to a hardcoded USDT
    // would silently mislabel a USDC-margined market.
    settleCurrency: String(row.ccy || '') || deps.resolveSettleCurrency(instId) || 'USDT',
    side,
    positionSide,
    state: 'closed',
    openedAt,
    closedAt,
    closedSize,
    maxSize,
    leverage: num(row.lever) || undefined,
    venueId: row.posId ? String(row.posId) : undefined,
    avgEntryPrice: num(row.openAvgPx),
    avgExitPrice: num(row.closeAvgPx) || undefined,
    realizedPnl,
    fees,
    funding,
    // OKX reports funding on the closed position itself, so it is attributed by the venue
    // rather than allocated by us — true even for a hedge leg.
    fundingAttributed: true,
    closeReason: CLOSE_REASON[String(row.type ?? '')] ?? 'user',
    provenance: 'reported',
    sourceAxis: 'closed-positions',
    // One OKX row is already the whole round trip, however many partial closes composed it.
    closeCount: 1,
  };
};
