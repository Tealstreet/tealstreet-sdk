import { describe, expect, it } from 'vitest';

import { computeRegion } from './scroll-region';

describe('computeRegion', () => {
  it('returns null when nothing is reserved (full-screen)', () => {
    expect(computeRegion(24, 0, 0)).toBeNull();
  });

  it('reserves the top (status bar: 2 rows → region rows 3..N)', () => {
    expect(computeRegion(24, 2, 0)).toEqual([3, 24]);
  });

  it('reserves the bottom (1-row footer → region rows 1..N-1)', () => {
    expect(computeRegion(24, 0, 1)).toEqual([1, 23]);
  });

  it('reserves both top and bottom simultaneously', () => {
    expect(computeRegion(24, 2, 1)).toEqual([3, 23]);
  });

  it('still yields a region with exactly one scrollable row', () => {
    expect(computeRegion(4, 2, 1)).toEqual([3, 3]);
  });

  it('returns null when the terminal is too short to leave any scrollable row', () => {
    // top=2, bottom=1 needs at least 4 rows (1,2 top + 1 scroll + 1 footer).
    expect(computeRegion(3, 2, 1)).toBeNull();
    expect(computeRegion(2, 2, 0)).toBeNull();
  });

  it('treats negative reservations as zero', () => {
    expect(computeRegion(24, -5, -5)).toBeNull();
    expect(computeRegion(24, -5, 1)).toEqual([1, 23]);
  });
});
