import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

// Regression: `tealstreet balance` printed its table and then ran forever —
// a user on the shipped 0.10.5 binary had to Ctrl-C out of every one-shot
// command. `stopExchange` is not enough on its own: the virtual clock
// re-arms a 5-minute sync and the HoW tunnel sockets hold a ping timer, and
// the REPL only ever avoided this because all of its exits call process.exit.
//
// repl.ts cannot be imported here (it builds a live session at module load),
// so the wiring is asserted against the source. The behaviour underneath it —
// that stopping the clock actually clears the timer — is a real test below.
const replSource = readFileSync(join(__dirname, 'repl.ts'), 'utf8');
const teardown = replSource.slice(replSource.indexOf('async function teardownOneShot'));

describe('one-shot teardown', () => {
  it('is what the single-command path ends with', () => {
    expect(replSource).toMatch(/await teardownOneShot\(session\.currentExchange\)/);
  });

  it('stops the virtual clock, whose sync timer would otherwise re-arm forever', () => {
    expect(teardown).toMatch(/virtualClock\.stop\(\)/);
  });

  it('disposes the HoW tunnel sockets, which hold a ping timer', () => {
    expect(teardown).toMatch(/disposeHOWWebSockets\(\)/);
  });

  it('exits rather than waiting for an event loop that never drains', () => {
    expect(teardown).toMatch(/process\.exit\(exitCode\)/);
  });

  it('makes a rejected API key terminal and non-zero', () => {
    expect(replSource).toMatch(/authFailureStreak \?\? 0\) > 0/);
    expect(replSource).toMatch(/teardownOneShot\(session\.currentExchange, 1\)/);
  });
});

describe('virtualClock.stop', () => {
  it('clears the pending sync timer', async () => {
    const { virtualClock } = await import('../../../packages/safe-cex/src/utils/virtualClock');
    const countTimers = (): number => process.getActiveResourcesInfo().filter((r) => r === 'Timeout').length;

    const before = countTimers();
    await virtualClock.start();
    virtualClock.stop();
    expect(countTimers()).toBe(before);
  });
});
