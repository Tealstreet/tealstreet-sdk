import { describe, expect, it } from 'vitest';

// Regression: the dedup-cleanup interval used to be ref'd. Every CLI subcommand
// imports this module, so `account add`, `exec` etc. finished their work and
// then hung until the user pressed Ctrl+C.
describe('logger module', () => {
  it('adds no ref\'d timers that would hold the event loop open', async () => {
    const countTimers = () => process.getActiveResourcesInfo().filter((r) => r === 'Timeout').length;

    const before = countTimers();
    await import('./logger');
    // Runtime/test-runner upgrades can let unrelated timers settle during the import.
    // The logger invariant is that importing it does not add a new ref'd timer.
    expect(countTimers()).toBeLessThanOrEqual(before);
  });
});
