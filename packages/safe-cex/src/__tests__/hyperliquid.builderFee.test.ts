import { afterEach, describe, expect, it, vi } from 'vitest';

import { checkBuilderFeeApproved } from '../exchanges/hyperliquid/hyperliquid.builderFee';

const mockFetch = (impl: () => Promise<Partial<Response>> | never) => {
  vi.stubGlobal('fetch', vi.fn(impl) as unknown as typeof fetch);
};

describe('checkBuilderFeeApproved', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('reports approval when the builder fee is set', async () => {
    mockFetch(async () => ({ ok: true, status: 200, text: async () => '10' }));

    const result = await checkBuilderFeeApproved('0xabc');

    expect(result).toEqual({ approved: true, maxFeeRate: '0.01%' });
    expect(result.error).toBeUndefined();
  });

  it('reports a genuine denial with no error', async () => {
    mockFetch(async () => ({ ok: true, status: 200, text: async () => '0' }));

    const result = await checkBuilderFeeApproved('0xabc');

    expect(result.approved).toBe(false);
    expect(result.error).toBeUndefined();
  });

  it('distinguishes a non-2xx response from a denial', async () => {
    mockFetch(async () => ({ ok: false, status: 503, text: async () => 'Service Unavailable' }));

    const result = await checkBuilderFeeApproved('0xabc');

    expect(result.approved).toBe(false);
    expect(result.error).toContain('503');
  });

  it('distinguishes an unparseable body from a denial', async () => {
    // A CDN error page still arrives with a 200 in some edge configurations.
    mockFetch(async () => ({ ok: true, status: 200, text: async () => '<html>nope</html>' }));

    const result = await checkBuilderFeeApproved('0xabc');

    expect(result.approved).toBe(false);
    expect(result.error).toBe('Unexpected response from Hyperliquid');
  });

  it('distinguishes a network failure from a denial', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    mockFetch(() => {
      throw new Error('Failed to fetch');
    });

    const result = await checkBuilderFeeApproved('0xabc');

    expect(result.approved).toBe(false);
    expect(result.error).toBe('Failed to fetch');
  });
});
