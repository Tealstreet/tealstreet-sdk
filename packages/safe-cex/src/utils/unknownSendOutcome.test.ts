// Rule 5 (Sam, 2026-11-20): display wording only; outcome and no-retry checks stay.
import { describe, expect, it } from 'vitest';

import { isUnknownOutcome } from './unknownSendOutcome';

describe('isUnknownOutcome', () => {
  it('trusts a classified outcome over the message text', () => {
    expect(isUnknownOutcome({ message: 'Coinbase write outcome requires reconciliation: HTTP 503', outcome: 'unknown' })).toBe(true);
    expect(isUnknownOutcome({ message: 'Timeout waiting for response', outcome: 'rejected' })).toBe(false);
    expect(isUnknownOutcome({ message: 'connection reset', outcome: 'notSent' })).toBe(false);
  });

  it('treats host interruption markers as unknown even when their text does not say so', () => {
    expect(
      isUnknownOutcome({
        message: 'Trading was interrupted before this action was confirmed. Check your open orders before trying again.',
        outcome: 'unknown',
      }),
    ).toBe(true);
  });

  it('falls back to text matching for unclassified errors', () => {
    expect(isUnknownOutcome({ message: 'socket hang up' })).toBe(true);
    expect(isUnknownOutcome({ message: 'Margin is insufficient.' })).toBe(false);
  });
});
