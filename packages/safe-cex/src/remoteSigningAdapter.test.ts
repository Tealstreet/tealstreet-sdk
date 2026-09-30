import type { AxiosAdapter, InternalAxiosRequestConfig } from 'axios';
import type { RemoteSigner } from './types';

import { AxiosHeaders } from 'axios';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { createRemoteSigningAdapter, markSkipRemoteSigning } from './remoteSigningAdapter';

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

  it('does not restore original request data when the signer intentionally omits it', async () => {
    const config = {
      ...configFor('/fapi/v1/positionSide/dual'),
      method: 'POST',
      data: { dualSidePosition: 'true' },
    } as InternalAxiosRequestConfig;

    sign.mockResolvedValueOnce({
      id: 'signed',
      config: {
        headers: { 'X-Signed': '1' },
        params: { dualSidePosition: 'true', timestamp: 1, signature: 'abc' },
        data: undefined,
      },
    });

    await build()(config);

    expect(inner).toHaveBeenCalledTimes(1);
    expect(inner.mock.calls[0][0].params).toEqual({ dualSidePosition: 'true', timestamp: 1, signature: 'abc' });
    expect(inner.mock.calls[0][0].data).toBeUndefined();
  });

  // The point of the tag: a public endpoint costs a WebSocket hop to the device
  // and crypto on its JS thread to produce a signature nothing checks.
  // EXCHANGE-SERVER-27. The Tealstreet proxy credential is transport auth, set
  // by THIS process before the adapter runs. The browser owns the venue
  // signature and nothing else, but the merge took its headers wholesale — so
  // when the signing exchange there did not re-add ours, the request reached
  // CloudFront with no credential and came back 401 "Authorization header is
  // missing", indistinguishable from the venue rejecting the API key.
  it('keeps our proxy credential when the signer does not return one', async () => {
    const config = configFor('/api/v5/account/config');
    config.headers.set('Authorization', 'Bearer our-proxy-token');

    await build()(config);

    expect(inner.mock.calls[0][0].headers.get('Authorization')).toBe('Bearer our-proxy-token');
    expect(inner.mock.calls[0][0].headers.get('X-Signed')).toBe('1');
  });

  // The production case. An older browser bundle sends `Bearer ${token}` with no
  // token; axios trims it to "Bearer"; the edge answers 401 "Authorization
  // header is missing". A merge that accepts any non-empty string keeps it.
  it('replaces a trimmed empty bearer from the signer with ours', async () => {
    sign.mockResolvedValueOnce({
      id: 'x',
      config: { headers: { Authorization: 'Bearer' }, params: undefined, data: undefined },
    });
    const config = configFor('/api/v5/account/config');
    config.headers.set('Authorization', 'Bearer our-proxy-token');

    await build()(config);

    expect(inner.mock.calls[0][0].headers.get('Authorization')).toBe('Bearer our-proxy-token');
  });

  // Better to send nothing than a credential the edge will read as absent: the
  // 401 is then honest about what happened instead of blaming the venue.
  it('drops an unusable credential when we have no good one either', async () => {
    sign.mockResolvedValueOnce({
      id: 'x',
      config: { headers: { Authorization: 'Bearer' }, params: undefined, data: undefined },
    });

    await build()(configFor('/api/v5/account/config'));

    expect(inner.mock.calls[0][0].headers.get('Authorization')).toBeUndefined();
  });

  it('prefers the credential the signer returns over ours', async () => {
    sign.mockResolvedValueOnce({
      id: 'x',
      config: { headers: { Authorization: 'Bearer fresher-token' }, params: undefined, data: undefined },
    });
    const config = configFor('/api/v5/account/config');
    config.headers.set('Authorization', 'Bearer our-proxy-token');

    await build()(config);

    expect(inner.mock.calls[0][0].headers.get('Authorization')).toBe('Bearer fresher-token');
  });

  it('carries the Tealstreet-Authorization variant too', async () => {
    const config = configFor('/full/v1/get_authorized_builders');
    config.headers.set('Tealstreet-Authorization', 'Bearer grvt-proxy-token');

    await build()(config);

    expect(inner.mock.calls[0][0].headers.get('Tealstreet-Authorization')).toBe('Bearer grvt-proxy-token');
  });

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
