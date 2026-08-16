import axios from 'axios';
import { describe, expect, it } from 'vitest';

import { isSigningCaptureRequest, SIGNING_CAPTURE_FLAG } from './signingCapture';

/**
 * The whole mechanism rests on axios carrying a NON-STANDARD property from the
 * request config through to `response.config`, where the response interceptor
 * reads it. That is not a documented guarantee — it is observed behaviour of
 * axios' config merge — so it is asserted here rather than assumed.
 *
 * If an axios upgrade ever drops unknown config keys, this test fails loudly.
 * Without it the only symptom would be silent phantom `ok` counts: one per
 * signature, invisible in every gate, wrong in exactly the direction that makes
 * a rate-limit metric look healthier than it is.
 */
describe('signing-capture flag', () => {
  it('survives axios from request config to response.config', async () => {
    const instance = axios.create();

    let seenInRequestInterceptor = false;
    let seenOnResponseConfig = false;

    instance.interceptors.request.use((config) => {
      seenInRequestInterceptor = isSigningCaptureRequest(config);
      return config;
    });

    instance.interceptors.response.use((response) => {
      seenOnResponseConfig = isSigningCaptureRequest(response.config);
      return response;
    });

    await instance.request({
      method: 'get',
      url: 'https://example.invalid/probe',
      [SIGNING_CAPTURE_FLAG]: true,
      // Mirrors `BaseExchange.signHttpRequest`: resolve without touching the network.
      adapter: (config: Record<string, unknown>) =>
        Promise.resolve({ data: null, status: 200, statusText: 'OK', headers: {}, config }),
    } as never);

    expect(seenInRequestInterceptor).toBe(true);
    expect(seenOnResponseConfig).toBe(true);
  });

  it('never reaches the wire as a header or param', async () => {
    const instance = axios.create();
    const captured: { config?: Record<string, unknown> } = {};

    await instance.request({
      method: 'get',
      url: 'https://example.invalid/probe',
      params: { a: 1 },
      [SIGNING_CAPTURE_FLAG]: true,
      adapter: (config: Record<string, unknown>) => {
        captured.config = config;
        return Promise.resolve({ data: null, status: 200, statusText: 'OK', headers: {}, config } as never);
      },
    } as never);

    expect(JSON.stringify(captured.config?.headers ?? {})).not.toContain(SIGNING_CAPTURE_FLAG);
    expect(JSON.stringify(captured.config?.params ?? {})).not.toContain(SIGNING_CAPTURE_FLAG);
  });

  it('treats a plain request as not a capture', () => {
    expect(isSigningCaptureRequest({ url: '/x' })).toBe(false);
    expect(isSigningCaptureRequest(undefined)).toBe(false);
    expect(isSigningCaptureRequest(null)).toBe(false);
  });
});
