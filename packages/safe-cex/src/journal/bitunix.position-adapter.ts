import type { PositionRecord } from './position.types';

import { PositionSide } from '../types';

/**
 * BitUnix `/api/v1/futures/position/get_history_positions` -> canonical `PositionRecord`.
 *
 * Measured live 2026-08-13 (sheet row 56). Row keys: `positionId, symbol, marginCoin,
 * maxQty, qty, entryPrice, closePrice, liqQty, side, marginMode, positionMode, leverage,
 * fee, funding, realizedPNL, margin, liqPrice, ctime, mtime, subAccountId`.
 *
 * A FIFTH convention profile — it matches no previous venue exactly:
 *
 * - `realizedPNL` is **net of fees** and `fee` is **POSITIVE when charged** — the opposite
 *   sign to blofin and okx, which report it negative. So gross is `realizedPNL + fee`,
 *   pinned by a row where `entryPrice == closePrice` and `funding == 0`: pnl `-0.00201641`
 *   against fee `0.00201641`, summing to exactly zero.
 * - **`side` is the POSITION's direction, not an order side.** `BUY` = long, `SELL` = short,
 *   verified on 14 rows where the price moved — every SELL profits when the price falls,
 *   every BUY when it rises, with zero contradictions. This is the field most likely to be
 *   read as an order side, which would invert every record while still reconciling in
 *   aggregate.
 * - It reports **`funding` ON THE ROW**, which bybit and blofin do not — and `realizedPNL`
 *   is net of that too. MEASURED: a 90-day identity run carrying two funding-bearing
 *   positions failed by `-0.09159256` against a funding total of `-0.09159256`, equal to the
 *   digit. So gross is `realizedPNL + fee - funding`. A 30-day window contains no funding at
 *   all and reconciles perfectly WITHOUT the term, which is precisely the false green this
 *   epic keeps finding.
 * - It carries a **real open time** (`ctime` != `mtime`).
 * - `positionMode` is an explicit `HEDGE` / one-way marker, so the leg is derivable rather
 *   than inferred.
 * - `liqQty` / `liqPrice` mark a **liquidation**, and unlike every other venue measured, a
 *   REAL one exists on the test account.
 */

const num = (value: unknown) => {
  const parsed = Number.parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export type BitunixPositionAdapterDeps = {
  account: string;
  resolveSettleCurrency: (symbol: string) => string;
  /**
   * Whether `realizedPNL` is also net of FUNDING, in addition to fees.
   *
   * Deliberately an INPUT, not a constant. The row reports `funding` but cannot say whether
   * it is already inside `realizedPNL`, and getting it wrong shifts every funding-bearing
   * record by that term while still reconciling in aggregate — exactly how bybit's assumed
   * `funding: false` survived.
   *
   * MEASURED TRUE for bitunix (see the header): callers on this venue must pass `true`. It
   * stays a parameter because it is the most error-prone bit per venue — bybit needed it,
   * binance did not, and blofin still cannot say — so each caller passes what its own
   * identity run showed rather than inheriting a neighbour's answer.
   */
  realizedIsNetOfFunding?: boolean;
};

export const mapBitunixPositionHistory = (
  row: Record<string, any>,
  deps: BitunixPositionAdapterDeps,
): PositionRecord | null => {
  const symbol = String(row.symbol ?? '');
  if (!symbol) return null;

  const rawSide = String(row.side ?? '').toUpperCase();
  // BUY = long, SELL = short — the POSITION's direction. Refused rather than defaulted,
  // because a wrong default inverts the record silently.
  if (rawSide !== 'BUY' && rawSide !== 'SELL') return null;
  const side = rawSide === 'BUY' ? PositionSide.Long : PositionSide.Short;

  // `qty` is what closed; `maxQty` is the peak the position reached. They diverge whenever
  // a position was added to mid-life, so they are kept apart rather than conflated.
  const closedSize = Math.abs(num(row.qty));
  if (!(closedSize > 0)) return null;
  const maxQty = Math.abs(num(row.maxQty));

  // POSITIVE when charged here — the opposite of blofin/okx. Negating it, as those need,
  // would flip every fee into a rebate.
  const fees = num(row.fee);
  const funding = num(row.funding);
  // Gross = realizedPNL + fee, pinned on a zero-price-move row. The funding term is added
  // back only when the caller has evidence it is folded in.
  const realizedPnl = num(row.realizedPNL) + fees - (deps.realizedIsNetOfFunding ? funding : 0);

  const openedAt = num(row.ctime) || undefined;
  const closedAt = num(row.mtime) || undefined;

  // An explicit venue field, so the leg is read rather than inferred. In one-way mode the
  // direction is still `side`, but there is no leg to occupy.
  const hedged = String(row.positionMode ?? '').toUpperCase() === 'HEDGE';

  // A non-zero liquidated quantity IS the marker. Unlike every other venue measured, this
  // one has a REAL specimen on the test account (liqQty -4.6685, closePrice == liqPrice).
  const liquidated = Math.abs(num(row.liqQty)) > 0;

  return {
    // `positionId` is the row's own identity here and was unique across the sample, unlike
    // okx's posId and blofin's positionId which are both reused across cycles.
    key: `${deps.account}|${symbol}|${side}|${row.positionId ?? closedAt ?? ''}`,
    account: deps.account,
    symbol,
    // `marginCoin` is null on every measured row, so the market's quote is the only source.
    settleCurrency: deps.resolveSettleCurrency(symbol),
    side,
    positionSide: hedged ? side : 'both',
    state: 'closed',
    openedAt,
    closedAt,
    closedSize,
    maxSize: maxQty > 0 ? maxQty : undefined,
    leverage: num(row.leverage) || undefined,
    venueId: row.positionId ? String(row.positionId) : undefined,
    avgEntryPrice: num(row.entryPrice),
    avgExitPrice: num(row.closePrice) || undefined,
    realizedPnl,
    fees,
    funding,
    // Venue-reported per position, not allocated by us — the only closed axis measured so
    // far that can say this truthfully.
    fundingAttributed: true,
    closeReason: liquidated ? 'liquidation' : 'user',
    provenance: 'reported',
    sourceAxis: 'closed-positions',
    closeCount: 1,
  };
};
