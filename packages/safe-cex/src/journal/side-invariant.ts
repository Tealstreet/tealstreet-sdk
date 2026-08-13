import type { PositionRecord } from './position.types';

import { PositionSide } from '../types';

export type PnlMagnitudeReport = {
  /** Records whose |PnL| matches the price move within tolerance. */
  readonly consistent: number;
  /** Records carrying no usable magnitude signal — NOT evidence of correctness. */
  readonly unchecked: number;
  /** Records whose PnL is the wrong SIZE, with the observed ratio. */
  readonly mismatches: string[];
};

export type SideConsistencyReport = {
  /** Records whose PnL sign agrees with side + price move. */
  readonly consistent: number;
  /** Records that carried no usable signal — NOT evidence of correctness. */
  readonly unchecked: number;
  /** Human-readable descriptions of records whose sign is impossible. */
  readonly contradictions: string[];
};

/**
 * PER-RECORD SIDE INVARIANT: is the PnL sign consistent with the side and the price move?
 *
 * For a linear perp, gross realized PnL is purely price-driven — a long profits when the
 * exit is above the entry, a short when it is below. So `sign(realizedPnl)` must equal
 * `sign(exit - entry)` for a long and its negation for a short.
 *
 * WHY THIS IS NOW A FIRST-CLASS CHECK. It is the ONLY side check available on a fold-only
 * venue, and every remaining candidate venue is fold-only. It was improvised by hand to
 * settle whether bitunix's `BUY`/`SELL` meant the position's direction or an order side —
 * correlating PnL sign against the price move across 14 rows answered it. Reading that field
 * wrong inverts every record on the venue while every total still reconciles, so no identity
 * check can see it.
 *
 * IT MUST NOT REPORT "CONSISTENT" WHERE IT SIMPLY COULD NOT FIRE. A record whose entry and
 * exit are equal, or whose expected PnL is smaller than the rounding on the numbers, carries
 * no signal at all. Those are UNCHECKED — the same distinction as a PASS over a window with
 * no funding in it.
 */
export const checkSideConsistency = (records: PositionRecord[]): SideConsistencyReport => {
  let consistent = 0;
  let unchecked = 0;
  const contradictions: string[] = [];

  for (const record of records) {
    const entry = record.avgEntryPrice;
    const exit = record.avgExitPrice;

    // No exit price, no move, or no size: nothing to correlate against.
    if (!entry || !exit || !(record.closedSize > 0) || entry === exit) {
      unchecked += 1;
      continue;
    }

    // What the price move alone should have produced, in settle currency.
    const direction = record.side === PositionSide.Long ? 1 : -1;
    const expected = (exit - entry) * record.closedSize * direction;

    // Below the noise floor the sign is rounding, not evidence. Scaled to notional so it
    // adapts to a 0.07 DOGE position and a 9.34 LINK one alike.
    const noiseFloor = Math.max(1e-8, 1e-6 * entry * record.closedSize);
    if (Math.abs(expected) <= noiseFloor || Math.abs(record.realizedPnl) <= noiseFloor) {
      unchecked += 1;
      continue;
    }

    // Funding that could not be separated out would sit inside `realizedPnl` and could flip
    // its sign on a long-held, low-PnL position. Where it dominates, the signal is gone.
    if (Math.abs(record.funding) >= Math.abs(record.realizedPnl)) {
      unchecked += 1;
      continue;
    }

    if (Math.sign(expected) === Math.sign(record.realizedPnl)) {
      consistent += 1;
    } else {
      contradictions.push(
        `${record.symbol} ${record.side} entry=${entry} exit=${exit} pnl=${record.realizedPnl.toFixed(8)} expected sign ${Math.sign(expected)}`,
      );
    }
  }

  return { consistent, unchecked, contradictions };
};

export type FeeConventionReport = {
  /** Records whose fee is a cost of a plausible size. */
  readonly consistent: number;
  /** Records whose fee is NEGATIVE, i.e. a rebate. Real, but rare on a real account. */
  readonly rebates: number;
  /** Records carrying no fee or no notional — NOT evidence of correctness. */
  readonly unchecked: number;
  /** Fees too large to be a trading fee, with the observed fraction of notional. */
  readonly implausible: string[];
};

/**
 * PER-RECORD FEE INVARIANT: is the fee a COST, and of a believable size?
 *
 * WRITTEN BEFORE RUNNING BITGET, by asking what that venue could break that the other four
 * would miss — the same discipline that produced `checkPnlMagnitude` before bitmex, and that
 * caught bitmex's minor-units scale bug.
 *
 * The answer is the fee SIGN, and it is the exact field the venues already measured disagree
 * on: blofin reports `fee` NEGATIVE when charged, bitunix POSITIVE, and bitget (measured on
 * 818 live rows) negative. Canonical `PositionRecord.fees` is positive-when-charged, so half
 * the venues need negating and half do not. Getting it backwards on a venue turns every
 * commission into a rebate.
 *
 * NONE OF THE OTHER FOUR CAN SEE THAT:
 *
 * - **the ledger identity** compares GROSS PnL against the ledger's realized term. Fees are
 *   deliberately excluded from both sides, so a fee-sign error changes nothing it looks at;
 * - **the side invariant** reads only the sign of `realizedPnl`;
 * - **the magnitude invariant** reads only its size;
 * - **the zero-PnL refusal** only asks whether a value is present.
 *
 * So a venue could ship with every fee inverted, reconcile to 1e-16, and pass every existing
 * check — while any consumer computing net PnL would be wrong by twice the fee on every
 * round trip. That is precisely the class this epic exists to catch.
 *
 * Rebates are REAL (maker rebates exist), so a single negative fee is not a defect. The
 * signal is the RATE: a venue where most records are rebates has an inverted sign, which is
 * why the count is reported rather than a pass/fail per record.
 */
export const checkFeeConvention = (records: PositionRecord[]): FeeConventionReport => {
  let consistent = 0;
  let rebates = 0;
  let unchecked = 0;
  const implausible: string[] = [];

  for (const record of records) {
    const notional = record.avgEntryPrice * record.closedSize;
    // No notional to scale against, or no fee charged at all: no signal either way.
    if (!(notional > 0) || record.fees === 0) {
      unchecked += 1;
      continue;
    }

    if (record.fees < 0) {
      rebates += 1;
      continue;
    }

    // A perp trading fee is basis points. Anything approaching a percent of notional is not
    // a fee — it is a units error, a wrong field, or a scale bug.
    const fraction = record.fees / notional;
    if (fraction > 0.01) {
      implausible.push(
        `${record.symbol} fees=${record.fees} notional=${notional.toFixed(8)} = ${(fraction * 100).toFixed(2)}% of notional`,
      );
      continue;
    }

    consistent += 1;
  }

  return { consistent, rebates, unchecked, implausible };
};

/**
 * PER-RECORD MAGNITUDE INVARIANT: is the PnL the right SIZE for the price move?
 *
 * For a linear perp, gross realized PnL is exactly `closedSize * (exit - entry)` — not
 * approximately. Measured on blofin and bitunix, the two agree to every printed digit. So
 * the ratio of reported to expected should sit at 1.
 *
 * WHY THIS IS THE FOURTH INVARIANT, and what it catches that the other three cannot. A
 * SCALE error — a venue reporting money in minor units, or quantities in contracts with a
 * multiplier — multiplies PnL by a constant. That is invisible to:
 *
 * - **the ledger identity**, when both sides read the same scaled source, so the factor
 *   cancels and the totals agree;
 * - **the side invariant**, because a positive scale factor preserves sign;
 * - **the zero-PnL refusal**, because the value is present.
 *
 * Written BEFORE running bitmex rather than after, because bitmex is the venue that makes it
 * concrete: `/user/walletHistory` reports `walletBalance: 60827952` in `USDt` on an account
 * worth about sixty dollars — minor units, a factor of 1e6. A record built by dividing by the
 * wrong power of ten would reconcile against a ledger scaled the same way and pass every
 * other check.
 *
 * The tolerance is deliberately loose (a factor of two) because this hunts ORDER-OF-MAGNITUDE
 * errors, not pennies. Anything nearer than that is the identity's job.
 */
export const checkPnlMagnitude = (records: PositionRecord[]): PnlMagnitudeReport => {
  let consistent = 0;
  let unchecked = 0;
  const mismatches: string[] = [];

  for (const record of records) {
    const entry = record.avgEntryPrice;
    const exit = record.avgExitPrice;

    if (!entry || !exit || !(record.closedSize > 0) || entry === exit) {
      unchecked += 1;
      continue;
    }

    const expected = Math.abs((exit - entry) * record.closedSize);
    const actual = Math.abs(record.realizedPnl);
    const noiseFloor = Math.max(1e-8, 1e-6 * entry * record.closedSize);

    if (expected <= noiseFloor || actual <= noiseFloor) {
      unchecked += 1;
      continue;
    }
    // Funding folded into the figure moves its magnitude, so it is not a clean comparison.
    if (Math.abs(record.funding) >= actual) {
      unchecked += 1;
      continue;
    }

    const ratio = actual / expected;
    if (ratio >= 0.5 && ratio <= 2) {
      consistent += 1;
    } else {
      mismatches.push(
        `${record.symbol} pnl=${record.realizedPnl} expected~${expected.toFixed(8)} ratio=${ratio.toExponential(2)}`,
      );
    }
  }

  return { consistent, unchecked, mismatches };
};
