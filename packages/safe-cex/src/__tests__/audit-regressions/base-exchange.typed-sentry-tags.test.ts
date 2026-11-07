import { afterEach, describe, expect, test, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';

// Command rejections report through emitAndLogError; in a worker its sentryData rides the 'error' event.
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('typed engine errors are tagged for Sentry', () => {
  test('in the worker branch of emitAndLogError', () => {
    vi.stubGlobal('window', undefined);
    const exchange = new BaseExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
    const listener = vi.fn();
    exchange.emitter.on('error', listener);
    exchange.emitAndLogError(
      Object.assign(new Error('API key is invalid.'), { kind: 'authInvalid', outcome: 'rejected', venueCode: '10003' }),
    );
    expect(listener.mock.calls[0][1].sentryData.tags).toMatchObject({
      exchange: 'bybit',
      error_kind: 'authInvalid',
      error_category: 'auth_invalid',
      error_outcome: 'rejected',
      venue_code: '10003',
    });
  });
});
