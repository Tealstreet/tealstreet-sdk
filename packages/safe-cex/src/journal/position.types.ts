import type { PositionSide } from '../types';

/**
 * Canonical position model — the unit traders actually measure.
 *
 * Every field here is shaped by a measured venue behaviour, recorded in
 * `docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md`. The exchange axes disagree with each other
 * in ways that are invisible to any totals-based check, so the coercion happens here and
 * consumers never branch on venue.
 *
 * Scope is **linear perps**. That is a declarable product decision; no event class is
 * excluded. Inverse contracts invert units (size in quote, PnL in base) and are out.
 */

/** Why a position stopped existing. A liquidation is a different journal entry, not a close. */
export type PositionCloseReason = 'user' | 'liquidation' | 'adl' | 'settlement';

/**
 * Whether the venue reported this position or we folded it out of fills.
 *
 * Consumers must not present `derived` rows as verified: a fold is path-dependent, so one
 * bad fill corrupts every position after it, whereas a reported row's error is local.
 */
export type PositionProvenance = 'reported' | 'derived';

/**
 * Which axis produced the record.
 *
 * `fills+orders` exists because bybit carries no position side on fills at all — in hedge
 * mode as in one-way — so its leg has to be joined from `/v5/order/history.positionIdx`
 * on `orderId`. OKX needs one axis for the same reconstruction.
 *
 * The distinction from `fills` matters downstream: a fold of FILLS is path-dependent, so
 * one bad fill corrupts every later position. A closed-position row joined to orders is
 * still venue-reported and error-local, hence the separate `closed-positions+orders`.
 */
export type PositionSourceAxis = 'closed-positions' | 'closed-positions+orders' | 'fills' | 'fills+orders';

export type PositionRecord = {
  /**
   * Stable identity. MUST include the leg and a sequence: a hedge-mode flatten emits one
   * closed row per leg **sharing a timestamp** (measured on okx: `closed=15:24:05` twice,
   * once long once short), so anything keyed on `(symbol, closedTime)` silently drops a
   * leg. The sequence distinguishes re-opening the same symbol from resurrecting it.
   */
  readonly key: string;
  readonly account: string;
  readonly symbol: string;

  /**
   * Currency the PnL settles in. On linear this is the quote currency, which is what makes
   * multi-currency settlement (USDT / USDC / USDe) tractable without a rate: keep it
   * native here and convert at display with a recorded rate. A single USD number with no
   * rate attached is unauditable.
   */
  readonly settleCurrency: string;

  /** Direction of the position. */
  readonly side: PositionSide;

  /**
   * The hedge leg this position occupies; `'both'` in one-way mode.
   *
   * Never derived from the order side. In one measured okx run a `sell` opened the short
   * leg and, 18 seconds later, another `sell` closed the long one.
   */
  readonly positionSide: PositionSide | 'both';

  readonly state: 'open' | 'closed';

  /**
   * OPTIONAL because bybit does not report it: every `/v5/position/closed-pnl` row returns
   * `opened == closed`, even for a position held across several fills. OKX reports a true
   * open time. Making this required would force every bybit position to claim a zero hold
   * time, which is worse than admitting the value is unavailable.
   */
  readonly openedAt?: number;
  readonly closedAt?: number;

  /**
   * Total volume CLOSED over the position's life, in BASE units.
   *
   * Deliberately not named `maxSize`: both venues report cumulative closed volume, and
   * for a position that was added to mid-life the two differ (open 100, close 50, add
   * 100, close 150 -> closed volume 200, peak size 150). Conflating them would report a
   * size the position never held.
   *
   * Base units are canonical even though okx quotes linear swaps in contracts
   * (`0.42` contracts = `420` DOGE). Linear-only scope does not remove that conversion;
   * it only removes inverse's unit inversion.
   */
  readonly closedSize: number;

  /**
   * Peak size the position actually reached, where the venue reports it (okx
   * `openMaxPos`). Absent on venues that only report closed volume — bybit's closed-PnL
   * rows carry no peak, and inferring one from the partials would be a guess.
   */
  readonly maxSize?: number;

  /** Leverage as reported by the venue, where available. */
  readonly leverage?: number;

  /** Quote-denominated value closed, where the venue reports it. */
  readonly closedNotional?: number;

  /**
   * The venue's own identifier for drill-down (okx `posId`, bybit closing `orderId`).
   * Not an identity — okx reuses `posId` across separate open/close cycles.
   */
  readonly venueId?: string;

  readonly avgEntryPrice: number;
  /** Size-weighted across every closing event; absent while the position is open. */
  readonly avgExitPrice?: number;

  /**
   * Realized PnL, **GROSS of fees**, in `settleCurrency`.
   *
   * This is the single most important normalization in the model. The same trade reports
   * gross on the fills axis and net on the closed-positions axis, on the same venue —
   * measured on okx as `fillPnl -0.0014` against `realizedPnl -0.0112889` with a
   * `0.0098889` fee, and on bybit as `-0.0123872` net against `-0.00141` gross. Both axes
   * name the quantity "realized PnL", so an adapter that passes the venue's field through
   * shifts the number by the commission — and every reconciliation still balances, because
   * the fee moves with it.
   */
  readonly realizedPnl: number;

  /** Positive = cost to the trader. */
  readonly fees: number;

  /** Signed, positive = received. Never folded into `realizedPnl`. */
  readonly funding: number;

  /**
   * False when funding could not be attributed to this position specifically.
   *
   * Funding arrives per symbol, not per leg. With both hedge legs open there is no
   * venue-reported split, and any allocation we invent is a fabrication — so it is
   * recorded at symbol level and flagged here rather than silently divided.
   */
  readonly fundingAttributed: boolean;

  readonly closeReason?: PositionCloseReason;

  readonly provenance: PositionProvenance;
  readonly sourceAxis: PositionSourceAxis;

  /**
   * How many closing events composed this position.
   *
   * Exists because the two venues disagree on the grain of the SAME axis: for one ladder
   * (open 3x, close in thirds) okx emitted one closed row for the round trip while bybit
   * emitted three, one per partial. Normalizing to the round trip means bybit's rows are
   * aggregated, and this records how many were folded in — so a "trade count" cannot read
   * 1 on one venue and 3 on the other for identical activity.
   */
  readonly closeCount: number;
};
