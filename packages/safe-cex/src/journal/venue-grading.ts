import type { ExchangeName } from '../types';

/**
 * How much a venue's journal records can be TRUSTED, and why.
 *
 * This exists because the alternative — showing every venue's records identically — is the
 * hollow-verdict shape this whole epic spent its time deleting. A round trip from a venue
 * whose identity reconciles against an independent ledger and one from a venue that cannot be
 * graded at all are not the same claim, and a UI that renders them the same way is asserting
 * something nobody measured.
 *
 * **It lives here rather than in the web app because it is a property of the VENUE and its
 * axis, not of a view.** The CLI, the audit harness and any future surface need the same
 * answer, and a second copy would drift the moment one venue is upgraded.
 *
 * Every entry is backed by a measured run recorded in
 * `docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md` -> "STATE OF THE 11". Do not add a venue here
 * on the strength of an adapter existing; the whole survey was wrong four times that way.
 */

export type JournalVenueGrade =
  /** Identity reconciles against an independent axis. */
  | 'graded'
  /** Reconciles to the venue's own measured precision floor, and no closer. */
  | 'boundary'
  /** Records and per-record invariants are real; the identity has no gradeable counterparty. */
  | 'ungraded'
  /** No usable history axis. Must not be rendered as an empty journal. */
  | 'unsupported';

export type JournalVenueStatus = {
  readonly grade: JournalVenueGrade;
  /**
   * Shown to the user when the grade is not `graded`. Written to be read by a trader, not by
   * whoever wrote the adapter — it says what is weaker about the number in front of them.
   */
  readonly reason?: string;
  /**
   * A known defect on a venue that OTHERWISE grades clean. Separate from `reason` because a
   * caveat must not downgrade the badge — it qualifies a number that is still reconciled.
   */
  readonly caveat?: string;
};

const GRADED: JournalVenueStatus = { grade: 'graded' };

/**
 * Measured 2026-08-14 across the 11 e2e venues. Anything absent is `unsupported` by default,
 * which is the safe direction: a venue we have never graded must not silently render an empty
 * journal that reads as "you have no trades".
 */
const JOURNAL_VENUE_STATUS: Partial<Record<ExchangeName, JournalVenueStatus>> = {
  okx: GRADED,
  binance: GRADED,
  blofin: GRADED,
  bitunix: GRADED,
  bitget: GRADED,

  // PASSES the identity, but merging two round trips CONSERVES VALUE, so the identity cannot
  // see it: `aggregateBybitClosedPnl` buckets on `(symbol, leg, side, avgEntryPrice)` and two
  // cycles reopening at an identical entry price merge into one. Measured 4 round trips run,
  // 3 recorded. Totals are right; trade COUNT and average hold time are not.
  bybit: {
    grade: 'graded',
    caveat:
      'Two round trips that reopen at exactly the same entry price can merge into one. ' +
      'Totals are correct; trade count and hold time may undercount.',
  },

  // The ledger quantizes to 4dp while the fills carry full precision, so the two axes cannot
  // agree more closely than that. Reconciled to the floor, not beyond it.
  kraken: {
    grade: 'boundary',
    reason: "Reconciles to the limit of Kraken's own 4-decimal ledger precision, not beyond it.",
  },

  // Its ledger IS its fills route, so reconciling compares data to itself.
  paradex: {
    grade: 'ungraded',
    reason: 'Paradex reports its ledger and its fills from one endpoint, so they cannot cross-check each other.',
  },

  // Gradeable only against an equity axis whose baseline carries unrealized PnL.
  hyperliquid: {
    grade: 'ungraded',
    reason: 'Only checkable against an account-equity history, which includes open-position PnL.',
  },

  // Its ledger's realized-PnL rows are daily, account-level and carry no symbol.
  woo: {
    grade: 'ungraded',
    reason: 'WOO reports realized PnL only as a daily account total, which cannot be matched to individual trades.',
  },
};

/**
 * Default is `unsupported`, deliberately.
 *
 * phemex is the worked example: it has no reachable history axis, and rendering it as an empty
 * journal would read as "you have no trades" rather than "we cannot see this venue yet" —
 * the same failure as a zero that no reader understood being printed as `0`.
 */
export const getJournalVenueStatus = (exchange: ExchangeName): JournalVenueStatus =>
  JOURNAL_VENUE_STATUS[exchange] ?? {
    grade: 'unsupported',
    reason: 'Trade history is not available for this exchange yet.',
  };

/** Whether the journal should render this venue at all. */
export const isJournalSupported = (exchange: ExchangeName): boolean =>
  getJournalVenueStatus(exchange).grade !== 'unsupported';

/**
 * Whether a record's numbers rest on an independent cross-check. `false` does NOT mean wrong —
 * every per-record invariant still ran — it means nothing outside the venue's own data
 * confirmed the total, and the UI must not present it as though something did.
 */
export const isIndependentlyReconciled = (exchange: ExchangeName): boolean => {
  const grade = getJournalVenueStatus(exchange).grade;
  return grade === 'graded' || grade === 'boundary';
};
