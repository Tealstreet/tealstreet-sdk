import { describe, expect, test } from 'vitest';

import { ExchangeHealthMonitor } from '../../health/ExchangeHealthMonitor';
import { appendHttpFailure } from '../../utils/httpFailure';

/**
 * `appendHttpFailure` lengthens a message that `ExchangeHealthMonitor` then
 * classifies by substring, so the suffix itself can change how an error is
 * treated. These run the REAL monitor rather than re-listing the patterns,
 * because the hazard is that someone adds a pattern later without knowing this
 * append exists.
 */
const classify = (message: string) => new ExchangeHealthMonitor('bitunix' as any).processMessage(message, true);

const withStatus = (message: string, status: number, statusText: string) =>
  appendHttpFailure(message, { status, status_text: statusText });

describe('appendHttpFailure cannot reclassify an error', () => {
  test.each([
    [401, 'Unauthorized', 'terminal credential failure'],
    [403, 'Forbidden', 'terminal credential failure'],
    [405, 'Method Not Allowed', 'terminal credential failure'],
    [500, 'Internal Server Error', 'demoted toast'],
    [408, 'Request Timeout', 'demoted toast'],
    [504, 'Gateway Timeout', 'demoted toast'],
  ])('a transport %i does not cause a %s', (status, statusText) => {
    const original = 'Order rejected by venue';
    const appended = withStatus(original, status, statusText);

    expect(appended).toBe(`Order rejected by venue (HTTP ${status})`);
    expect(classify(appended)).toBe(classify(original));
  });

  test('a 404 cannot push a message about a market into the suppressed category', () => {
    // `/market.*not found/i` is the FIRST pattern group and wins over every
    // other category, so a bridged match would drop the error with no toast.
    const original = 'Failed to place order on market BTCUSDT';
    const appended = withStatus(original, 404, 'Not Found');

    expect(appended).toBe('Failed to place order on market BTCUSDT (HTTP 404)');
    expect(classify(appended)).not.toBe('suppressed');
    expect(classify(appended)).toBe(classify(original));
  });

  test('the bare number still classifies the cases worth classifying', () => {
    // '429', '502' and '503' are already patterns in errorPatterns.ts, and
    // rate_limit / exchange_down are the right answers for a transport failure —
    // so the number alone gets these right without any reason phrase.
    expect(withStatus('Order rejected', 429, 'Too Many Requests')).toBe('Order rejected (HTTP 429)');
    expect(classify(withStatus('Order rejected', 429, 'Too Many Requests'))).toBe('rate_limit');
    expect(classify(withStatus('Order rejected', 503, 'Service Unavailable'))).toBe('exchange_down');
    expect(classify(withStatus('Order rejected', 502, 'Bad Gateway'))).toBe('exchange_down');
  });

  test('never emits a canonical reason phrase, whatever the status', () => {
    // The guard that cannot rot: no reason text means no new pattern can ever
    // collide with it, however errorPatterns.ts grows.
    for (const [status, statusText] of [
      [401, 'Unauthorized'],
      [404, 'Not Found'],
      [451, 'Unavailable For Legal Reasons'],
      [507, 'Insufficient Storage'],
    ] as const) {
      expect(withStatus('Order rejected', status, statusText)).toBe(`Order rejected (HTTP ${status})`);
    }
  });

  test('does not throw when a venue hands it a non-string message', () => {
    // bitget and bitunix pass `v(response.data, 'msg')`, which is `any`.
    expect(() => appendHttpFailure(40001 as never, { status: 500 })).not.toThrow();
    expect(appendHttpFailure(40001 as never, { status: 500, status_text: 'Internal Server Error' })).toBe(
      '40001 (HTTP 500)',
    );
  });
});
