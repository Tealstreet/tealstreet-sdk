import { describe, expect, it } from 'vitest';

import { splitTimeRange } from '../../utils/splitTimeRange';

const DAY = 24 * 60 * 60 * 1000;
const WEEK = 7 * DAY;

describe('splitTimeRange', () => {
  it('returns one window when the range already fits', () => {
    expect(splitTimeRange(1_000, 1_000 + DAY, WEEK)).toEqual([{ startTime: 1_000, endTime: 1_000 + DAY }]);
  });

  it('covers a 30-day range with 7-day windows', () => {
    const from = 1_000_000;
    const to = from + 30 * DAY;
    const windows = splitTimeRange(from, to, WEEK);

    expect(windows[0].startTime).toBe(from);
    expect(windows[windows.length - 1].endTime).toBe(to);
    // Every window must be within the cap, or the venue rejects the request outright and
    // the caller's catch turns it into an empty list.
    for (const w of windows) expect(w.endTime - w.startTime).toBeLessThanOrEqual(WEEK);
  });

  it('never shares a boundary millisecond between windows', () => {
    // Sharing it returns the boundary row twice, and deduping is NOT the fix — bybit's
    // tranId is not unique, so a Map keyed on it deletes real value.
    const windows = splitTimeRange(0, 30 * DAY, WEEK);
    for (let i = 1; i < windows.length; i += 1) {
      expect(windows[i].startTime).toBe(windows[i - 1].endTime + 1);
    }
  });

  it('leaves no gap between windows', () => {
    // A gap silently drops every row inside it — the same invisible-loss shape as the
    // unchunked request this helper exists to prevent.
    const from = 500;
    const to = from + 23 * DAY + 12345;
    const windows = splitTimeRange(from, to, WEEK);
    let covered = 0;
    for (const w of windows) covered += w.endTime - w.startTime + 1;
    expect(covered).toBe(to - from + 1);
  });

  it('handles a range that is an exact multiple of the cap', () => {
    const windows = splitTimeRange(0, WEEK, WEEK);
    expect(windows).toEqual([{ startTime: 0, endTime: WEEK }]);
  });

  it('handles a zero-length range', () => {
    expect(splitTimeRange(42, 42, WEEK)).toEqual([{ startTime: 42, endTime: 42 }]);
  });

  it('returns nothing for an inverted or non-finite range rather than looping', () => {
    expect(splitTimeRange(100, 50, WEEK)).toEqual([]);
    expect(splitTimeRange(NaN, 50, WEEK)).toEqual([]);
  });

  it('treats a non-positive cap as no cap instead of hanging', () => {
    // A zero or negative span would never advance `start`, so the loop would run forever.
    expect(splitTimeRange(0, DAY, 0)).toEqual([{ startTime: 0, endTime: DAY }]);
    expect(splitTimeRange(0, DAY, -1)).toEqual([{ startTime: 0, endTime: DAY }]);
  });
});
