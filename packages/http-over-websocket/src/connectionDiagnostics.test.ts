import { afterEach, expect, it, vi } from 'vitest';
import {
  ConnectionDiagnostics,
  HiddenTimerLateness,
  maximumTimerLateness,
  setConnectionVisibility,
} from './connectionDiagnostics';
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  setConnectionVisibility('visible');
});
it('keeps only phase, initiator, final dial port and lifetime buckets', () => {
  vi.useFakeTimers({ now: 1_000 });
  const connection = new ConnectionDiagnostics('relay');
  connection.dial('wss://relay.test:8443/tunnel?token=secret');
  expect(connection.closed('local')).toEqual({
    route: 'relay',
    port: '8443',
    phase: 'pre-open',
    initiator: 'local',
    lifetime: 'unknown',
  });
  connection.opened(true);
  vi.setSystemTime(9_000);
  expect(connection.closed('unknown')).toMatchObject({ phase: 'handshake', lifetime: '5-15s' });
  connection.established();
  vi.setSystemTime(31_000);
  expect(connection.closed('remote')).toMatchObject({ phase: 'established', lifetime: '30-120s' });
  connection.dial('wss://relay.test/tunnel');
  expect(connection.closed('unknown')).toMatchObject({ phase: 'pre-open', port: '443', lifetime: 'unknown' });
  expect(JSON.stringify(connection.lastClose)).not.toMatch(/secret|relay.test|wss:/);
});
it('measures maximum lateness only for callbacks delivered while hidden', () => {
  vi.useFakeTimers({ now: 10_000 });
  vi.stubGlobal('document', undefined);
  const timers = new HiddenTimerLateness();
  expect(timers.bucket).toBe('unknown');
  timers.observe(0);
  expect(timers.bucket).toBe('unknown');
  setConnectionVisibility('hidden');
  timers.observe(5_000);
  expect(timers.bucket).toBe('5-20s');
  timers.observe(9_999);
  expect(timers.bucket).toBe('5-20s');
  vi.setSystemTime(80_000);
  timers.observe(10_000);
  expect(timers.bucket).toBe('>=60s');
  expect(maximumTimerLateness('unknown', '1-5s', '20-60s')).toBe('20-60s');
});
