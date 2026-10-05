import { CORE_READ_MANIFEST } from '@tealstreet/script-api-core/src/runtime/read-manifest';
import { describe, expect, it } from 'vitest';

import { READ_METHOD_NAMES } from './capabilityNames';

/**
 * `READ_METHOD_NAMES` is a hand-maintained `as const` list and has to stay one
 * — deriving it from the manifest would widen it to `string[]` and destroy the
 * compile-time drift assertions in `capabilityNames.ts` that are its whole
 * purpose. Tying the two together HERE gets the coverage without losing the
 * literal types, so the manifest cannot become yet another independent copy of
 * the read vocabulary. This package is the only one that can import both.
 */
describe('the read manifest and the capability list describe the same surface', () => {
  it('names exactly the same read methods', () => {
    expect(CORE_READ_MANIFEST.map((entry) => entry.method).sort()).toEqual([...READ_METHOD_NAMES].sort());
  });
});
