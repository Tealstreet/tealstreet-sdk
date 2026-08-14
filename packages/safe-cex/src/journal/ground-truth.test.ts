import type { GroundTruthEvent } from './ground-truth';
import type { PositionRecord } from './position.types';

import { describe, expect, it } from 'vitest';

import { PositionSide } from '../types';
import { buildExpectedRoundTrips, gradeAgainstGroundTruth } from './ground-truth';

/**
 * Fixtures are transcribed from REAL logs produced by the nightly suite
 * (`apps/web/cypress/logs/{bybit,okx}-ground-truth.jsonl`, gitignored), not invented.
 * That matters: the first version of the fold was verb-driven and passed hand-written
 * fixtures, then met bybit's actual log — which contains three `market-order -> flip`
 * pairs whose short leg vanishes with no `close` event, because those runs were aborted
 * and the suite's own cleanup flattened the account without recording it.
 */

const event = (
  ts: number,
  action: string,
  positionsAfter: Array<{ side: string; contracts: number }>,
  symbol = 'DOGEUSDT',
): GroundTruthEvent => ({ ts, symbol, action, positionsAfter, exchange: 'bybit' });

const long = (contracts: number) => [{ side: 'long', contracts }];
const short = (contracts: number) => [{ side: 'short', contracts }];
const flat: Array<{ side: string; contracts: number }> = [];

const record = (over: Partial<PositionRecord> & { closedAt: number; side: PositionSide }): PositionRecord => ({
  key: `k-${over.closedAt}-${over.side}`,
  account: 'test',
  symbol: 'DOGEUSDT',
  settleCurrency: 'USDT',
  positionSide: 'both',
  state: 'closed',
  closedSize: 141,
  avgEntryPrice: 0.0707,
  realizedPnl: -0.0014,
  fees: 0.0098,
  funding: 0,
  fundingAttributed: true,
  provenance: 'reported',
  sourceAxis: 'closed-positions',
  closeCount: 1,
  ...over,
});

describe('buildExpectedRoundTrips', () => {
  it('folds a plain open and close into one round trip', () => {
    const fold = buildExpectedRoundTrips([event(1_000, 'market-order', long(141)), event(5_000, 'close', flat)]);

    expect(fold.trips).toHaveLength(1);
    expect(fold.unclosed).toHaveLength(0);
    expect(fold.trips[0]).toMatchObject({
      symbol: 'DOGEUSDT',
      side: PositionSide.Long,
      closedAtLower: 1_000,
      closedAtUpper: 5_000,
      maxContracts: 141,
    });
  });

  it('reads a flip as closing one leg and opening the other', () => {
    const fold = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      event(2_000, 'flip', short(142)),
      event(3_000, 'close', flat),
    ]);

    expect(fold.trips.map((t) => t.side)).toEqual([PositionSide.Long, PositionSide.Short]);
    expect(fold.trips[0]).toMatchObject({ closedAtLower: 1_000, closedAtUpper: 2_000 });
    expect(fold.trips[1]).toMatchObject({ closedAtLower: 2_000, closedAtUpper: 3_000 });
  });

  it('absorbs flip-observed, which repeats the state the flip is about to report', () => {
    // Real bybit lines: flip-observed at ...722713 and flip at ...722729, 16ms apart,
    // carrying identical positionsAfter. A verb-driven fold sees two closes.
    const fold = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      event(2_000, 'flip-observed', short(142)),
      event(2_016, 'flip', short(142)),
      event(3_000, 'close', flat),
    ]);

    expect(fold.trips).toHaveLength(2);
    expect(fold.trips[1].side).toBe(PositionSide.Short);
  });

  it('keeps a partial-close ladder as ONE trip, not one per partial', () => {
    const fold = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(423)),
      event(2_000, 'partial-close', long(282)),
      event(3_000, 'partial-close', long(141)),
      event(4_000, 'close', flat),
    ]);

    expect(fold.trips).toHaveLength(1);
    // The peak, not the last observed size.
    expect(fold.trips[0].maxContracts).toBe(423);
    expect(fold.trips[0].actions).toContain('partial-close');
  });

  it('closes a leg that vanishes with no close event, and widens the interval to say so', () => {
    // The aborted-rerun case, straight out of the bybit log.
    const fold = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      event(2_000, 'flip', short(142)),
      event(120_000, 'market-order', long(141)),
      event(121_000, 'close', flat),
    ]);

    expect(fold.trips).toHaveLength(3);
    const orphan = fold.trips[1];
    expect(orphan.side).toBe(PositionSide.Short);
    // Bounded only by the two samples either side — honest about not knowing when.
    expect(orphan.closedAtLower).toBe(2_000);
    expect(orphan.closedAtUpper).toBe(120_000);
    expect(fold.unclosed).toHaveLength(0);
  });

  it('reports a still-open leg as unclosed rather than as a round trip', () => {
    const fold = buildExpectedRoundTrips([event(1_000, 'market-order', long(141))]);

    expect(fold.trips).toHaveLength(0);
    expect(fold.unclosed).toEqual([expect.objectContaining({ side: PositionSide.Long, maxContracts: 141 })]);
  });

  it('keeps hedge legs independent', () => {
    const fold = buildExpectedRoundTrips([
      event(1_000, 'market-order', [
        { side: 'long', contracts: 100 },
        { side: 'short', contracts: 50 },
      ]),
      event(2_000, 'partial-close', [{ side: 'long', contracts: 100 }]),
      event(3_000, 'close', flat),
    ]);

    expect(fold.trips).toHaveLength(2);
    expect(fold.trips[0]).toMatchObject({ side: PositionSide.Short, closedAtUpper: 2_000 });
    expect(fold.trips[1]).toMatchObject({ side: PositionSide.Long, closedAtUpper: 3_000 });
  });

  it('treats a missing positionsAfter as unreadable, NOT as a flat account', () => {
    const fold = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      { ts: 2_000, symbol: 'DOGEUSDT', action: 'close' },
      event(3_000, 'close', flat),
    ]);

    // The malformed line must not close the leg — otherwise one bad write invents a trip.
    expect(fold.unreadable).toBe(1);
    expect(fold.trips).toHaveLength(1);
    expect(fold.trips[0].closedAtUpper).toBe(3_000);
  });

  it('sorts by timestamp and survives an empty log', () => {
    expect(buildExpectedRoundTrips([]).trips).toEqual([]);

    const fold = buildExpectedRoundTrips([event(5_000, 'close', flat), event(1_000, 'market-order', long(141))]);
    expect(fold.trips).toHaveLength(1);
  });

  it('ignores another symbol going absent', () => {
    const fold = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141), 'DOGEUSDT'),
      event(2_000, 'market-order', long(10), 'BTCUSDT'),
      event(3_000, 'close', flat, 'BTCUSDT'),
    ]);

    // DOGE never appears in the BTC samples; that silence is not a close.
    expect(fold.trips).toHaveLength(1);
    expect(fold.trips[0].symbol).toBe('BTCUSDT');
    expect(fold.unclosed[0].symbol).toBe('DOGEUSDT');
  });
});

describe('gradeAgainstGroundTruth', () => {
  const fold = buildExpectedRoundTrips([event(1_000, 'market-order', long(141)), event(5_000, 'close', flat)]);

  it('matches a record whose close lands inside the interval', () => {
    const hit = record({ closedAt: 4_000, side: PositionSide.Long });
    const grade = gradeAgainstGroundTruth([hit], fold);

    expect(grade.matched).toHaveLength(1);
    expect(grade.missing).toHaveLength(0);
    expect(grade.unmatchedRecords).toHaveLength(0);
  });

  it('reports a trip with no record as MISSING — the venue lost a real event', () => {
    const grade = gradeAgainstGroundTruth([], fold);
    expect(grade.missing).toHaveLength(1);
  });

  it('does NOT fail a record with no expected trip', () => {
    // Shared accounts carry trading nobody logged; failing on it makes the mode unusable.
    const stranger = record({ closedAt: 9_000_000, side: PositionSide.Short });
    const grade = gradeAgainstGroundTruth([stranger], fold);

    expect(grade.unmatchedRecords).toEqual([stranger]);
    expect(grade.missing).toHaveLength(1);
    expect(grade.matched).toHaveLength(0);
  });

  it('never matches across sides', () => {
    const wrongSide = record({ closedAt: 4_000, side: PositionSide.Short });
    expect(gradeAgainstGroundTruth([wrongSide], fold).missing).toHaveLength(1);
  });

  it('absorbs browser-vs-venue clock skew up to the tolerance and no further', () => {
    const skewed = record({ closedAt: 5_000 + 9 * 60 * 1000, side: PositionSide.Long });
    expect(gradeAgainstGroundTruth([skewed], fold).matched).toHaveLength(1);

    const tooFar = record({ closedAt: 5_000 + 11 * 60 * 1000, side: PositionSide.Long });
    expect(gradeAgainstGroundTruth([tooFar], fold).matched).toHaveLength(0);
  });

  it('will not bind two trips on the same leg to one record', () => {
    const twoCycles = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      event(2_000, 'close', flat),
      event(3_000, 'market-order', long(141)),
      event(4_000, 'close', flat),
    ]);
    const only = record({ closedAt: 2_000, side: PositionSide.Long });
    const grade = gradeAgainstGroundTruth([only], twoCycles);

    expect(grade.matched).toHaveLength(1);
    expect(grade.missing).toHaveLength(1);
  });

  it('skips a record carrying no closedAt rather than treating it as time zero', () => {
    const openRecord = { ...record({ closedAt: 4_000, side: PositionSide.Long }), closedAt: undefined };
    expect(gradeAgainstGroundTruth([openRecord], fold).missing).toHaveLength(1);
  });

  it('honours a venue-specific symbol comparison', () => {
    const dashed = record({ closedAt: 4_000, side: PositionSide.Long, symbol: 'DOGE-USDT-SWAP' });
    const grade = gradeAgainstGroundTruth([dashed], fold, {
      symbolsMatch: (a, b) => a.replace(/[-_]/g, '').startsWith(b.replace(/[-_]/g, '').slice(0, 4)),
    });

    expect(grade.matched).toHaveLength(1);
  });

  it('flags overlapping same-leg trips, because then only the COUNT is meaningful', () => {
    // The real bybit shape: three long->short flips inside five minutes. The venue holds
    // two long round trips, so exactly one trip is always unmatched — but which one moved
    // between the flip and flip-observed cycles purely by widening the tolerance.
    const overlapping = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      event(2_000, 'flip', short(142)),
      event(60_000, 'market-order', long(141)),
      event(61_000, 'flip', short(142)),
      event(120_000, 'close', flat),
    ]);
    const single = [record({ closedAt: 2_000, side: PositionSide.Long })];
    const grade = gradeAgainstGroundTruth(single, overlapping);

    expect(grade.ambiguousAssignment).toBe(true);
    // Two long trips, one long record: the DEFICIT is one, which is the durable fact. The
    // shorts are unmatched too here simply because no short record was supplied.
    expect(grade.missing.filter((trip) => trip.side === PositionSide.Long)).toHaveLength(1);
  });

  it('finds the MAXIMUM matching — a nearest-first greedy would overstate the loss here', () => {
    // The counterexample that killed the first implementation. Nearest-midpoint lets the
    // earlier trip claim the record the later one needed, reporting a loss the venue did
    // not incur. Overstating what a venue lost is the worse direction for this mode to be
    // wrong in, since convicting the venue is its entire purpose.
    const twoTrips = buildExpectedRoundTrips([
      event(0, 'market-order', long(141)),
      event(1_000, 'close', flat),
      event(2_000, 'market-order', long(141)),
      event(2_000, 'close', flat),
    ]);

    // One record sits before both windows' lower bound but inside tolerance; the other
    // lands exactly on the second trip's close. A perfect assignment matches both.
    const both = [
      record({ closedAt: -1_400, side: PositionSide.Long }),
      record({ closedAt: 2_000, side: PositionSide.Long }),
    ];

    const grade = gradeAgainstGroundTruth(both, twoTrips, { toleranceMs: 1_500 });

    expect(grade.matched).toHaveLength(2);
    expect(grade.missing).toHaveLength(0);
    expect(grade.deficits).toEqual([]);
  });

  it('does not stop scanning at a wrong-side record that sorts before a usable one', () => {
    // The scan breaks out on the first record past the window's upper bound, relying on the
    // candidate list being globally sorted by close time. A wrong-SIDE record must take the
    // `continue` path instead, or a usable later record of the right side is never reached.
    const shortFirst = [
      record({ closedAt: 3_500, side: PositionSide.Short }),
      record({ closedAt: 4_000, side: PositionSide.Long }),
    ];

    expect(gradeAgainstGroundTruth(shortFirst, fold).matched).toHaveLength(1);
    expect(gradeAgainstGroundTruth(shortFirst, fold).missing).toHaveLength(0);
  });

  it('reports a per-leg breakdown of the shortfall', () => {
    const twoLongs = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      event(2_000, 'close', flat),
      event(60_000, 'market-order', long(141)),
      event(61_000, 'close', flat),
    ]);
    const grade = gradeAgainstGroundTruth([record({ closedAt: 2_000, side: PositionSide.Long })], twoLongs);

    expect(grade.deficits).toEqual([
      { symbol: 'DOGEUSDT', side: PositionSide.Long, expected: 2, matched: 1, deficit: 1 },
    ]);
  });

  it('reports no deficit when every leg reconciles', () => {
    const hit = record({ closedAt: 4_000, side: PositionSide.Long });
    expect(gradeAgainstGroundTruth([hit], fold).deficits).toEqual([]);
  });

  it('does not flag ambiguity when the windows are comfortably apart', () => {
    const spaced = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      event(2_000, 'close', flat),
      event(60 * 60 * 1000, 'market-order', long(141)),
      event(60 * 60 * 1000 + 1_000, 'close', flat),
    ]);

    expect(gradeAgainstGroundTruth([], spaced, { toleranceMs: 60_000 }).ambiguousAssignment).toBe(false);
  });

  it('never calls opposite sides ambiguous — matching cannot cross them', () => {
    const flip = buildExpectedRoundTrips([
      event(1_000, 'market-order', long(141)),
      event(2_000, 'flip', short(142)),
      event(3_000, 'close', flat),
    ]);

    expect(gradeAgainstGroundTruth([], flip).ambiguousAssignment).toBe(false);
  });

  it('carries unclosed legs through so they are never counted as missing', () => {
    const openLeg = buildExpectedRoundTrips([event(1_000, 'market-order', long(141))]);
    const grade = gradeAgainstGroundTruth([], openLeg);

    expect(grade.missing).toHaveLength(0);
    expect(grade.unclosed).toHaveLength(1);
  });
});
