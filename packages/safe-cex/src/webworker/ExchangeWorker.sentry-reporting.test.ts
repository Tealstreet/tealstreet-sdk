import { afterEach, describe, expect, it, vi } from 'vitest';

import { ExchangeWorker } from './ExchangeWorker';

const createWorker = () => {
  const messages: any[] = [];
  const ctx = {
    postMessage: vi.fn((message: any) => {
      messages.push(message);
    }),
  };
  const worker = new ExchangeWorker(ctx as any, 'bybit-main', 'bybit', {} as any, {} as any, {} as any, 'session', false);

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
});
