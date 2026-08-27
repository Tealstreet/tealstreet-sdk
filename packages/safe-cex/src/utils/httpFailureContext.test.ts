import { describe, expect, it } from 'vitest';

import { HTTP_FAILURE_CORRELATION_WINDOW_MS, readHttpFailureContext, recordHttpFailure } from './httpFailureContext';

describe('recordHttpFailure', () => {
  it('records a swallowed non-2xx with its url and time', () => {
    expect(recordHttpFailure({ status: 429, status_text: 'Too Many Requests' }, 1000, '/v5/order/create')).toEqual({
      status: 429,
      statusText: 'Too Many Requests',
      url: '/v5/order/create',
      at: 1000,
    });
  });

  it('clears rather than keeps the slot on a successful response', () => {
    // A stale failure is worse than none, because it reads as current.
    expect(recordHttpFailure({ status: 200 }, 1000, '/v5/order/create')).toBeUndefined();
  });
});

describe('readHttpFailureContext', () => {
  const record = { status: 502, statusText: 'Bad Gateway', url: '/exchange', at: 1000 };

  it('names the tags as a correlation, not as the error own status', () => {
    // `statusCode` is set by formatErrorForSentry from the error's OWN response
    // and is causal; these are circumstantial and must not be confusable.
    expect(readHttpFailureContext(record, 1400)).toEqual({
      recentHttpStatus: 502,
      recentHttpStatusText: 'Bad Gateway',
      recentHttpUrl: '/exchange',
      recentHttpAgeMs: 400,
    });
  });

  it('reports nothing when there is no record', () => {
    expect(readHttpFailureContext(undefined, 1400)).toBeUndefined();
  });

  it('expires outside the correlation window', () => {
    const edge = 1000 + HTTP_FAILURE_CORRELATION_WINDOW_MS;
    expect(readHttpFailureContext(record, edge)).toBeDefined();
    expect(readHttpFailureContext(record, edge + 1)).toBeUndefined();
  });

  it('ignores a record from the future rather than reporting a negative age', () => {
    expect(readHttpFailureContext(record, 900)).toBeUndefined();
  });

  it('strips the query string so a signed URL cannot land in an indexed tag', () => {
    // Kraken rewrites config.url to `path?postData`; Coinbase prefixes a proxy host.
    const withQuery = { status: 401, statusText: '', url: '/0/private/Balance?nonce=1756&otp=x', at: 1000 };
    expect(readHttpFailureContext(withQuery, 1100)?.recentHttpUrl).toBe('/0/private/Balance');
  });

  it('buckets the age so the tag is not unique per event', () => {
    expect(readHttpFailureContext({ status: 500, statusText: '', at: 1000 }, 1437)?.recentHttpAgeMs).toBe(400);
  });

  it('omits an empty status text instead of emitting a blank tag', () => {
    // The tunnel fills status_text from canonical_reason().unwrap_or("").
    expect(readHttpFailureContext({ status: 599, statusText: '', at: 1000 }, 1100)).toEqual({
      recentHttpStatus: 599,
      recentHttpAgeMs: 100,
    });
  });
});
