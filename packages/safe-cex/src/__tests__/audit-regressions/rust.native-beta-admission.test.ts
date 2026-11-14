import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, it } from 'vitest';

import { assertBetaVenue, BETA_VENUES } from '../../exchanges/rust/betaVenues';
import rules from '../../exchanges/rust/construction-rules.json';

it('native admission matches the full SDK roster for every supported name', () => {
  const source = readFileSync(resolve(__dirname, '../../../../../sdk-rs/registry/src/lib.rs'), 'utf8');
  const supported = source.split('pub fn supported(venue: &str) -> bool {')[1].split('\n}')[0];
  const names = [...supported.matchAll(/^\s+((?:"[a-z-]+"(?: \| )?)+) => true,/gm)]
    .flatMap((branch) => [...branch[1].matchAll(/"([a-z-]+)"/g)].map((name) => name[1]));
  expect(names.length).toBeGreaterThanOrEqual(BETA_VENUES.length);
  const table = rules.venues as Record<string, { betaAdmitted?: boolean }>;
  const refused: string[] = [];
  for (const name of names) {
    const admitted = BETA_VENUES.some((venue) => venue === name);
    expect.soft(table[name]?.betaAdmitted ?? true, name).toBe(admitted);
    if (!admitted) {
      refused.push(name);
      expect(() => assertBetaVenue(name)).toThrowError(
        expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }),
      );
    }
  }
  // restored by #3502; #3317 dropped a live venue without a product decision.
  expect(refused.sort()).toEqual(['orderly', 'polymarket', 'txflow', 'weex']);
});
