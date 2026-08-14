/**
 * SIXTH INVARIANT: do the rows a venue returned actually fall inside the window we asked for?
 *
 * WHY THIS IS ORTHOGONAL TO THE OTHER FIVE, rather than a sixth flavour of them. The other
 * five all ask whether a row is internally coherent — its sign, its size, its fee, its
 * identity against a ledger. **Every one of them passes on a row from the wrong month.** The
 * rows are real; they were simply produced by a request whose window the venue discarded.
 *
 * MEASURED, not hypothesised. WOO's `/v1/client/trades` accepts `startTime`/`endTime`,
 * ignores them, and serves its newest page: a January-2020 window came back holding
 * August-2026 fills, which the caller returned as the answer for the window it had asked
 * for — and those rows are cached in `users_app_trades`, keyed on row id. The identity
 * reconciled, the signs were right, the fees were plausible, and the data was six years off.
 *
 * THE GENERAL SHAPE, which is why this is not a WOO fix: a venue that ignores an
 * UNRECOGNISED window parameter fails silently and identically. Nothing rejects the request,
 * so there is no error to catch; the only evidence is the returned timestamps themselves.
 * Every windowed axis on every venue is exposed to it, and the same shape already hid in
 * binance's swallowed rate limit (492 of 512 rows returned while reporting success) and in
 * bitget's ascending-page idleness verdict.
 *
 * WHICH TIMESTAMP. The one the venue filtered on — a close time for a closed-position axis,
 * a fill time for a fills axis. A position OPENED before `from` and CLOSED inside the window
 * is legitimately in range and must not be reported as an overrun; that straddler case is
 * normal and is why this takes one timestamp per row rather than an interval.
 */

export type WindowContainmentReport = {
  /** Rows whose timestamp falls inside the requested window. */
  readonly withinWindow: number;
  /**
   * Rows carrying no usable timestamp, or a call made with no window at all — NOT evidence
   * of correctness. An axis with no window semantics cannot fire this check and must not
   * read as green.
   */
  readonly unchecked: number;
  /** Rows outside the window, described. */
  readonly outside: string[];
  /**
   * How far outside the worst row fell, in milliseconds. This is the number that separates
   * an inclusive/exclusive boundary artifact from a discarded window: a few milliseconds is
   * the venue disagreeing about `>=` vs `>`; six years is the venue ignoring the parameter.
   */
  readonly worstOverrunMs: number;
};

export type WindowContainmentInput = {
  /** Milliseconds. A row with `undefined` here counts as unchecked, never as contained. */
  readonly timestampMs: number | undefined;
  /** Used only to describe an offending row. */
  readonly label?: string;
};

/**
 * Boundary slack, in milliseconds.
 *
 * Deliberately small and deliberately non-zero. Venues disagree about whether a window bound
 * is inclusive, and clocks skew — neither is the failure this hunts, which is off by weeks or
 * years. Widening this to hide a real overrun would defeat the check, so overruns are
 * reported WITH their magnitude instead of being absorbed.
 */
const BOUNDARY_SLACK_MS = 1_000;

export const checkWindowContainment = (
  rows: WindowContainmentInput[],
  window: { from?: number; to?: number },
  slackMs: number = BOUNDARY_SLACK_MS,
): WindowContainmentReport => {
  const { from, to } = window;
  const hasWindow = Number.isFinite(from) && Number.isFinite(to) && (to as number) > (from as number);

  // No window was requested, so nothing about these rows can be out of it. Every row is
  // UNCHECKED rather than contained — an unwindowed axis must not look verified.
  if (!hasWindow) {
    return { withinWindow: 0, unchecked: rows.length, outside: [], worstOverrunMs: 0 };
  }

  const lower = (from as number) - slackMs;
  const upper = (to as number) + slackMs;

  let withinWindow = 0;
  let unchecked = 0;
  let worstOverrunMs = 0;
  const outside: string[] = [];

  for (const row of rows) {
    const at = row.timestampMs;
    if (at === undefined || !Number.isFinite(at) || at <= 0) {
      unchecked += 1;
      continue;
    }

    if (at >= lower && at <= upper) {
      withinWindow += 1;
      continue;
    }

    const overrun = at < lower ? lower - at : at - upper;
    if (overrun > worstOverrunMs) worstOverrunMs = overrun;

    if (outside.length < 8) {
      const side = at < lower ? 'BEFORE from' : 'AFTER to';
      outside.push(
        `${row.label ?? 'row'} at=${new Date(at).toISOString()} ${side} by ${(overrun / 86_400_000).toFixed(2)}d`,
      );
    }
  }

  return { withinWindow, unchecked, outside, worstOverrunMs };
};

/**
 * Census line, in the same shape as `sideInvariant` and `feeConvention`.
 *
 * UNCHECKED is a separate outcome from consistent on purpose: an axis that has no window
 * semantics, or rows that carry no timestamp, produce no evidence either way, and the whole
 * point of this epic is that "nothing to check" has repeatedly been read as "checked and
 * fine".
 */
export const describeWindowContainment = (report: WindowContainmentReport): string => {
  const checked = report.withinWindow + report.outside.length;
  if (checked === 0) {
    return `UNCHECKED (${report.unchecked} row(s) carried no timestamp, or no window was requested)`;
  }

  // A sub-second overrun is a boundary convention, not a discarded window. Named rather than
  // silently tolerated, so the distinction stays visible in the census.
  const severity = report.worstOverrunMs > 86_400_000 ? ' — WINDOW APPEARS IGNORED' : '';

  return (
    `${report.withinWindow}/${checked} inside the requested window` +
    (report.outside.length
      ? `, ${report.outside.length} OUTSIDE (worst ${(report.worstOverrunMs / 86_400_000).toFixed(2)}d)${severity}`
      : '') +
    (report.unchecked ? `, ${report.unchecked} unchecked` : '')
  );
};
