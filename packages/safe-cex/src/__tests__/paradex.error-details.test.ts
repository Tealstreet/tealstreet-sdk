import type { Axios } from 'axios';

import { describe, expect, it, vi } from 'vitest';

import { ParadexExchange } from './reference/paradex/paradex.exchange';
import { DefaultStore } from '../store/store.base';

describe('Paradex error logging', () => {
  it('rethrows the venue error when it is circular, not a JSON.stringify failure', async () => {
    const exchange = new ParadexExchange(
      'paradex',
      { publicDataOnly: true, tuning: { consoleLogging: false } } as any,
      new DefaultStore(),
    );
    const venueError: Error & { originalError?: unknown } = new Error('auth not ready');
    venueError.originalError = venueError;
    exchange.xhr = { get: vi.fn().mockRejectedValue(venueError) } as unknown as Axios;
    exchange.jwtManager = {} as any;
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    await expect(exchange.fetchPositions()).rejects.toBe(venueError);
  });
});
