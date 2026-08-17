import type { ExchangeName } from '../types';

import { describe, expect, it } from 'vitest';

import { getAxisStatus, isAxisIndependentlyReconciled } from './axis-grading';

const ex = (name: string) => name as ExchangeName;

describe('getAxisStatus', () => {
  describe('per-axis rather than per-venue', () => {
    /**
     * The reason this model exists. bybit's POSITION axis was graded against
     * income; its FILLS axis was never independently checked. Stamping the venue
     * grade onto a fill row would assert something nobody measured.
     */
    it('does not transfer a venue grade to an axis nobody checked', () => {
      expect(getAxisStatus(ex('bybit'), 'positions').grade).toBe('graded');
      expect(getAxisStatus(ex('bybit'), 'fills').grade).toBe('unmeasured');
    });

    it('grades the order axis on bybit, which the leg join actually used', () => {
      // Without that join longs and shorts merged and the count came out short:
      // 127 records against the gate's 150.
      expect(getAxisStatus(ex('bybit'), 'orders').grade).toBe('graded');
    });

    it('propagates woo as ungraded on the order axis its fold actually reads', () => {
      // woo folds from ORDERS — the only route carrying realized PnL, total fee
      // and position side on one row; both its fills axes report null PnL. The
      // axis is real and its records are real, but woo publishes no gradeable
      // counterparty (realized PnL arrives as a daily account total), so
      // `ungraded` is the correct verdict and must NOT read as `unmeasured`:
      // we looked, and nothing exists to check it against.
      expect(getAxisStatus(ex('woo'), 'orders').grade).toBe('ungraded');
      expect(getAxisStatus(ex('woo'), 'fills').grade).toBe('unmeasured');
    });

    it('reports an axis the venue cannot serve as unsupported, not unmeasured', () => {
      // "We cannot see this" and "we have not looked" are different claims.
      expect(getAxisStatus(ex('asterdex'), 'positions').grade).toBe('unsupported');
      expect(getAxisStatus(ex('asterdex'), 'fills').grade).toBe('unmeasured');
    });

    it('treats an entirely unknown venue as unsupported on every axis', () => {
      for (const axis of ['positions', 'fills', 'orders', 'income'] as const) {
        expect(getAxisStatus(ex('definitely-not-a-venue'), axis).grade).toBe('unsupported');
      }
    });
  });

  describe('provenance is a separate dimension from the grade', () => {
    /**
     * The distinction this state exists for. `history-audit` dispatched
     * `exchange[venue.historyMethod]`, and binance, woo and bitrue were
     * configured as `fetchHistoricalTrades` — a method deleted 2026-08-16. Their
     * probes now read `fetchHistoricalFills`. The verdict is expected to hold, but
     * "expected to hold" is not "measured".
     */
    it('marks a venue whose probe changed method as pending revalidation', () => {
      expect(getAxisStatus(ex('binance'), 'positions').provenance).toBe('pending-revalidation');
      expect(getAxisStatus(ex('woo'), 'orders').provenance).toBe('pending-revalidation');
    });

    it('keeps the grade itself intact — the verdict stands, its evidence is stale', () => {
      expect(getAxisStatus(ex('binance'), 'positions').grade).toBe('graded');
    });

    it('leaves every other venue current', () => {
      expect(getAxisStatus(ex('okx'), 'positions').provenance).toBe('current');
      expect(getAxisStatus(ex('bybit'), 'positions').provenance).toBe('current');
      expect(getAxisStatus(ex('bitunix'), 'positions').provenance).toBe('current');
    });

    /**
     * The collapse this model refuses. An unmeasured axis has no verdict, so
     * there is nothing for a re-run to confirm — marking it pending would invent
     * a measurement that was never taken.
     */
    it('never marks an unmeasured axis pending, even on a pending venue', () => {
      const status = getAxisStatus(ex('bitrue'), 'fills');
      expect(status.grade).toBe('unmeasured');
      expect(status.provenance).toBe('current');
    });
  });

  describe('caveats ride the axis', () => {
    // bybit's PnL sums are safe to aggregate; its trade COUNTS are not, because
    // two round trips reopening at an identical entry price merge into one.
    it('carries the venue caveat onto its graded axis', () => {
      expect(getAxisStatus(ex('bybit'), 'positions').caveat?.kind).toBe('undercounts-trades');
    });

    it('does not attach a caveat to an unmeasured axis', () => {
      expect(getAxisStatus(ex('bybit'), 'fills').caveat).toBeUndefined();
    });
  });
});

describe('isAxisIndependentlyReconciled', () => {
  it('is true for a graded axis whose provenance is current', () => {
    expect(isAxisIndependentlyReconciled(ex('okx'), 'positions')).toBe(true);
  });

  /**
   * A consumer asking "is this independently verified" is asking about EVIDENCE,
   * and the evidence here points at an instrument that has since been changed.
   * Answering yes would be the epic's own failure — presenting a verdict as
   * confirmed by a measurement nobody has actually re-run.
   */
  it('is false while provenance is pending, even though the grade is graded', () => {
    expect(getAxisStatus(ex('binance'), 'positions').grade).toBe('graded');
    expect(isAxisIndependentlyReconciled(ex('binance'), 'positions')).toBe(false);
  });

  it('is false for unmeasured and unsupported axes', () => {
    expect(isAxisIndependentlyReconciled(ex('bybit'), 'fills')).toBe(false);
    expect(isAxisIndependentlyReconciled(ex('asterdex'), 'positions')).toBe(false);
  });

  it('is true at boundary — reconciled to the venue precision floor still counts', () => {
    expect(isAxisIndependentlyReconciled(ex('kraken'), 'positions')).toBe(true);
  });
});
