import { describe, expect, it } from 'vitest';

import { summarizeSocketDiagnostics } from '../../exchanges/base';

/**
 * These scalars are what a consuming app attaches to an error report, so the
 * failure mode is silent: a wrong number here reads as a confident diagnosis.
 */
describe('summarizeSocketDiagnostics', () => {
  it('takes the close code from the most recently closed socket', () => {
    const summary = summarizeSocketDiagnostics(
      [
        { lastCloseCode: 1000, lastCloseAt: 1_000 },
        { lastCloseCode: 1006, lastCloseAt: 5_000 },
        { lastCloseCode: 4001, lastCloseAt: 2_000 },
      ],
      'public',
    );

    expect(summary.publicCloseCode).toBe(1006);
  });

  it('ignores sockets that have never closed', () => {
    const summary = summarizeSocketDiagnostics([{}, { lastCloseCode: 1006, lastCloseAt: 10 }], 'private');

    expect(summary.privateCloseCode).toBe(1006);
  });

  it('reports no close code when nothing has closed', () => {
    const summary = summarizeSocketDiagnostics([{}, {}], 'public');

    expect(summary.publicCloseCode).toBeUndefined();
    expect(summary.publicCloseCountTotal).toBe(0);
    expect(summary.publicReconnectPaused).toBe(false);
  });

  it('keeps the lifetime total separate from the streak counter', () => {
    // The streak counter is reset on healthy traffic, so after a recovery it
    // reads 0 while the lifetime total still shows the churn happened.
    const summary = summarizeSocketDiagnostics(
      [
        { lifetimeCloseCount: 12, onCloseCount: 0 },
        { lifetimeCloseCount: 3, onCloseCount: 2 },
      ],
      'public',
    );

    expect(summary.publicCloseCountTotal).toBe(15);
    expect(summary.publicCloseStreak).toBe(2);
  });

  it('flags the pause if ANY socket has given up', () => {
    const summary = summarizeSocketDiagnostics(
      [{ isReconnectPauseActive: false }, { isReconnectPauseActive: true }],
      'private',
    );

    // One dead socket means that feed is not coming back on its own, which is
    // the whole point of surfacing it.
    expect(summary.privateReconnectPaused).toBe(true);
  });

  // Several exchanges (lighter, polymarket) replace onClose wholesale and run
  // their own reconnect loops. Reporting 0 for those is worse than reporting
  // nothing — it is indistinguishable from a healthy socket.
  it('distinguishes an instrumented quiet socket from an uninstrumented one only via the count', () => {
    const neverClosed = summarizeSocketDiagnostics([{ lifetimeCloseCount: 0 }], 'public');
    const churning = summarizeSocketDiagnostics([{ lifetimeCloseCount: 7 }], 'public');

    expect(neverClosed.publicCloseCountTotal).toBe(0);
    expect(churning.publicCloseCountTotal).toBe(7);
  });

  it('namespaces its keys by socket kind so both can be spread together', () => {
    const merged = {
      ...summarizeSocketDiagnostics([{ lastCloseCode: 1006, lastCloseAt: 1 }], 'public'),
      ...summarizeSocketDiagnostics([{ lastCloseCode: 1000, lastCloseAt: 1 }], 'private'),
    };

    expect(merged.publicCloseCode).toBe(1006);
    expect(merged.privateCloseCode).toBe(1000);
  });
});
