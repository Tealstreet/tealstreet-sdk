import type { PositionRecord } from './position.types';

import { PositionSide } from '../types';

/**
 * Grading position history against an oracle that is NOT the exchange.
 *
 * Every other check in this tree compares a venue's mapped data against that same venue's
 * raw rows. That proves the mapper parses, and it cannot prove the numbers describe what
 * happened — both sides come from the same place and agree by construction. It is why
 * hyperliquid and paradex are ungradeable: their ledger and their positions both derive
 * from one `/fills` call, so the identity closes no matter what.
 *
 * The Cypress e2e suite is the way out. It flattens every account before each test, then
 * performs known actions, and `recordGroundTruth` in `apps/web/cypress/e2e/spec.cy.ts`
 * appends what it did plus the position state that resulted. The suite knows it opened a
 * long and closed it four seconds later; a venue's history that does not contain that
 * round trip is wrong however self-consistent it looks.
 *
 * The file read lives in `scripts/history-audit/lib/ground-truth.ts`, not here — safe-cex
 * ships in the `tealstreet-sdk` OSS mirror and must not gain an `fs` dependency. This
 * module is pure so it can be tested by the package's own vitest run; a test under
 * `scripts/**` is never collected (see `scripts/keys/lib/*.test.ts`, orphaned).
 */

/** One position as the suite observed it, straight off `__getStoreSnapshot()`. */
export type GroundTruthPositionSnapshot = {
  readonly side: string;
  readonly contracts: number;
  readonly entryPrice?: number;
};

/**
 * One line of `<exchange>-ground-truth.jsonl`.
 *
 * `action` is deliberately `string` rather than the recorder's union: the log is written
 * by a different package on a different release cadence, so a line carrying an action this
 * build has never heard of must parse, not throw. The fold ignores the verb anyway — see
 * `buildExpectedRoundTrips`.
 */
export type GroundTruthEvent = {
  readonly ts: number;
  readonly symbol: string;
  readonly action: string;
  readonly test?: string;
  readonly exchange?: string;
  readonly positionsAfter?: GroundTruthPositionSnapshot[];
};

/**
 * A round trip the suite is KNOWN to have opened and closed.
 *
 * Both ends are intervals, not instants, and that is the design rather than sloppiness.
 * `positionsAfter` is sampled after an action, so all the log establishes is that the
 * transition happened at some point between the previous sample and this one. A normal
 * close bounds that to a few seconds; an aborted rerun (below) bounds it to a couple of
 * minutes. Matching consumes the interval, so the two need no special-casing.
 */
export type ExpectedRoundTrip = {
  readonly symbol: string;
  readonly side: PositionSide;
  readonly openedAtLower: number;
  readonly openedAtUpper: number;
  readonly closedAtLower: number;
  readonly closedAtUpper: number;
  /** Largest size the leg reached, in contracts. Context for a human reading a failure. */
  readonly maxContracts: number;
  /** Actions observed while the leg was open, in order. Metadata only. */
  readonly actions: string[];
};

/** A leg the log shows opening and never closing. NOT a missing round trip — see below. */
export type UnclosedLeg = {
  readonly symbol: string;
  readonly side: PositionSide;
  readonly openedAtLower: number;
  readonly openedAtUpper: number;
  readonly maxContracts: number;
};

export type GroundTruthFold = {
  readonly trips: ExpectedRoundTrip[];
  readonly unclosed: UnclosedLeg[];
  /** Lines whose shape could not be read. Counted, never silently dropped. */
  readonly unreadable: number;
};

const toSide = (raw: string | undefined): PositionSide | undefined => {
  if (raw === PositionSide.Long) return PositionSide.Long;
  if (raw === PositionSide.Short) return PositionSide.Short;
  return undefined;
};

type OpenLeg = {
  side: PositionSide;
  symbol: string;
  openedAtLower: number;
  openedAtUpper: number;
  maxContracts: number;
  actions: string[];
};

/**
 * Folds the event stream into the round trips it proves happened.
 *
 * **Driven by `positionsAfter`, not by the action verb**, and that is the whole of the
 * design. A verb-driven fold reads `close` as a close and `flip` as a close-plus-open,
 * which works right up until the log does something the verbs do not describe — and the
 * real logs do, on the first venue checked. bybit's has three `market-order → flip` pairs
 * where the short opened by one flip is simply gone by the next `market-order`, with no
 * `close` between them: the runs were aborted and the suite's own `flattenPositions`
 * cleanup closed the leg without recording anything. A verb-driven fold either loses those
 * legs or reports them as still open forever.
 *
 * Observed state has no such gap. Diffing consecutive `positionsAfter` snapshots detects
 * the close wherever it happened, and bounds it to the interval between the two samples.
 *
 * It also makes three other cases fall out for free rather than needing branches:
 *
 * - **`flip-observed`** carries the same `positionsAfter` as the `flip` ~16ms later. As an
 *   observation it produces no transition and is absorbed silently. As a verb it would
 *   read as a second close of a leg already closed.
 * - **Hedge mode**, where both legs of one symbol are open at once. Legs are keyed by
 *   `(symbol, side)`, so they open and close independently. The suite runs one-way today
 *   (`ensureOneWayPositionMode`), so this is untested against a real hedge log.
 * - **The log is APPEND-ONLY across runs.** The same `(symbol, side)` opens and closes many
 *   times; each cycle is its own trip.
 */
export function buildExpectedRoundTrips(events: readonly GroundTruthEvent[]): GroundTruthFold {
  const trips: ExpectedRoundTrip[] = [];
  const unclosed: UnclosedLeg[] = [];
  let unreadable = 0;

  // Keyed `symbol|side` so hedge legs are independent and re-opens start a fresh trip.
  const open = new Map<string, OpenLeg>();
  // Timestamp of the last sample, so a transition can be bounded at its lower end.
  let previousTs: number | undefined;

  const ordered = [...events]
    .filter((event) => {
      const ok = event && typeof event.ts === 'number' && typeof event.symbol === 'string';
      if (!ok) unreadable += 1;
      return ok;
    })
    .sort((a, b) => a.ts - b.ts);

  for (const event of ordered) {
    // An event with no `positionsAfter` says nothing about state. Absent is not empty:
    // treating it as a flat account would close every open leg on a malformed line.
    if (!Array.isArray(event.positionsAfter)) {
      unreadable += 1;
      continue;
    }

    const observed = new Map<string, { side: PositionSide; contracts: number }>();
    for (const snapshot of event.positionsAfter) {
      const side = toSide(snapshot?.side);
      const contracts = Math.abs(Number(snapshot?.contracts));
      if (!side || !Number.isFinite(contracts) || contracts === 0) continue;
      observed.set(`${event.symbol}|${side}`, { side, contracts });
    }

    // Opens and increases.
    for (const [key, { side, contracts }] of observed) {
      const existing = open.get(key);
      if (existing) {
        existing.maxContracts = Math.max(existing.maxContracts, contracts);
        if (!existing.actions.includes(event.action)) existing.actions.push(event.action);
        continue;
      }
      open.set(key, {
        side,
        symbol: event.symbol,
        openedAtLower: previousTs ?? event.ts,
        openedAtUpper: event.ts,
        maxContracts: contracts,
        actions: [event.action],
      });
    }

    // Closes. A leg absent from this sample closed somewhere in (previousTs, event.ts].
    // Only legs for THIS symbol are considered: a sample is filtered to the test symbol,
    // so another symbol's absence is silence, not evidence of a close.
    for (const [key, leg] of [...open]) {
      if (leg.symbol !== event.symbol || observed.has(key)) continue;
      open.delete(key);
      trips.push({
        symbol: leg.symbol,
        side: leg.side,
        openedAtLower: leg.openedAtLower,
        openedAtUpper: leg.openedAtUpper,
        closedAtLower: previousTs ?? leg.openedAtUpper,
        closedAtUpper: event.ts,
        maxContracts: leg.maxContracts,
        actions: [...leg.actions, event.action],
      });
    }

    previousTs = event.ts;
  }

  for (const leg of open.values()) {
    unclosed.push({
      symbol: leg.symbol,
      side: leg.side,
      openedAtLower: leg.openedAtLower,
      openedAtUpper: leg.openedAtUpper,
      maxContracts: leg.maxContracts,
    });
  }

  return { trips, unclosed, unreadable };
}

export type GroundTruthMatch = {
  readonly trip: ExpectedRoundTrip;
  readonly record: PositionRecord;
};

export type GroundTruthGrade = {
  readonly matched: GroundTruthMatch[];
  /** Round trips the suite performed that the venue's history does not contain. FAILURES. */
  readonly missing: ExpectedRoundTrip[];
  /** Records with no expected trip. Reported for context — explicitly NOT failures. */
  readonly unmatchedRecords: PositionRecord[];
  /** Legs the log never saw close. Cannot be graded either way. */
  readonly unclosed: UnclosedLeg[];
  /**
   * True when two or more trips on the same `(symbol, side)` have overlapping match
   * windows, so which of them ends up unmatched is an artifact of assignment order.
   *
   * Measured on bybit: the suite ran three long→short flips inside five minutes and the
   * venue's history holds two long round trips, so exactly one trip is always unmatched —
   * but WHICH one moved between the `flip` cycle and the `flip-observed` cycle purely by
   * changing the tolerance from 1 minute to 10. **The count is the finding; the identity is
   * not.** Printing a specific trip without this flag invites someone to go and investigate
   * an arbitrary one.
   */
  readonly ambiguousAssignment: boolean;
  /**
   * Per-leg breakdown of `missing`: trips the suite ran on a `(symbol, side)` against
   * records matched to it.
   *
   * It sums to `missing.length` by construction — every trip lands in exactly one bucket —
   * so it is a PRESENTATION of the shortfall, not a second opinion about it. An earlier
   * draft claimed it was an independent "sound lower bound" that greedy matching could not
   * inflate; that was arithmetically false, and the honest fix was to make the matching
   * itself optimal (see `gradeAgainstGroundTruth`) rather than to keep a second number that
   * only looked more trustworthy.
   *
   * It earns its place because it is the form the finding is actually reported in:
   * "long DOGEUSDT: 4 run, 3 found" survives a tolerance sweep unchanged, while the
   * individual trip named as missing does not.
   */
  readonly deficits: LegDeficit[];
};

export type LegDeficit = {
  readonly symbol: string;
  readonly side: PositionSide;
  readonly expected: number;
  readonly matched: number;
  readonly deficit: number;
};

export type GradeOptions = {
  /**
   * Slack either side of the close interval.
   *
   * Needed because the two clocks are unrelated: `ts` is the BROWSER's clock stamped after
   * the action settled, `closedAt` is the VENUE's clock for the fill. Neither is
   * authoritative and the skew is not measurable from the data, so the tolerance has to be
   * wide enough to absorb it. Ten minutes is far wider than any observed skew and still far
   * narrower than the gap between the suite's own test cases.
   */
  readonly toleranceMs?: number;
  /**
   * Compares a record's symbol to the log's. Defaults to exact match; venues whose
   * canonical symbol differs from the e2e config's need to pass one.
   *
   * **Must be an equivalence relation.** The maximum-matching guarantee below rests on
   * compatibility being transitive — a "shares a prefix" style matcher is not, and under one
   * the assignment is still greedy-correct but no longer provably minimal. Nothing in the
   * harness passes a non-transitive matcher today; only the tests do, deliberately.
   */
  readonly symbolsMatch?: (recordSymbol: string, logSymbol: string) => boolean;
};

const DEFAULT_TOLERANCE_MS = 10 * 60 * 1000;

/**
 * Grades records against the fold.
 *
 * **Matching never consults PnL, size or price** — only `(symbol, side, close time)`.
 * Grading a venue's PnL against a figure derived from that same venue is the circularity
 * this whole mode exists to break, and `maxContracts` is the suite's own observation of the
 * venue's store rather than an independent measurement, so it cannot serve as a key either.
 *
 * **The asymmetry is deliberate.** A trip with no record is a failure: the suite proved the
 * position existed and closed, so history that omits it has lost a real event. A record
 * with no trip is NOT — these are shared accounts and the window can hold trading nobody
 * logged. Failing on those would make the grade unusable on any account a human touches.
 *
 * `closedAt` is the only anchor. `openedAt` is optional on `PositionRecord` and bybit
 * reports `opened == closed` on every row, so matching on open time would fail that venue
 * everywhere for a reason that has nothing to do with correctness.
 */
export function gradeAgainstGroundTruth(
  records: readonly PositionRecord[],
  fold: GroundTruthFold,
  options: GradeOptions = {},
): GroundTruthGrade {
  const tolerance = options.toleranceMs ?? DEFAULT_TOLERANCE_MS;
  const symbolsMatch = options.symbolsMatch ?? ((a: string, b: string) => a === b);

  const matched: GroundTruthMatch[] = [];
  const missing: ExpectedRoundTrip[] = [];
  const claimed = new Set<PositionRecord>();

  // MAXIMUM matching, not nearest-first — and the difference is not cosmetic.
  //
  // Nearest-midpoint greedy lets an earlier trip claim a record a later one needed, so it
  // can report two missing where a perfect assignment reports one. That would overstate
  // what the venue lost, which on this branch is the worse direction to be wrong in: the
  // whole mode exists to convict a venue of losing history.
  //
  // Records are points and trips are intervals, so the optimal assignment is the classic
  // one: take intervals by EARLIEST UPPER BOUND, and give each the earliest still-available
  // point inside it. Any point that fits a tighter-ending interval also fits a later-ending
  // one, so committing the earliest point to the earliest-ending interval never blocks a
  // match that was otherwise achievable. `missing.length` is therefore the true minimum
  // shortfall rather than an artifact of iteration order.
  const byDeadline = [...fold.trips].sort(
    (a, b) => a.closedAtUpper - b.closedAtUpper || a.closedAtLower - b.closedAtLower,
  );

  const eligible = records
    .filter((record) => typeof record.closedAt === 'number')
    .sort((a, b) => (a.closedAt as number) - (b.closedAt as number));

  for (const trip of byDeadline) {
    const lower = trip.closedAtLower - tolerance;
    const upper = trip.closedAtUpper + tolerance;

    let taken: PositionRecord | undefined;
    for (const record of eligible) {
      if (claimed.has(record)) continue;
      if (record.side !== trip.side) continue;
      if (!symbolsMatch(record.symbol, trip.symbol)) continue;

      const closedAt = record.closedAt as number;
      if (closedAt < lower) continue;
      // `eligible` is sorted ascending, so the first in-range record is the earliest one,
      // and anything past `upper` means nothing later can fit either.
      if (closedAt > upper) break;

      taken = record;
      break;
    }

    if (taken) {
      claimed.add(taken);
      matched.push({ trip, record: taken });
    } else {
      missing.push(trip);
    }
  }

  // Report in the log's own order, not deadline order — a reader comparing against the
  // jsonl should not have to reorder it mentally.
  missing.sort((a, b) => a.closedAtLower - b.closedAtLower);

  const deficits = new Map<string, { symbol: string; side: PositionSide; expected: number; matched: number }>();
  for (const trip of fold.trips) {
    const key = `${trip.symbol}|${trip.side}`;
    const entry = deficits.get(key) ?? { symbol: trip.symbol, side: trip.side, expected: 0, matched: 0 };
    entry.expected += 1;
    deficits.set(key, entry);
  }
  for (const hit of matched) {
    const entry = deficits.get(`${hit.trip.symbol}|${hit.trip.side}`);
    if (entry) entry.matched += 1;
  }

  return {
    matched,
    missing,
    unmatchedRecords: records.filter((record) => !claimed.has(record)),
    unclosed: fold.unclosed,
    ambiguousAssignment: hasOverlappingWindows(fold.trips, tolerance),
    deficits: [...deficits.values()]
      .map((entry) => ({ ...entry, deficit: entry.expected - entry.matched }))
      .filter((entry) => entry.deficit > 0),
  };
}

/**
 * Do two trips on the same leg compete for the same records?
 *
 * Only same `(symbol, side)` pairs can, since matching never crosses sides. Compared
 * pairwise rather than by sorting: the trip count per run is small, and a sort would have
 * to be stable across two interval ends rather than one.
 */
function hasOverlappingWindows(trips: readonly ExpectedRoundTrip[], tolerance: number): boolean {
  for (let i = 0; i < trips.length; i += 1) {
    for (let j = i + 1; j < trips.length; j += 1) {
      const a = trips[i];
      const b = trips[j];
      if (a.side !== b.side || a.symbol !== b.symbol) continue;
      if (
        a.closedAtLower - tolerance <= b.closedAtUpper + tolerance &&
        b.closedAtLower - tolerance <= a.closedAtUpper + tolerance
      ) {
        return true;
      }
    }
  }
  return false;
}
