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
        message: 'Exchange engine stopped; command outcome is unknown. Reconcile before retrying',
        outcome: 'unknown',
      }),
    ).toBe(true);
  });

  it('falls back to text matching for unclassified errors', () => {
    expect(isUnknownOutcome({ message: 'socket hang up' })).toBe(true);
    expect(isUnknownOutcome({ message: 'Margin is insufficient.' })).toBe(false);
  });
});
