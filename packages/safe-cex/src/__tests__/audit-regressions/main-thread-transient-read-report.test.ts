import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { BaseExchange } from '../../exchanges/base';
import { PAGE_MARKETS_MISSING, RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';
import { setTelemetrySink } from '../../telemetry';
import { engineCapabilities } from '../fixtures/engineCapabilities';

// Sentry TEALSTREET-NEXT-EGZ: page-engine read timeouts are still reported, but a listed
// read whose failure is transport-only carries `transient_read_op` so the host sink alone
// decides whether to demote it. A timeout with no command can be a write refusal
// (PAGE_MARKETS_MISSING, the startup write wait), so only the op is positive evidence.
const exchanges: { dispose(): void }[] = [];
const timeout = (message = 'Relay or network request timed out', fields: Record<string, unknown> = {}) =>
  new ExchangeError(message, { message, kind: 'timeout', outcome: 'notSent', ...fields });

let captureException: ReturnType<typeof vi.fn<(error: unknown, context: { tags?: Record<string, unknown> }) => void>>;
beforeEach(() => {
  for (const method of ['error', 'warn', 'trace'] as const) vi.spyOn(console, method).mockImplementation(() => {});
  captureException = vi.fn<(error: unknown, context: { tags?: Record<string, unknown> }) => void>();
  setTelemetrySink({ captureException });
});
afterEach(() => {
  for (const value of exchanges.splice(0)) value.dispose();
  setTelemetrySink({});
  vi.restoreAllMocks();
});

const exchangeErrorLogged = () =>
  vi.mocked(console.error).mock.calls.some(([first]) => String(first).includes('exchange error'));
const reportedTags = () => {
  expect(captureException).toHaveBeenCalledOnce();
  return captureException.mock.calls[0][1].tags ?? {};
};

describe('BaseExchange.emitAndLogError on the main thread', () => {
  it('still reports a read its caller marked transient, logging it at warning level', () => {
    const current = new BaseExchange(
      'hyperliquid',
      { key: '', secret: '', publicDataOnly: true, tuning: { consoleLogging: false } },
      new FrameStore(),
    );
    exchanges.push(current);
    current.emitAndLogError(timeout(), undefined, {
      transientRead: true,
      sentryTags: { transient_read_op: 'fetchCandles' },
    });
    expect(reportedTags()).toMatchObject({ transient_read_op: 'fetchCandles', error_kind: 'timeout' });
    expect(exchangeErrorLogged()).toBe(false);
    expect(console.warn).toHaveBeenCalled();
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
    ['fetchCandles', 'timeout', 'notSent'],
    ['fetchOptionChain', 'network', 'notSent'],
    ['refreshMarkets', 'timeout', undefined],
  ])('tags a transport failure of %s (%s, outcome %s) and still reports it', async (op, kind, outcome) => {
    const error = timeout(undefined, { kind, outcome });
    await expect(rust(error).command({ op })).rejects.toBe(error);
    expect(reportedTags()).toMatchObject({ transient_read_op: op });
    expect(exchangeErrorLogged()).toBe(false);
  });

  it.each([
    ['an unknown outcome', 'fetchCandles', { outcome: 'unknown' }],
    ['a non-transport kind', 'fetchCandles', { kind: 'rejected', outcome: 'rejected' }],
    ['a write op', 'setLeverage', {}],
    ['an op outside the read set', 'registerCancelChecks', {}],
  ])('reports %s untagged', async (_name, op, fields) => {
    const error = timeout(undefined, fields);
    await expect(rust(error).command({ op })).rejects.toBe(error);
    expect(reportedTags()).not.toHaveProperty('transient_read_op');
    expect(exchangeErrorLogged()).toBe(true);
  });

  it.each([
    ['missing page markets', PAGE_MARKETS_MISSING],
    [
      'the startup write wait',
      'hyperliquid placeOrder was not sent: the account did not load within 30s. Retry once it loads.',
    ],
  ])('reports %s, a timeout refusal with no command, untagged', (_name, message) => {
    rust(undefined).reportCommandError(timeout(message));
    expect(reportedTags()).not.toHaveProperty('transient_read_op');
    expect(exchangeErrorLogged()).toBe(true);
  });
});
