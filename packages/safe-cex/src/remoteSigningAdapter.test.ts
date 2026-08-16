import type { AxiosAdapter, InternalAxiosRequestConfig } from 'axios';

import { AxiosHeaders } from 'axios';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createRemoteSigningAdapter, markSkipRemoteSigning } from './remoteSigningAdapter';
import type { RemoteSigner } from './types';

const configFor = (url: string): InternalAxiosRequestConfig =>
  ({
    method: 'GET',
    url,
    baseURL: 'https://futures.kraken.com/derivatives/api',
    headers: new AxiosHeaders(),
  }) as InternalAxiosRequestConfig;

describe('createRemoteSigningAdapter', () => {
  let sign: ReturnType<typeof vi.fn>;
  let inner: ReturnType<typeof vi.fn>;
  let remoteSigner: RemoteSigner;

  beforeEach(() => {
    inner = vi.fn(async (config: InternalAxiosRequestConfig) => ({
      data: { ok: true },
      status: 200,
      statusText: 'OK',
      headers: {},
      config,
    }));
    sign = vi.fn(async (request: { id: string }) => ({
      id: request.id,
      config: { headers: { 'X-Signed': '1' }, params: undefined, data: undefined },
    }));
    remoteSigner = { sign, accountName: 'K' } as unknown as RemoteSigner;
  });

  const build = () => createRemoteSigningAdapter(remoteSigner, 'kraken' as never, () => inner as AxiosAdapter, 'K');

  it('round-trips an untagged request through the remote signer', async () => {
    await build()(configFor('/v3/openorders'));

    expect(sign).toHaveBeenCalledTimes(1);
    expect(inner).toHaveBeenCalledTimes(1);
    expect(inner.mock.calls[0][0].headers.get('X-Signed')).toBe('1');
  });

  // The point of the tag: a public endpoint costs a WebSocket hop to the device
  // and crypto on its JS thread to produce a signature nothing checks.
  it('skips the remote signer entirely for a tagged request', async () => {
    await build()(markSkipRemoteSigning(configFor('/v3/instruments')));

    expect(sign).not.toHaveBeenCalled();
    expect(inner).toHaveBeenCalledTimes(1);
  });

  it('still executes a tagged request and returns the response', async () => {
    const response = await build()(markSkipRemoteSigning(configFor('/v3/tickers')));

    expect(response.status).toBe(200);
    expect(response.data).toEqual({ ok: true });
    expect(inner.mock.calls[0][0].url).toBe('/v3/tickers');
  });

  // Interceptors tag and then fall through, and some rebuild the config on the
  // way out (Bybit returns `{ ...config, headers: new AxiosHeaders({}) }`). The
  // tag has to survive that, or those exchanges silently keep round-tripping.
  it('survives an interceptor that spreads the config after tagging', async () => {
    const tagged = markSkipRemoteSigning(configFor('/v5/market/tickers'));
    const rebuilt = { ...tagged, headers: new AxiosHeaders({}) } as InternalAxiosRequestConfig;

    await build()(rebuilt);

    expect(sign).not.toHaveBeenCalled();
    expect(inner).toHaveBeenCalledTimes(1);
  });

  // Absence of the tag must mean "sign it", so an exchange that never tags
  // keeps exactly the behaviour it has today.
  it('treats a missing tag as needing a signature', async () => {
    const config = configFor('/v3/accounts');
    expect((config as unknown as Record<string, unknown>).safeCexSkipRemoteSigning).toBeUndefined();

    await build()(config);

    expect(sign).toHaveBeenCalledTimes(1);
  });
});
