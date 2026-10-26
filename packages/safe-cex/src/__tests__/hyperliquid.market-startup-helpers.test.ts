import { describe, expect, it, vi } from 'vitest';

import {
  fetchCoreMetaAndAssetCtxsWithFallback,
  fetchHip3MetaAndAssetCtxsWithRetry,
} from './reference/hyperliquid/hyperliquid.market-startup-helpers';

const createAxiosMock = (responses: Array<{ data: unknown } | Error>) => {
  const queue = [...responses];
  return {
    post: vi.fn().mockImplementation(async () => {
      const next = queue.shift();
      if (next instanceof Error) {
        throw next;
      }
      return next ?? { data: null };
    }),
  };
};

describe('hyperliquid.market-startup-helpers', () => {
  it('falls back to official API when primary core response is malformed', async () => {
    const validTuple = [{ universe: [] }, []];
    const infoXhr = createAxiosMock([{ data: { invalid: true } }]);
    const xhr = createAxiosMock([{ data: validTuple }]);
    const log = vi.fn();

    const result = await fetchCoreMetaAndAssetCtxsWithFallback({
      infoXhr: infoXhr as any,
      xhr: xhr as any,

      log,
    });

    expect(result).toEqual(validTuple);
    expect(infoXhr.post).toHaveBeenCalledTimes(1);
    expect(xhr.post).toHaveBeenCalledTimes(1);
    expect(log).toHaveBeenCalledWith(
      expect.stringContaining('Retrying core markets fetch via fallback'),
      expect.anything(),
    );
  });

  it('throws when both primary and fallback core responses are invalid', async () => {
    const infoXhr = createAxiosMock([{ data: { invalid: true } }]);
    const xhr = createAxiosMock([{ data: { stillInvalid: true } }]);
    const log = vi.fn();

    await expect(
      fetchCoreMetaAndAssetCtxsWithFallback({
        infoXhr: infoXhr as any,
        xhr: xhr as any,

        log,
      }),
    ).rejects.toThrow('Failed to fetch valid core markets response from primary and fallback sources');
  });

  it('retries HIP-3 tuple fetch once on malformed response', async () => {
    const validTuple = [{ universe: [] }, []];
    const xhr = createAxiosMock([{ data: { malformed: true } }, { data: validTuple }]);
    const log = vi.fn();

    const result = await fetchHip3MetaAndAssetCtxsWithRetry({
      xhr: xhr as any,
      dexName: 'vntls',
      log,
    });

    expect(result).toEqual(validTuple);
    expect(xhr.post).toHaveBeenCalledTimes(2);
  });

  it('returns null when HIP-3 fetch fails on both attempts', async () => {
    const xhr = createAxiosMock([new Error('network 1'), new Error('network 2')]);
    const log = vi.fn();

    const result = await fetchHip3MetaAndAssetCtxsWithRetry({
      xhr: xhr as any,
      dexName: 'vntls',
      log,
    });

    expect(result).toBeNull();
    expect(xhr.post).toHaveBeenCalledTimes(2);
  });
});
