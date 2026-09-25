import { describe, expect, it } from 'vitest';

import { isNewerThanCurrent } from './updater';

// Regression: the cached-state branch of `checkForUpdatesBackground` showed the
// banner whenever `notifiedVersion !== latestVersion`, with no comparison to the
// running binary. After updating, `latestVersion` IS the running version while
// `notifiedVersion` still holds the older one, so the banner read
// "Update available! 0.10.6 -> 0.10.6" and told the user to update to what they
// already had, until the check interval expired.
describe('isNewerThanCurrent', () => {
  it('is false for the version already running', () => {
    expect(isNewerThanCurrent('0.10.6', '0.10.6')).toBe(false);
  });

  it('is false for an older version', () => {
    expect(isNewerThanCurrent('0.10.5', '0.10.6')).toBe(false);
    expect(isNewerThanCurrent('0.9.9', '0.10.0')).toBe(false);
  });

  it('is true only for a genuinely newer one', () => {
    expect(isNewerThanCurrent('0.10.7', '0.10.6')).toBe(true);
    expect(isNewerThanCurrent('0.11.0', '0.10.6')).toBe(true);
    expect(isNewerThanCurrent('1.0.0', '0.10.6')).toBe(true);
  });
});
