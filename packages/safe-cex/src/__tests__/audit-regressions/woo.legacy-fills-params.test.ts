import { describe, expect, it } from 'vitest';

import { buildWooLegacyFillsParams } from '../reference/woo/woo.legacy-fills';

/**
 * WOO `GET /v1/client/trades` parameter vocabulary.
 *
 * Every assertion here is a MEASUREMENT against the live account (sheet row 28), not a
 * reading of WOO's docs — the docs and a reviewer had already disagreed about the units on
 * the sibling route, and both were partly wrong.
 *
 * The whole point of this file is that a wrong parameter name on this route does not error.
 * It is accepted and ignored, the venue serves its newest default page, and the caller
 * cannot distinguish that from a genuine answer. So these are equality assertions on the
 * exact wire names, not shape checks.
 */

describe('buildWooLegacyFillsParams', () => {
  it('emits `size`, because `limit` is accepted and IGNORED on this route', () => {
    // Measured: `limit=5` returned 100 rows (the route default); `size=5` returned 5.
    const params = buildWooLegacyFillsParams({ limit: 5 });
    expect(params.size).toBe(5);
    expect(params.limit).toBeUndefined();
  });

  it('emits `start_t`/`end_t`, because `startTime`/`endTime` are accepted and IGNORED', () => {
    // This is the pair that corrupted data. Measured against row 28: a January-2020 window
    // sent as `startTime`/`endTime` returned 100 rows of AUGUST-2026 fills, which the caller
    // then returned as the answer for the window it asked for. The identical window under
    // `start_t`/`end_t` returned 0 rows.
    const params = buildWooLegacyFillsParams({
      limit: 100,
      fromMilliseconds: 1577836800000,
      toMilliseconds: 1580515200000,
    });
    expect(params.start_t).toBe(1577836800000);
    expect(params.end_t).toBe(1580515200000);
    expect(params.startTime).toBeUndefined();
    expect(params.endTime).toBeUndefined();
  });

  it('keeps the window in MILLISECONDS — the response reports fractional seconds, the request does not', () => {
    // `executed_timestamp` comes back as "1786690691.317" (seconds with a ms fraction) while
    // the request takes milliseconds. Scaling the request to match the response puts the
    // window in 1970, where it silently matches nothing.
    const params = buildWooLegacyFillsParams({ limit: 10, fromMilliseconds: 1786690691317 });
    expect(params.start_t).toBe(1786690691317);
    expect(String(params.start_t)).toHaveLength(13);
  });

  it('omits the window entirely when unbounded, rather than sending 0', () => {
    // A `start_t=0` would be a real filter meaning "since the epoch"; absent means unbounded.
    // These coincide today, but only because the venue has no rows before 1970.
    const params = buildWooLegacyFillsParams({ limit: 10 });
    expect('start_t' in params).toBe(false);
    expect('end_t' in params).toBe(false);
  });

  it('passes `page` through — it works here, unlike on /v3/trade/transactionHistory', () => {
    // Measured: page=2 returned strictly older ids (1850167214 < 1850423592 on page 1), so
    // this route genuinely paginates. The v3 route ignores `page`, so the two must not share
    // a pager.
    expect(buildWooLegacyFillsParams({ limit: 100, page: 2 }).page).toBe(2);
    expect('page' in buildWooLegacyFillsParams({ limit: 100 })).toBe(false);
  });

  it('omits `symbol` when account-wide instead of sending an empty string', () => {
    expect('symbol' in buildWooLegacyFillsParams({ limit: 10 })).toBe(false);
    expect(buildWooLegacyFillsParams({ limit: 10, symbol: 'PERP_BTC_USDT' }).symbol).toBe('PERP_BTC_USDT');
  });
});
