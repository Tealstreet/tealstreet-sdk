import { describe, expect, it } from 'vitest';

import { checkWindowContainment, describeWindowContainment } from './window-containment';

/**
 * The sixth invariant, driven by the case that produced it.
 *
 * The fixture window and the offending timestamps are the real ones measured against WOO
 * (sheet row 28): a January-2020 request that came back holding August-2026 fills.
 */

const JAN_2020_FROM = Date.parse('2020-01-01T00:00:00Z');
const JAN_2020_TO = Date.parse('2020-02-01T00:00:00Z');
const AUG_2026_FILL = Date.parse('2026-08-12T10:19:53Z');

describe('checkWindowContainment', () => {
  it('catches the failure every other invariant passes: right rows, wrong window', () => {
    // These rows are internally perfect — real fills, correct signs, plausible fees. The
    // ONLY thing wrong with them is that nobody asked for August 2026.
    const report = checkWindowContainment([{ timestampMs: AUG_2026_FILL, label: 'PERP_ETH_USDT' }], {
      from: JAN_2020_FROM,
      to: JAN_2020_TO,
    });

    expect(report.withinWindow).toBe(0);
    expect(report.outside).toHaveLength(1);
    expect(report.outside[0]).toContain('AFTER to');
    // ~6.5 years. The magnitude is the evidence that the parameter was discarded rather
    // than that a boundary was handled differently.
    expect(report.worstOverrunMs).toBeGreaterThan(2000 * 86_400_000);
  });

  it('names a discarded window separately from a boundary disagreement', () => {
    const ignored = checkWindowContainment([{ timestampMs: AUG_2026_FILL }], {
      from: JAN_2020_FROM,
      to: JAN_2020_TO,
    });
    expect(describeWindowContainment(ignored)).toContain('WINDOW APPEARS IGNORED');

    // One hour past the bound is a real overrun worth reporting, but it is NOT the
    // "venue ignored the parameter" signal and must not be labelled as one.
    const nearBoundary = checkWindowContainment([{ timestampMs: JAN_2020_TO + 3_600_000 }], {
      from: JAN_2020_FROM,
      to: JAN_2020_TO,
    });
    expect(nearBoundary.outside).toHaveLength(1);
    expect(describeWindowContainment(nearBoundary)).not.toContain('WINDOW APPEARS IGNORED');
  });

  it('tolerates an inclusive/exclusive boundary, which is a convention and not a defect', () => {
    const report = checkWindowContainment([{ timestampMs: JAN_2020_FROM - 500 }, { timestampMs: JAN_2020_TO + 500 }], {
      from: JAN_2020_FROM,
      to: JAN_2020_TO,
    });
    expect(report.withinWindow).toBe(2);
    expect(report.outside).toHaveLength(0);
  });

  it('reports UNCHECKED, never consistent, when no window was requested', () => {
    // An axis with no window semantics cannot fire this check. Counting its rows as
    // contained would make an unverifiable axis read green — the exact failure this whole
    // epic keeps rediscovering.
    const report = checkWindowContainment([{ timestampMs: AUG_2026_FILL }], {});
    expect(report.withinWindow).toBe(0);
    expect(report.unchecked).toBe(1);
    expect(describeWindowContainment(report)).toContain('UNCHECKED');
  });

  it('reports UNCHECKED for a degenerate window rather than failing every row', () => {
    const report = checkWindowContainment([{ timestampMs: AUG_2026_FILL }], {
      from: JAN_2020_TO,
      to: JAN_2020_FROM,
    });
    expect(report.unchecked).toBe(1);
    expect(report.outside).toHaveLength(0);
  });

  it('counts a row with no timestamp as unchecked, not as contained', () => {
    const report = checkWindowContainment([{ timestampMs: undefined }, { timestampMs: 0 }, { timestampMs: NaN }], {
      from: JAN_2020_FROM,
      to: JAN_2020_TO,
    });
    expect(report.unchecked).toBe(3);
    expect(report.withinWindow).toBe(0);
  });

  it('accepts a straddler — opened before the window, closed inside it', () => {
    // Only the timestamp the venue filtered on is compared. A position opened in 2019 and
    // closed in January 2020 is legitimately in range; treating it as an overrun would
    // make the check fire constantly on every venue and get it switched off.
    const closedInside = Date.parse('2020-01-15T00:00:00Z');
    const report = checkWindowContainment([{ timestampMs: closedInside }], {
      from: JAN_2020_FROM,
      to: JAN_2020_TO,
    });
    expect(report.withinWindow).toBe(1);
    expect(report.outside).toHaveLength(0);
  });

  it('summarises a clean axis without claiming more than it checked', () => {
    const report = checkWindowContainment([{ timestampMs: JAN_2020_FROM + 1000 }, { timestampMs: undefined }], {
      from: JAN_2020_FROM,
      to: JAN_2020_TO,
    });
    const line = describeWindowContainment(report);
    expect(line).toContain('1/1 inside the requested window');
    expect(line).toContain('1 unchecked');
  });
});
