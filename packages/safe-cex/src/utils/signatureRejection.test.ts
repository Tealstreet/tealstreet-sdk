import type { AxiosInstance } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import {
  SIGNATURE_RETRY_CONFIG_KEY,
  resendOnSignatureRejection,
  signatureRejectedResponse,
} from './signatureRejection';

const isRejection = (data: any) => data?.code === -1022;

describe('signatureRejectedResponse', () => {
  it('approves a rejected signature once, then lets the second surface', () => {
    const shouldRetry = signatureRejectedResponse(isRejection);
    const error = { config: { url: '/order' }, response: { data: { code: -1022 } } } as any;

    expect(shouldRetry(error)).toBe(true);
    expect(error.config[SIGNATURE_RETRY_CONFIG_KEY]).toBe(true);
    expect(shouldRetry(error)).toBe(false);
  });

  it('ignores other errors and network failures without a response', () => {
    const shouldRetry = signatureRejectedResponse(isRejection);

    expect(shouldRetry({ config: {}, response: { data: { code: -2010 } } } as any)).toBe(false);
    expect(shouldRetry({ config: {} } as any)).toBe(false);
  });
});

describe('resendOnSignatureRejection', () => {
  const xhrWith = () => ({ request: vi.fn(async (config: any) => ({ data: {}, config })) }) as unknown as AxiosInstance;

  it('resends a rejected envelope once under the marker', async () => {
    const xhr = xhrWith();
    const response = { data: { code: -1022 }, config: { url: '/order', data: '{"a":1}' } } as any;

    await resendOnSignatureRejection(xhr, response, isRejection);

    expect(xhr.request).toHaveBeenCalledWith({ url: '/order', data: '{"a":1}', [SIGNATURE_RETRY_CONFIG_KEY]: true });
  });

  it('leaves a resend, or anything that is not a rejection, alone', () => {
    const xhr = xhrWith();
    const resent = { data: { code: -1022 }, config: { [SIGNATURE_RETRY_CONFIG_KEY]: true } } as any;

    expect(resendOnSignatureRejection(xhr, resent, isRejection)).toBeUndefined();
    expect(resendOnSignatureRejection(xhr, { data: { code: 0 }, config: {} } as any, isRejection)).toBeUndefined();
    expect(xhr.request).not.toHaveBeenCalled();
  });
});
