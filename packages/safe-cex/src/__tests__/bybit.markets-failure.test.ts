import { describe, expect, it } from 'vitest';

import { getBybitMarketsFailureTelemetry, describeBybitMarketsFailure } from './reference/bybit/bybit.exchange';

describe('describeBybitMarketsFailure', () => {
  it('names a rate limit from the retMsg', () => {
    expect(
      describeBybitMarketsFailure({ response: { status: 403, data: { retMsg: 'Way too many requests; IP(1.2.3.4)' } } }),
    ).toBe('rate-limited');
  });

  it('names a rate limit from a 429', () => {
    expect(describeBybitMarketsFailure({ response: { status: 429 } })).toBe('rate-limited');
  });

  it('names a ban', () => {
    expect(
      describeBybitMarketsFailure({ response: { status: 403, data: { retMsg: 'IP banned until 1788351953047' } } }),
    ).toBe('blocked');
  });

  it('names a timeout', () => {
    expect(describeBybitMarketsFailure({ message: 'timeout of 10000ms exceeded' })).toBe('timeout');
    expect(describeBybitMarketsFailure({ code: 'ECONNABORTED', message: 'aborted' })).toBe('timeout');
  });

  it('falls back to the status code', () => {
    expect(describeBybitMarketsFailure({ response: { status: 502 } })).toBe('HTTP 502');
  });

  it('never leaks the ban timestamp or proxy IP into the message', () => {
    const described = describeBybitMarketsFailure({
      response: { status: 403, data: { retMsg: 'Way too many requests; IP(18.178.14.27) banned until 1788351953047' } },
    });
    expect(described).not.toMatch(/\d{10}/);
    expect(described).not.toContain('18.178.14.27');
  });

  it('tolerates a missing or malformed cause', () => {
    expect(describeBybitMarketsFailure(null)).toBe('unknown');
    expect(describeBybitMarketsFailure(undefined)).toBe('unknown');
    expect(describeBybitMarketsFailure({})).toBe('unknown');
    expect(describeBybitMarketsFailure('nope')).toBe('unknown');
  });

  it('keeps the vocabulary small so Sentry grouping stays bounded', () => {
    const causes = [
      { response: { status: 429 } },
      { response: { status: 403, data: { retMsg: 'banned' } } },
      { message: 'timed out' },
      { response: { status: 500 } },
      null,
    ];
    const described = new Set(causes.map(describeBybitMarketsFailure));
    expect(described.size).toBeLessThanOrEqual(causes.length);
  });

  it('returns safe telemetry fields without exposing raw venue messages', () => {
    const telemetry = getBybitMarketsFailureTelemetry({
      code: 'ECONNABORTED',
      config: { method: 'get', url: '/v5/market/instruments-info' },
      response: {
        status: 429,
        data: {
          retCode: '10006',
          retMsg: 'Too many visits; IP(18.178.14.27) banned until 1788351953047',
        },
      },
    });

    expect(telemetry).toEqual({
      reason: 'rate-limited',
      httpStatus: 429,
      errorCode: 'ECONNABORTED',
      retCode: 10006,
      endpoint: '/v5/market/instruments-info',
      method: 'GET',
    });
    expect(JSON.stringify(telemetry)).not.toContain('18.178.14.27');
    expect(JSON.stringify(telemetry)).not.toContain('1788351953047');
  });
});
