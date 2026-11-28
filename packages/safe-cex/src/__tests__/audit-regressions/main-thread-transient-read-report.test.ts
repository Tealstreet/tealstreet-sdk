import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { BaseExchange } from '../../exchanges/base';
import { PAGE_MARKETS_MISSING, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';
import { setTelemetrySink } from '../../telemetry';
import { engineCapabilities } from '../fixtures/engineCapabilities';

// Sentry TEALSTREET-NEXT-EGZ: the page engine reported classified notSent read
// timeouts straight to Sentry, before the host's transient-read rule could log them.
// Only positive evidence of a read skips the report: a timeout with no command can be
// a write refusal (PAGE_MARKETS_MISSING, the startup write wait).
const exchanges: { dispose(): void }[] = [];
const timeout = (message = 'Relay or network request timed out', fields: Record<string, unknown> = {}) =>
  new ExchangeError(message, { message, kind: 'timeout', outcome: 'notSent', ...fields });

let captureException: ReturnType<typeof vi.fn<(error: unknown) => void>>;
beforeEach(() => {
  for (const method of ['error', 'warn', 'trace'] as const) vi.spyOn(console, method).mockImplementation(() => {});
  captureException = vi.fn<(error: unknown) => void>();
  setTelemetrySink({ captureException });
});
afterEach(() => {
  for (const value of exchanges.splice(0)) value.dispose();
  setTelemetrySink({});
  vi.restoreAllMocks();
});

const exchangeErrorLogged = () =>
  vi.mocked(console.error).mock.calls.some(([first]) => String(first).includes('exchange error'));

describe('BaseExchange.emitAndLogError on the main thread', () => {
  const exchange = () => {
    const value = new BaseExchange(
      'hyperliquid',
      { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging: false } },
      new FrameStore(),
    );
    exchanges.push(value);
    return value;
  };

  it('logs a read its caller marked transient at warning level, with no Sentry exception', () => {
    const current = exchange();
    const listener = vi.fn();
    current.on('error', listener);
    current.emitAndLogError(timeout(), undefined, { transientRead: true });
    expect(captureException).not.toHaveBeenCalled();
    expect(exchangeErrorLogged()).toBe(false);
    expect(console.warn).toHaveBeenCalled();
    expect(listener).toHaveBeenCalledOnce();
    expect(listener.mock.calls[0][1].classified).toMatchObject({ kind: 'timeout', outcome: 'notSent' });
  });

  it('still reports a timeout no caller marked as a read', () => {
    exchange().emitAndLogError(timeout());
    expect(captureException).toHaveBeenCalledOnce();
    expect(exchangeErrorLogged()).toBe(true);
  });
});

describe('RustEngineExchange command reports', () => {
  const rust = (reject: unknown) => {
    setWasmEngineLoader(() => new Promise(() => {}));
    const value = new RustEngineExchange(
      'hyperliquid',
      { key: '', secret: '', publicDataOnly: true },
      new FrameStore(),
    );
    exchanges.push(value);
    Object.assign(value as unknown as Record<string, unknown>, {
      capabilityRecord: JSON.parse(engineCapabilities('hyperliquid')),
      host: { command: () => Promise.reject(reject), dispose: () => {}, flush: () => {} },
    });
    return value as unknown as {
      command: (command: { op: string }) => Promise<unknown>;
      reportCommandError: (error: unknown) => void;
    };
  };

  it.each([
    ['timeout', 'notSent'],
    ['network', 'notSent'],
    ['timeout', undefined],
  ])('sends no exception for a %s candle read (outcome %s)', async (kind, outcome) => {
    const error = timeout(undefined, { kind, outcome });
    await expect(rust(error).command({ op: 'fetchCandles' })).rejects.toBe(error);
    expect(captureException).not.toHaveBeenCalled();
    expect(exchangeErrorLogged()).toBe(false);
  });

  it.each([
    ['an unknown outcome', 'fetchCandles', { outcome: 'unknown' }],
    ['a non-transport kind', 'fetchCandles', { kind: 'rejected', outcome: 'rejected' }],
    ['a write op', 'setLeverage', {}],
    ['an op outside the read set', 'registerCancelChecks', {}],
  ])('still reports %s', async (_name, op, fields) => {
    const error = timeout(undefined, fields);
    await expect(rust(error).command({ op })).rejects.toBe(error);
    expect(captureException).toHaveBeenCalledOnce();
  });

  it.each([
    ['missing page markets', PAGE_MARKETS_MISSING],
    [
      'the startup write wait',
      'hyperliquid placeOrder was not sent: the account did not load within 30s. Retry once it loads.',
    ],
  ])('still reports %s, a timeout refusal with no command', (_name, message) => {
    rust(undefined).reportCommandError(timeout(message));
    expect(captureException).toHaveBeenCalledOnce();
    expect(exchangeErrorLogged()).toBe(true);
  });
});
