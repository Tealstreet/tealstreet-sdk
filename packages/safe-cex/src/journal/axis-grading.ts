import type { ExchangeName } from '../types';
import type { JournalVenueCaveat } from './venue-grading';

import { getJournalVenueStatus } from './venue-grading';

/**
 * Grading PER AXIS, not per venue.
 *
 * `getJournalVenueStatus` returns one grade for a venue, and that grade
 * describes only the **position identity**. On bybit and okx the position axis
 * was graded against income while their fills axis was never independently
 * checked, so stamping the venue grade onto a fill row would assert something
 * nobody measured. Storage keeps the grade of the axis a row actually came from.
 */

export type HistoryAxis = 'positions' | 'fills' | 'orders' | 'income';

export type AxisGrade =
  /** Took part in an identity that closed against an independent axis. */
  | 'graded'
  /** Closes only to the venue's own measured precision floor, no further. */
  | 'boundary'
  /** Rows are real; no gradeable counterparty exists to check them against. */
  | 'ungraded'
  /** Never checked. The honest default. */
  | 'unmeasured'
  /** No such axis on this venue. */
  | 'unsupported';

/**
 * Whether the grade still rests on the instrument that produced it.
 *
 * **A separate dimension from the grade, deliberately.** Collapsing a
 * provenance question into `unmeasured` would erase the difference between "we
 * never looked" and "we looked, with a different instrument than the one that
 * produced this verdict" — and that distinction is the epic's central lesson,
 * not a nicety. A row graded on a superseded probe is not ungraded; it is graded
 * pending confirmation, and the two demand different treatment: the first needs
 * a first measurement, the second needs a re-run.
 */
export type GradeProvenance =
  /** The verdict and the instrument that produced it still agree. */
  | 'current'
  /** Verdict stands, but the probe that produced it has since changed. */
  | 'pending-revalidation';

export type AxisStatus = {
  readonly grade: AxisGrade;
  readonly provenance: GradeProvenance;
  readonly reason?: string;
  readonly caveat?: JournalVenueCaveat;
};

/**
 * Venues whose recorded verdict came from a probe that has since been
 * repointed.
 *
 * `history-audit` dispatched `exchange[venue.historyMethod]`, and these three
 * were configured as `fetchHistoricalTrades`. That method was deleted 2026-08-16
 * and their probes now read `fetchHistoricalFills`. The mapper was made a strict
 * superset first, so the verdict is expected to hold — but "expected to hold" is
 * not "measured", and every venue verdict in this epic came from that harness.
 *
 * **Clear an entry only after a harness run confirms the grade on the new
 * probe.** It needs live credentials, so it is not something this code can do
 * for itself.
 */
const PENDING_REVALIDATION: ReadonlySet<ExchangeName> = new Set(['binance', 'woo', 'bitrue'] as ExchangeName[]);

/**
 * Which axes actually took part in each venue's measured identity.
 *
 * Read off the venue's `sourceAxis` and its adapter, NOT assumed. An axis absent
 * here is `unmeasured` — the point of the whole model is that a venue grade does
 * not transfer to an axis nobody checked.
 */
const MEASURED_AXES: Partial<Record<ExchangeName, readonly HistoryAxis[]>> = {
  // Native closed-position axis graded against the income ledger.
  okx: ['positions', 'income'],
  blofin: ['positions', 'income'],
  bitunix: ['positions', 'income'],
  // Closed-PnL rows joined to the ORDER axis to recover the hedge leg, graded
  // against income. Without that join longs and shorts merged and the count came
  // out short — 127 records against the gate's 150.
  bybit: ['positions', 'orders', 'income'],
  // Folded from fills, graded against income.
  binance: ['positions', 'fills', 'income'],
  bitget: ['positions', 'fills', 'income'],
  kraken: ['positions', 'fills', 'income'],
  // Folded from ORDERS — the only woo route carrying realized PnL, total fee and
  // position side on one row. Both its fills axes report `realizedPnl: null`.
  woo: ['positions', 'orders'],
  // Circular: ledger and positions both derive from one `/fills` call, so the
  // identity closes regardless of what the data says.
  hyperliquid: ['positions', 'fills'],
  paradex: ['positions', 'fills'],
};

/** Axes a venue can serve at all, regardless of whether they were graded. */
const AVAILABLE_AXES: Partial<Record<ExchangeName, readonly HistoryAxis[]>> = {
  okx: ['positions', 'fills', 'orders', 'income'],
  bybit: ['positions', 'fills', 'orders', 'income'],
  binance: ['positions', 'fills', 'orders', 'income'],
  blofin: ['positions', 'fills', 'orders', 'income'],
  bitunix: ['positions', 'fills', 'orders', 'income'],
  bitget: ['positions', 'fills', 'income'],
  kraken: ['positions', 'fills', 'income'],
  woo: ['positions', 'fills', 'orders'],
  hyperliquid: ['positions', 'fills', 'orders', 'income'],
  paradex: ['positions', 'fills', 'orders', 'income'],
  bitmex: ['fills', 'income'],
  weex: ['fills', 'income'],
  asterdex: ['fills'],
  bitrue: ['fills'],
  bingx: ['fills'],
  apexomni: ['fills'],
  lighter: ['fills'],
  backpack: ['fills'],
  phemex: ['fills'],
  // VARIANTS. Each has a working `fetchHistoricalFills` — its own (kucoinspot,
  // bybitinverse) or inherited from its parent class (bitgetusdc and
  // bitgetinverse extend BitgetExchange; okxeu extends OKXExchange) — and every
  // one of them was absent from this table, so `collectAccountAxes` reported
  // them unsupported and never asked. Their axes mirror what the class they
  // resolve to actually implements, not what the parent VENUE is graded for:
  // none carries a `MEASURED_AXES` entry, so all of them read `unmeasured`.
  // That is the honest answer — the harness never probed a variant, and a
  // spot book or an inverse contract is not the venue its parent was measured
  // on. Collection does not wait on grading.
  //
  // `bitgetinverse` sets `enabled: false` and so never registers today; its
  // entry is inert rather than wrong, and is here so enabling the venue does not
  // silently ship it with collection switched off. The other four inherit
  // `enabled: true` through their metadata spread and are live.
  kucoinspot: ['fills'],
  bybitinverse: ['fills'],
  bitgetusdc: ['positions', 'fills', 'income'],
  bitgetinverse: ['positions', 'fills', 'income'],
  okxeu: ['positions', 'fills', 'orders', 'income'],
};

const UNSUPPORTED: AxisStatus = {
  grade: 'unsupported',
  provenance: 'current',
  reason: 'This exchange has no such history axis yet.',
};

/**
 * The grade for one venue's one axis.
 *
 * Collection is never gated on this — trust is a column. A venue that can serve
 * an axis is collected from even at `unmeasured`, because a row we hold and
 * cannot yet vouch for is strictly better than one that aged out unrecorded.
 */
export const getAxisStatus = (exchange: ExchangeName, axis: HistoryAxis): AxisStatus => {
  const available = AVAILABLE_AXES[exchange];
  if (!available?.includes(axis)) return UNSUPPORTED;

  const provenance: GradeProvenance = PENDING_REVALIDATION.has(exchange) ? 'pending-revalidation' : 'current';
  const measured = MEASURED_AXES[exchange];

  if (!measured?.includes(axis)) {
    return {
      grade: 'unmeasured',
      // Never `pending-revalidation`: nothing was measured here, so there is no
      // verdict for a re-run to confirm. This is the distinction the two states
      // exist to keep apart.
      provenance: 'current',
      reason: 'This axis has not been checked against an independent counterparty.',
    };
  }

  const venue = getJournalVenueStatus(exchange);
  return {
    grade: venue.grade === 'unsupported' ? 'unmeasured' : venue.grade,
    provenance,
    reason: venue.reason,
    caveat: venue.caveat,
  };
};

/**
 * Whether a stored row's numbers rest on a confirmed independent cross-check.
 *
 * `pending-revalidation` returns FALSE. The verdict may well stand, but a
 * consumer asking "is this independently verified" is asking about evidence, and
 * the evidence currently points at an instrument that has been changed.
 */
export const isAxisIndependentlyReconciled = (exchange: ExchangeName, axis: HistoryAxis): boolean => {
  const status = getAxisStatus(exchange, axis);
  if (status.provenance !== 'current') return false;
  return status.grade === 'graded' || status.grade === 'boundary';
};
