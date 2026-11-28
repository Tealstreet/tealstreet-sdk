import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { FrameStore } from '../../store/frameStore';
import { setTelemetrySink } from '../../telemetry';

// Sentry TEALSTREET-NEXT-EGZ: the page engine reported classified notSent read
// timeouts straight to Sentry, before the host's transient-read rule could log them.
const exchanges: BaseExchange[] = [];
const exchange = () => {
  const value = new BaseExchange(
    'hyperliquid',
    { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging: false } },
    new FrameStore(),
  );
  exchanges.push(value);
  return value;
};
const failure = (fields: Record<string, unknown>) =>
  Object.assign(new Error('Relay or network request timed out'), fields);

beforeEach(() => {
  for (const method of ['error', 'trace'] as const) vi.spyOn(console, method).mockImplementation(() => {});
});
afterEach(() => {
  for (const value of exchanges.splice(0)) value.dispose();
  setTelemetrySink({});
  vi.restoreAllMocks();
});

describe('main-thread emitAndLogError for a transport failure', () => {
  it.each([
    ['timeout', 'notSent'],
    ['network', 'notSent'],
    ['timeout', undefined],
  ])('logs a %s read (outcome %s) without a Sentry exception', (kind, outcome) => {
    const current = exchange();
    const captureException = vi.fn();
    const listener = vi.fn();
    setTelemetrySink({ captureException });
    current.on('error', listener);
    current.emitAndLogError(failure({ kind, outcome }));
    expect(captureException).not.toHaveBeenCalled();
    expect(listener).toHaveBeenCalledOnce();
    expect(listener.mock.calls[0][1].classified).toMatchObject({ kind });
  });

  it.each([
    ['an unknown outcome', { kind: 'timeout', outcome: 'unknown' }],
    ['a write command', { kind: 'timeout', outcome: 'notSent', command: 'placeOrders' }],
    ['an unclassified error', {}],
    ['a non-transport kind', { kind: 'rejected', outcome: 'rejected' }],
  ])('still reports %s', (_name, fields) => {
    const current = exchange();
    const captureException = vi.fn();
    setTelemetrySink({ captureException });
    current.emitAndLogError(failure(fields));
    expect(captureException).toHaveBeenCalledOnce();
  });
});
