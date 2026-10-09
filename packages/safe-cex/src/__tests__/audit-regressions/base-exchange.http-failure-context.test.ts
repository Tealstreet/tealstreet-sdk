import { afterEach, describe, expect, test, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';

/**
 * The HOW tunnel adapter resolves non-2xx without axios' settle(), so no axios
 * error is ever built and `formatErrorForSentry`'s causal `statusCode` never
 * fires. This correlation is the only trace of the status on that path.
 *
 * These assert the recorded slot rather than the emitted Sentry tags on
 * purpose: under happy-dom `window` exists, so `emitAndLogError` takes the
 * main-thread branch, which hands its tags to the telemetry sink (the no-op
 * default here) rather than to the emitter. Asserting the emitted payload would pass
 * against a field that is undefined in every case, proving nothing.
 */
const makeExchange = () => new BaseExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());

const slot = (exchange: BaseExchange) => (exchange as any)._lastHttpFailure;

/**
 * Force the web-worker branch of emitAndLogError. On the main-thread branch the
 * tags go to the telemetry sink, which is the no-op default here — so asserting
 * the emitted payload there would pass against a field that is undefined in
 * every case.
 */
const asWorker = <T>(fn: () => T): T => {
  vi.stubGlobal('window', undefined);
  try {
    return fn();
  } finally {
    vi.unstubAllGlobals();
  }
};

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('BaseExchange swallowed-HTTP-failure correlation', () => {
  test('suppresses console noise when requested but still emits the exchange error', () => {
    const exchange = makeExchange();
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const consoleTrace = vi.spyOn(console, 'trace').mockImplementation(() => undefined);
    const emitted: any[] = [];
    exchange.on('error', (message: string, meta: any) => emitted.push({ message, meta }));

    exchange.emitAndLogError('Expected venue rejection', undefined, { suppressConsoleLog: true });

    expect(consoleError).not.toHaveBeenCalled();
    expect(consoleTrace).not.toHaveBeenCalled();
    expect(emitted[0]).toMatchObject({
      message: 'Exchange Error: Expected venue rejection',
      meta: { suppressConsoleLog: true },
    });
    exchange.dispose();
  });

  test('spreads the correlation into the emitted Sentry tags', () => {
    const exchange = makeExchange();
    const tags: any[] = [];
    exchange.on('error', (_msg: string, meta: any) => tags.push(meta?.sentryData?.tags));

    exchange.noteHttpResponse({ status: 429, status_text: 'Too Many Requests' }, '/v5/order/create');
    asWorker(() => exchange.emitAndLogError('Unknown Exchange Error'));

    expect(tags[0]).toMatchObject({
      recentHttpStatus: 429,
      recentHttpStatusText: 'Too Many Requests',
      recentHttpUrl: '/v5/order/create',
    });
    exchange.dispose();
  });

  test('emits no correlation tags when nothing recently failed', () => {
    const exchange = makeExchange();
    const tags: any[] = [];
    exchange.on('error', (_msg: string, meta: any) => tags.push(meta?.sentryData?.tags));

    asWorker(() => exchange.emitAndLogError('Unknown Exchange Error'));

    expect(tags[0]?.recentHttpStatus).toBeUndefined();
    expect(tags[0]).toMatchObject({ exchange: 'bybit' });
    exchange.dispose();
  });

  test('records a swallowed non-2xx that axios never rejected', () => {
    const exchange = makeExchange();
    exchange.noteHttpResponse({ status: 503, status_text: 'Service Unavailable' }, '/v5/order/create');

    expect(slot(exchange)).toMatchObject({
      status: 503,
      statusText: 'Service Unavailable',
      url: '/v5/order/create',
    });
  });

  test('a successful response clears an earlier failure rather than leaving it current', () => {
    const exchange = makeExchange();
    exchange.noteHttpResponse({ status: 503 }, '/v5/order/create');
    exchange.noteHttpResponse({ status: 200 }, '/v5/market/tickers');

    expect(slot(exchange)).toBeUndefined();
  });

  test('consumes the record once, so one failure cannot decorate every error behind it', () => {
    const exchange = makeExchange();
    exchange.noteHttpResponse({ status: 429, status_text: 'Too Many Requests' }, '/v5/order/create');

    exchange.emitAndLogError('Unknown Exchange Error');
    expect(slot(exchange)).toBeUndefined();
  });

  test('leaves the record alone for an enhanced axios error, which already has a causal statusCode', () => {
    const exchange = makeExchange();
    exchange.noteHttpResponse({ status: 404 }, '/v5/order/realtime');

    const enhanced: any = new Error('Request failed with 500');
    enhanced.context = { statusCode: 500 };
    enhanced.enhancedMessage = 'Request failed with 500';
    exchange.emitAndLogError(enhanced);

    // Not consumed: a stale 404 must never be tagged beside a real 500, but the
    // next genuinely uncorrelated error is still entitled to see it.
    expect(slot(exchange)).toMatchObject({ status: 404 });
  });

  test('drops an expired record instead of leaving it in the slot forever', () => {
    const exchange = makeExchange();
    exchange.noteHttpResponse({ status: 500 }, '/v5/order/create');
    (exchange as any)._lastHttpFailure.at -= 60_000;

    exchange.emitAndLogError('Unknown Exchange Error');
    expect(slot(exchange)).toBeUndefined();
  });

  test('never throws when handed a malformed response', () => {
    const exchange = makeExchange();
    expect(() => exchange.noteHttpResponse(null)).not.toThrow();
    expect(() => exchange.noteHttpResponse(undefined)).not.toThrow();
    expect(() => exchange.noteHttpResponse('nonsense')).not.toThrow();
    expect(slot(exchange)).toBeUndefined();
  });
});
