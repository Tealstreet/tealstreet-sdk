import type { AxiosResponse, InternalAxiosRequestConfig } from 'axios';

import axios, { AxiosError, AxiosHeaders } from 'axios';
import { describe, expect, it, vi } from 'vitest';

import {
  resendOnSignatureRejection,
  SIGNATURE_RETRY_CONFIG_KEY,
  signatureRejectedResponse,
} from './signatureRejection';

const isRejection = (data: { code?: number } | undefined) => data?.code === -1022;
const responseWith = (
  data: { code: number },
  config: InternalAxiosRequestConfig = { headers: new AxiosHeaders() },
): AxiosResponse => ({ data, config, status: 200, statusText: 'OK', headers: {} });

const errorWith = (data?: { code: number }) => {
  const config = { headers: new AxiosHeaders() };
  return new AxiosError(undefined, undefined, config, undefined, data ? responseWith(data, config) : undefined);
};

describe('signatureRejectedResponse', () => {
  it('approves a rejected signature once, then lets the second surface', () => {
    const shouldRetry = signatureRejectedResponse(isRejection);
    const config: InternalAxiosRequestConfig & { [SIGNATURE_RETRY_CONFIG_KEY]?: boolean } = {
      url: '/order',
      headers: new AxiosHeaders(),
    };
    const error = new AxiosError(undefined, undefined, config, undefined, responseWith({ code: -1022 }, config));

    expect(shouldRetry(error)).toBe(true);
    expect(config[SIGNATURE_RETRY_CONFIG_KEY]).toBe(true);
    expect(shouldRetry(error)).toBe(false);
  });

  it('ignores other errors and network failures without a response', () => {
    const shouldRetry = signatureRejectedResponse(isRejection);

    expect(shouldRetry(errorWith({ code: -2010 }))).toBe(false);
    expect(shouldRetry(errorWith())).toBe(false);
  });
});

describe('resendOnSignatureRejection', () => {
  const xhrWith = () => {
    const xhr = axios.create();
    vi.spyOn(xhr, 'request').mockResolvedValue(responseWith({ code: 0 }));
    return xhr;
  };

  it('resends a rejected envelope once under the marker', async () => {
    const xhr = xhrWith();
    const config = { url: '/order', data: '{"a":1}', headers: new AxiosHeaders() };
    const response = responseWith({ code: -1022 }, config);

    await resendOnSignatureRejection(xhr, response, isRejection);

    expect(xhr.request).toHaveBeenCalledWith({
      url: '/order',
      data: '{"a":1}',
      headers: config.headers,
      [SIGNATURE_RETRY_CONFIG_KEY]: true,
    });
  });

  it('leaves a resend, or anything that is not a rejection, alone', () => {
    const xhr = xhrWith();
    const config = { headers: new AxiosHeaders(), [SIGNATURE_RETRY_CONFIG_KEY]: true };
    const resent = responseWith({ code: -1022 }, config);

    expect(resendOnSignatureRejection(xhr, resent, isRejection)).toBeUndefined();
    expect(resendOnSignatureRejection(xhr, responseWith({ code: 0 }), isRejection)).toBeUndefined();
    expect(xhr.request).not.toHaveBeenCalled();
  });
});
