import { describe, expect, it } from 'vitest';

import { formatPnl } from './formatPnl';

describe('formatPnl — abbreviated mode preserves the sign for negatives', () => {
  it('keeps the leading minus for plain abbreviated negatives', () => {
    expect(formatPnl(-1_500, { abbreviated: true })).toBe('-1.50k');
    expect(formatPnl(-2_000_000, { abbreviated: true })).toBe('-2.00M');
  });

  it('puts the currency prefix AFTER the minus sign', () => {
    expect(formatPnl(-1_500, { abbreviated: true, currency: '$' })).toBe('-$1.50k');
  });

  it('positive abbreviated values keep the same shape', () => {
    expect(formatPnl(1_500, { abbreviated: true })).toBe('1.50k');
    expect(formatPnl(1_500, { abbreviated: true, currency: '$' })).toBe('$1.50k');
  });

  it('signed:true forces a leading + on positives only', () => {
    expect(formatPnl(1_500, { abbreviated: true, signed: true })).toBe('+1.50k');
    expect(formatPnl(-1_500, { abbreviated: true, signed: true })).toBe('-1.50k');
  });

  it('falls through to safe-cex for the non-abbreviated path', () => {
    // The non-abbreviated branch is just a passthrough to safe-cex's
    // formatPnl. Sanity-check shape only — don't reimplement its rules
    // here.
    const out = formatPnl(123.456, { precision: 2 });
    expect(out).toMatch(/123\.4[56]/);
  });
});
