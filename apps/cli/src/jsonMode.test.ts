import { describe, expect, it } from 'vitest';

import { isJsonMode } from './jsonMode';

describe('isJsonMode', () => {
  it('is on for a --json flag and off for one inside a quoted exec command', () => {
    expect(isJsonMode(['whoami', '--json'])).toBe(true);
    expect(isJsonMode(['exec', 'buy --json'])).toBe(false);
    expect(isJsonMode(['whoami'])).toBe(false);
  });
});
