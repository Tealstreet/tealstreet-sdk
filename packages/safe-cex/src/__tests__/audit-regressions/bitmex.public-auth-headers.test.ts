import type { ExchangeOptions } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { createAPI } from '../../exchanges/bitmex/bitmex.api';
import { ENDPOINTS } from '../../exchanges/bitmex/bitmex.types';

const makeExchange = () =>
  ({
    name: 'bitmex',
    emitter: { emit: vi.fn() },
    grafanaEndpointsEvery: [],
    grafanaEndpointsPeriodic: [],
  }) as any;

describe('BitMEX public REST auth headers', () => {
  it('does not send an empty api-key header for public-data-only requests', async () => {
    const adapter = vi.fn().mockResolvedValue({
      data: [],
      status: 200,
      statusText: 'OK',
      headers: {},
      config: {},
    });
    const api = createAPI({ key: '', secret: '', publicDataOnly: true } as ExchangeOptions, makeExchange());
    api.defaults.adapter = adapter;

    await api.get(ENDPOINTS.MARKETS);

    const request = adapter.mock.calls[0][0];
    expect(request.headers.get('api-key')).toBeUndefined();
    expect(request.headers.get('api-expires')).toBeUndefined();
    expect(request.headers.get('api-signature')).toBeUndefined();
  });
});
