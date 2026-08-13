import type { PositionRecord } from './position.types';

import { PositionSide } from '../types';

/**
 * BloFin `/api/v1/account/positions-history` -> canonical `PositionRecord`.
 *
 * Measured live 2026-08-13 (sheet row 39), not inherited from any existing venue. Row keys:
 * `historyId, positionId, instId, instType, marginMode, positionSide, closePositions,
 * maxPositions, liquidationPositions, openAveragePrice, closeAveragePrice, createTime,
 * updateTime, leverage, realizedPnl, realizedPnlRatio, fee`.
 *
 * A FOURTH convention profile — it matches no existing venue exactly:
 *
 * - `realizedPnl` is **net of fees**, and `fee` is **NEGATIVE when charged** (okx's sign
 *   convention, not binance's). So gross is `realizedPnl - fee`, verified as a consistent
 *   `-0.0042` across rows whose pnl and fee both varied.
 * - It carries a **real open time** (`createTime` != `updateTime`, measured 50s apart),
 *   which bybit's axis does not.
 * - The **hedge leg is on the row** (`positionSide`), so no order-axis join — unlike bybit.
 * - It reports **both** `maxPositions` (peak) and `closePositions` (closed volume), which
 *   bybit reports neither of.
 * - It has **no funding field at all**, so whether `realizedPnl` is also net of funding is
 *   NOT determinable from this row — see `funding` below.
 * - It uniquely reports **`liquidationPositions`**, a per-row liquidated quantity.
 */

const num = (value: unknown) => {
  const parsed = Number.parseFloat(String(value ?? ''));
  return Number.isFinite(parsed) ? parsed : 0;
};

export type BlofinPositionAdapterDeps = {
  account: string;
  /** Venue instId -> normalized symbol; `undefined` when the market is unknown. */
  resolveSymbol: (instId: string) => string | undefined;
  resolveSettleCurrency: (instId: string) => string | undefined;
  /**
   * Contract size for the market, when it is not 1.
   *
   * **Almost certainly leave this unset.** Verified on `DOGE-USDT`: `closePositions: "420"`
   * against a 420-DOGE e2e ladder, so this axis reports BASE units — even though the store's
   * `contractSize` for that same market is **1000**. The two are unrelated here, and wiring
   * the market's contractSize in inflated every size by 1000x while the PnL identity kept
   * PASSING, because size does not enter it. The hook exists only for a market later shown
   * to be contract-denominated on THIS endpoint; it is not a general units conversion.
   */
  contractSize?: (instId: string) => number;
};

export const mapBlofinPositionHistory = (
  row: Record<string, any>,
  deps: BlofinPositionAdapterDeps,
): PositionRecord | null => {
  const instId = String(row.instId ?? '');
  const symbol = deps.resolveSymbol(instId);
  if (!symbol) return null;

  const rawSide = String(row.positionSide ?? '').toLowerCase();
  // `net` is one-way mode, where this row carries NO direction at all: `closePositions` is
  // unsigned and the prices alone cannot distinguish a winning long from a losing short
  // once fees are netted in. Rather than guess a side — which would silently invert half a
  // one-way account's history — the row is refused so the caller counts it as skipped.
  // UNMEASURED: the account probed was hedged, so no `net` row has been observed.
  if (rawSide !== 'long' && rawSide !== 'short') return null;

  const side = rawSide === 'long' ? PositionSide.Long : PositionSide.Short;

  const scale = deps.contractSize?.(instId) || 1;
  const closedSize = Math.abs(num(row.closePositions)) * scale;
  if (!(closedSize > 0)) return null;
  const maxPositions = Math.abs(num(row.maxPositions)) * scale;

  // `fee` is NEGATIVE when charged and POSITIVE on a rebate, so canonical `fees`
  // (positive = cost) is its negation, NOT its absolute value — the same trap that cost
  // twice the rebate on okx and only shows up on maker fills.
  const fees = -num(row.fee);
  const realizedPnl = num(row.realizedPnl) + fees;

  const closedAt = num(row.updateTime) || undefined;
  const openedAt = num(row.createTime) || undefined;

  const liquidated = Math.abs(num(row.liquidationPositions)) > 0;

  return {
    // `positionId` is REUSED across separate open/close cycles — a live sample had 20 rows
    // across only 2 positionIds, the same trap as okx's posId. `historyId` is the row's own
    // identity and is what the venue's own paging dedupes on.
    key: `${deps.account}|${symbol}|${side}|${row.historyId ?? closedAt ?? ''}`,
    account: deps.account,
    symbol,
    settleCurrency: deps.resolveSettleCurrency(instId) || 'USDT',
    side,
    // The row's `positionSide` IS the hedge leg here. A `net` row never reaches this point.
    positionSide: side,
    state: 'closed',
    openedAt,
    closedAt,
    closedSize,
    maxSize: maxPositions > 0 ? maxPositions : undefined,
    leverage: num(row.leverage) || undefined,
    venueId: row.positionId ? String(row.positionId) : undefined,
    avgEntryPrice: num(row.openAveragePrice),
    avgExitPrice: num(row.closeAveragePrice) || undefined,
    realizedPnl,
    fees,
    // The row has NO funding field, so nothing can be attributed from this axis. Whether
    // `realizedPnl` is ALSO net of funding is therefore undecidable here and must be
    // settled by the ledger identity — which is exactly how bybit's whole-life funding fold
    // was found. Left 0 and unattributed rather than assumed absent.
    funding: 0,
    fundingAttributed: false,
    // BloFin is the only venue measured so far that reports a liquidated quantity per row.
    // NOTE: this classifies from a non-zero quantity; no live liquidation has been observed
    // on the test account, so the mapping is unexercised against real data.
    closeReason: liquidated ? 'liquidation' : 'user',
    provenance: 'reported',
    sourceAxis: 'closed-positions',
    // One row is already the whole round trip; `maxPositions` vs `closePositions` shows the
    // venue aggregated the partials itself.
    closeCount: 1,
  };
};
