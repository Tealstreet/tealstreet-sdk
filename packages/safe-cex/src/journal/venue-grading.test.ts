import { describe, expect, it } from 'vitest';

import type { ExchangeName } from '../types';

import { getJournalVenueStatus, isIndependentlyReconciled, isJournalSupported } from './venue-grading';

/**
 * These assertions encode measured verdicts, not preferences. Each corresponds to a run in
 * `docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md` -> "STATE OF THE 11".
 */

describe('journal venue grading', () => {
  it('defaults an unknown venue to unsupported rather than showing an empty journal', () => {
    // The whole point. An empty tab reads as "you have no trades"; the truth is "we cannot
    // see this venue yet". Same failure as a zero no reader understood being printed as 0.
    expect(getJournalVenueStatus('someNewVenue' as ExchangeName).grade).toBe('unsupported');
    expect(isJournalSupported('someNewVenue' as ExchangeName)).toBe(false);
  });

  it('excludes phemex — it has no reachable history axis', () => {
    expect(isJournalSupported('phemex' as ExchangeName)).toBe(false);
  });

  it.each(['okx', 'binance', 'blofin', 'bitunix', 'bitget', 'bybit'])('grades %s', (venue) => {
    expect(getJournalVenueStatus(venue as ExchangeName).grade).toBe('graded');
    expect(isIndependentlyReconciled(venue as ExchangeName)).toBe(true);
  });

  it('keeps bybit GRADED but carries its merge caveat', () => {
    // A caveat must not downgrade the badge: bybit's totals genuinely reconcile. What it
    // loses is trade COUNT, and merging conserves value so no identity can see it.
    const status = getJournalVenueStatus('bybit' as ExchangeName);
    expect(status.grade).toBe('graded');
    expect(status.caveat).toMatch(/entry price/i);
    expect(status.reason).toBeUndefined();
  });

  it('marks kraken boundary — reconciled to its own precision floor, not beyond', () => {
    const status = getJournalVenueStatus('kraken' as ExchangeName);
    expect(status.grade).toBe('boundary');
    // Still independently reconciled: the residual is the venue's quantization, not a gap.
    expect(isIndependentlyReconciled('kraken' as ExchangeName)).toBe(true);
  });

  it.each(['paradex', 'hyperliquid', 'woo'])('shows %s ungraded WITH a reason, never silently', (venue) => {
    const status = getJournalVenueStatus(venue as ExchangeName);
    expect(status.grade).toBe('ungraded');
    // Rendering an ungraded venue without saying why is the hollow-verdict shape this whole
    // epic exists to delete, so the reason is asserted rather than assumed present.
    expect(status.reason).toBeTruthy();
    expect(isJournalSupported(venue as ExchangeName)).toBe(true);
    expect(isIndependentlyReconciled(venue as ExchangeName)).toBe(false);
  });

  it('gives every ungraded venue a DISTINCT reason — three venues refuse for three reasons', () => {
    const reasons = ['paradex', 'hyperliquid', 'woo'].map(
      (v) => getJournalVenueStatus(v as ExchangeName).reason,
    );
    expect(new Set(reasons).size).toBe(3);
  });

  it('writes reasons for a trader, not for the adapter author', () => {
    // No endpoint names, no internal identifiers. A reason a user cannot act on is decoration.
    for (const venue of ['paradex', 'hyperliquid', 'woo', 'kraken']) {
      const reason = getJournalVenueStatus(venue as ExchangeName).reason ?? '';
      expect(reason).not.toMatch(/ENDPOINTS\.|fetchIncomeHistory|\/v\d|_PERP/);
    }
  });
});
