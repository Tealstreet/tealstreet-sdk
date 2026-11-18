import { afterEach, describe, expect, it, vi } from 'vitest';

import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';

import { ExchangeWorker } from './ExchangeWorker';

const createWorker = () => {
  const messages: any[] = [];
  const ctx = {
    postMessage: vi.fn((message: any) => {
      messages.push(message);
    }),
  };
  const worker = new ExchangeWorker(
    ctx as any,
    'bybit-main',
    'bybit',
    {} as any,
    {} as any,
    {} as any,
    'session',
  );

  return { messages, worker };
};

const controlledBybitMarketsError = () =>
  Object.assign(new Error('Failed to fetch Bybit Trading markets (timeout)'), {
    name: 'BybitTradingMarketsRequestError',
    safeCexSentryReportCooldownMs: 5 * 60_000,
    safeCexSentryReportKey: 'bybit:trading-markets:timeout',
    safeCexSentryTags: {
      bybit_markets_failure_reason: 'timeout',
      worker_error_family: 'bybit_markets_startup',
    },
    safeCexSentryExtra: {
      bybitMarketsEndpoint: '/v5/market/instruments-info',
      bybitMarketsFailureReason: 'timeout',
    },
  });

describe('ExchangeWorker Sentry reporting', () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it('cooldown-suppresses repeated controlled startup errors and reports the suppressed count later', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_800_000_000_000);
    const { messages, worker } = createWorker();

    (worker as any).logError('Error starting exchange: bybit', controlledBybitMarketsError());
    (worker as any).logError('Error starting exchange: bybit', controlledBybitMarketsError());

    expect(messages).toHaveLength(2);
    expect(messages[0].data[1]).toMatchObject({
      sendToSentry: true,
      suppressConsoleLog: false,
      sentryData: {
        tags: {
          bybit_markets_failure_reason: 'timeout',
          worker_error_family: 'bybit_markets_startup',
        },
        extra: {
          bybitMarketsEndpoint: '/v5/market/instruments-info',
          bybitMarketsFailureReason: 'timeout',
        },
      },
    });
    expect(messages[1].data[1]).toMatchObject({
      sendToSentry: false,
      suppressConsoleLog: true,
    });

    await vi.advanceTimersByTimeAsync(5 * 60_000 + 1);
    (worker as any).logError('Error starting exchange: bybit', controlledBybitMarketsError());

    expect(messages[2].data[1]).toMatchObject({
      sendToSentry: true,
      suppressConsoleLog: false,
      sentryData: {
        extra: {
          suppressedRepeatedReports: 1,
        },
      },
    });
  });

  // Regression guard rather than a prove-failable assertion: this held before
  // the phase tag existed and must keep holding, or every worker error in the
  // process would read as a startup failure.
  it('leaves non-startup worker errors unphased', () => {
    const { messages, worker } = createWorker();

    (worker as any).logError('Something else broke', new Error('boom'));

    expect(messages[0].data[1].sentryData.tags).not.toHaveProperty('worker_error_phase');
  });

  // The phase is orthogonal to the family. bybit already self-tags its markets
  // failure, and that tag spreads last — this pins that the two coexist rather
  // than one clobbering the other.
  it('keeps the bybit family tag alongside the startup phase', () => {
    const { messages, worker } = createWorker();

    (worker as any).logError('Error starting exchange: bybit', controlledBybitMarketsError(), {
      sentryTags: { worker_error_phase: 'startup' },
    });

    expect(messages[0].data[1].sentryData.tags).toMatchObject({
      worker_error_family: 'bybit_markets_startup',
      worker_error_phase: 'startup',
    });
  });
});


describe('ExchangeWorker closed history diagnostics', () => {
  it('retains native decode evidence through the actual redacted worker packet', () => {
    const { messages, worker } = createWorker();
    const error = new ExchangeError('Kraken order history rejected: apiKey=private-key-canary', {
      message: 'Kraken order history rejected: apiKey=private-key-canary',
      kind: 'internal',
      causeCode: 'HISTORY_DECODE',
      historyDecode: { schema: 'krakenOrders', reason: 'quantityDecimal' },
    });
    Object.assign(error, { row: { uid: 'private-row', quantity: 123 }, credentials: { apiKey: 'private-key-canary' } });
    Reflect.get(worker, 'logError').call(worker, 'Kraken history failed', error);
    const { sentryData } = structuredClone(messages[0]).data[1];
    expect(sentryData.tags).toMatchObject({
      cause_code: 'HISTORY_DECODE',
      history_schema: 'krakenOrders',
      history_reason: 'quantityDecimal',
    });
    expect(sentryData.error).toMatchObject({
      causeCode: 'HISTORY_DECODE',
      historyDecode: { schema: 'krakenOrders', reason: 'quantityDecimal' },
    });
    expect(sentryData.error).not.toHaveProperty('row');
    expect(sentryData.error).not.toHaveProperty('credentials');
    expect(JSON.stringify(sentryData)).not.toContain('private-row');
    expect(JSON.stringify(sentryData)).not.toContain('private-key-canary');
  });

  it('does not project arbitrary uppercase causes or row-shaped diagnostic values', () => {
    const { messages, worker } = createWorker();
    const error = Object.assign(new Error('history failed'), {
      causeCode: 'PRIVATE_ACCOUNT_123',
      historyDecode: { schema: 'krakenOrders', reason: 'private-amount' },
    });
    Reflect.get(worker, 'logError').call(worker, 'history failed', error);
    const { sentryData } = messages[0].data[1];
    expect(sentryData.tags).not.toHaveProperty('cause_code');
    expect(sentryData.tags).not.toHaveProperty('history_reason');
    expect(sentryData.error).not.toHaveProperty('causeCode');
    expect(sentryData.error).not.toHaveProperty('historyDecode');
    expect(JSON.stringify(sentryData)).not.toContain('PRIVATE_ACCOUNT_123');
    expect(JSON.stringify(sentryData)).not.toContain('private-amount');
  });
});
