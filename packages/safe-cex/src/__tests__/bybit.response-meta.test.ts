import { describe, expect, it } from 'vitest';

import { getBybitResponseMeta } from './reference/bybit/bybit.response';

describe('getBybitResponseMeta', () => {
  it('treats native Bybit success payloads as successful', () => {
    const result = getBybitResponseMeta({ retCode: 0, retMsg: 'OK' });

    expect(result.isSuccess).toBe(true);
    expect(result.hasExplicitStatus).toBe(true);
    expect(result.retCode).toBe(0);
    expect(result.retMsg).toBe('OK');
  });

  it('extracts status from wrapped raw_response payloads', () => {
    const result = getBybitResponseMeta({
      raw_response: { ret_code: 0, ret_msg: 'OK' },
    });

    expect(result.isSuccess).toBe(true);
    expect(result.hasExplicitStatus).toBe(true);
    expect(result.retCode).toBe(0);
    expect(result.retMsg).toBe('OK');
  });

  it('handles stringified raw_response payloads', () => {
    const result = getBybitResponseMeta({
      raw_response: '{"retCode":10001,"retMsg":"position mode error"}',
    });

    expect(result.isSuccess).toBe(false);
    expect(result.hasExplicitStatus).toBe(true);
    expect(result.retCode).toBe(10001);
    expect(result.retMsg).toBe('position mode error');
  });

  it('marks payloads with no status fields as non-explicit', () => {
    const result = getBybitResponseMeta({ result: { list: [] } });

    expect(result.isSuccess).toBe(false);
    expect(result.hasExplicitStatus).toBe(false);
    expect(result.retCode).toBeUndefined();
    expect(result.retMsg).toBeUndefined();
  });
});
