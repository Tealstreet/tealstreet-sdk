import { expect, it } from 'vitest';

import { assertBetaVenue, BETA_VENUES } from '../../exchanges/rust/betaVenues';
import rules from '../../exchanges/rust/construction-rules.json';

it('native admission keeps the SDK OUT venues closed', () => {
  const refused = Object.entries(rules.venues)
    .filter(([, rule]) => 'betaAdmitted' in rule && rule.betaAdmitted === false)
    .map(([name]) => name);
  expect(refused).toEqual(['bitrue', 'polymarket']);
  for (const name of refused) {
    expect(BETA_VENUES.some((venue) => venue === name)).toBe(false);
    expect(() => assertBetaVenue(name)).toThrowError(
      expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }),
    );
  }
});
