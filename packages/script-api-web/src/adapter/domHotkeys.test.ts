import { describe, expect, it } from 'vitest';

import { canAddEventListener, isDomTarget } from './domHotkeys';

describe('canAddEventListener', () => {
  it('accepts anything with addEventListener', () => {
    expect(canAddEventListener({ addEventListener: () => {} })).toBe(true);
  });

  it('rejects non-objects and plain objects', () => {
    for (const bad of [null, undefined, 42, 'x', {}, () => {}]) {
      expect(canAddEventListener(bad)).toBe(false);
    }
  });
});

describe('isDomTarget', () => {
  // The whole point of splitting this from canAddEventListener: a duck-type
  // would narrow these to HTMLElement|Document|Window and the compiler would
  // then trust it.
  it('rejects non-DOM objects that merely have addEventListener', () => {
    expect(isDomTarget({ addEventListener: () => {} })).toBe(false);
  });

  it('rejects primitives and nullish values', () => {
    for (const bad of [null, undefined, 0, '', {}]) {
      expect(isDomTarget(bad)).toBe(false);
    }
  });
});
