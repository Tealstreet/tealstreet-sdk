import { describe, expect, it } from 'vitest';

import { appendHttpFailure, describeHttpFailure, getHttpFailure } from './httpFailure';

describe('getHttpFailure', () => {
  it('reports nothing for a response that did not fail', () => {
    expect(getHttpFailure({ status: 200, statusText: 'OK' })).toBeUndefined();
    expect(getHttpFailure({ status: 304 })).toBeUndefined();
  });

  it('reports nothing when there is no numeric status to trust', () => {
    expect(getHttpFailure({ statusText: 'Bad Gateway' })).toBeUndefined();
    expect(getHttpFailure({ status: '502' })).toBeUndefined();
    expect(getHttpFailure(null)).toBeUndefined();
    expect(getHttpFailure(undefined)).toBeUndefined();
    expect(getHttpFailure('502')).toBeUndefined();
  });

  it("reads the tunnel frame's snake_case status_text", () => {
    // Axios' own statusText is undefined on every tunnelled response — the Rust
    // ResponseData struct has no serde rename_all.
    expect(getHttpFailure({ status: 429, status_text: 'Too Many Requests' })).toEqual({
      status: 429,
      statusText: 'Too Many Requests',
    });
  });

  it('treats an empty status_text as absent rather than falling back to it', () => {
    // The tunnel fills it from canonical_reason().unwrap_or(""), so a
    // non-standard code arrives as a present-but-empty string.
    expect(getHttpFailure({ status: 599, status_text: '' })).toEqual({ status: 599, statusText: '' });
  });

  it('treats 400 as failed and 399 as not, so the boundary cannot drift', () => {
    expect(getHttpFailure({ status: 400 })).toEqual({ status: 400, statusText: '' });
    expect(getHttpFailure({ status: 399 })).toBeUndefined();
  });

  it('rejects an array, which is typeof object but never a response', () => {
    expect(getHttpFailure([])).toBeUndefined();
    expect(getHttpFailure(Object.assign([], { status: 500 }))).toBeUndefined();
  });

  it('prefers a real axios statusText over the tunnel spelling', () => {
    expect(getHttpFailure({ status: 500, statusText: 'Internal Server Error', status_text: '' })).toEqual({
      status: 500,
      statusText: 'Internal Server Error',
    });
  });
});

describe('appendHttpFailure', () => {
  it('leaves a message untouched when the response did not fail', () => {
    expect(appendHttpFailure('Order rejected', { status: 200 })).toBe('Order rejected');
    expect(appendHttpFailure('Order rejected', undefined)).toBe('Order rejected');
  });

  it('appends the status to a message the caller was already emitting', () => {
    expect(appendHttpFailure('Order rejected', { status: 429, status_text: 'Too Many Requests' })).toBe(
      'Order rejected (HTTP 429)',
    );
  });

  it('never appends the canonical reason, which would reclassify the error', () => {
    // See appendHttpFailure's own note: ExchangeHealthMonitor matches over the
    // whole message, so a reason phrase changes how the error is treated.
    expect(appendHttpFailure('Order rejected', { status: 401, status_text: 'Unauthorized' })).not.toContain(
      'Unauthorized',
    );
    expect(appendHttpFailure('Order rejected', { status: 599, status_text: '' })).toBe('Order rejected (HTTP 599)');
  });

  it('is idempotent so a venue that formats its own status does not get it twice', () => {
    const once = appendHttpFailure('Order rejected', { status: 503, status_text: 'Service Unavailable' });
    expect(appendHttpFailure(once, { status: 503, status_text: 'Service Unavailable' })).toBe(once);
  });
});

describe('describeHttpFailure', () => {
  it('names the venue and the status, never the reason', () => {
    // Same rule as appendHttpFailure: this string reaches emitAndLogError, so a
    // canonical reason phrase would reclassify the error.
    expect(describeHttpFailure('Hyperliquid', { status: 429, statusText: 'Too Many Requests', data: null })).toBe(
      'Hyperliquid API error (HTTP 429)',
    );
    expect(describeHttpFailure('Hyperliquid', { status: 401, statusText: 'Unauthorized', data: null })).not.toContain(
      'Unauthorized',
    );
  });

  it('carries an unreadable body through, since that is the whole diagnosis', () => {
    expect(
      describeHttpFailure('Hyperliquid', {
        status: 422,
        data: { raw_response: 'Failed to deserialize the JSON body into the target type' },
      }),
    ).toBe(
      'Hyperliquid API error (HTTP 422): {"raw_response":"Failed to deserialize the JSON body into the target type"}',
    );
  });

  it('degrades to `HTTP unknown` rather than `HTTP undefined` without a status', () => {
    expect(describeHttpFailure('Hyperliquid', { data: null })).toBe('Hyperliquid API error (HTTP unknown)');
  });

  it('truncates a long body to 300 characters', () => {
    const described = describeHttpFailure('Bybit', { status: 502, data: { pad: 'x'.repeat(5000) } });
    const body = described.slice(described.indexOf(': ') + 2);
    expect(body).toHaveLength(300);
  });

  it('survives a body that cannot be serialized', () => {
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    expect(describeHttpFailure('Bybit', { status: 500, data: circular })).toContain('Bybit API error (HTTP 500)');
  });
});
