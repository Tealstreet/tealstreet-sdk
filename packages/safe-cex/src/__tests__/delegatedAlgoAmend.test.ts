import { describe, expect, it } from 'vitest';

import { isDelegatedAction } from '../delegatedActions';

describe('typed running algo amendments', () => {
  it('admits only the conservative shared runtime amendment fields', () => {
    for (const [op, update] of [
      ['chaser:amend', { limitPrice: 100, offset: 0.5, maxAttempts: 20 }],
      ['twap:amend', { intervalMs: 1000, remainingSlices: 3 }],
    ]) {
      expect(isDelegatedAction({ kind: 'algo', op, data: { id: 'job', update } })).toBe(true);
    }
  });

  it('refuses immutable identities, total quantity, empty and malformed updates', () => {
    for (const op of ['chaser:amend', 'twap:amend']) {
      for (const update of [
        { size: 2 },
        { side: 'sell' },
        { symbol: 'other' },
        {},
        { intervalMs: -1 },
        { maxAttempts: '20' },
      ]) {
        expect(isDelegatedAction({ kind: 'algo', op, data: { id: 'job', update } })).toBe(false);
      }
    }
  });
});
