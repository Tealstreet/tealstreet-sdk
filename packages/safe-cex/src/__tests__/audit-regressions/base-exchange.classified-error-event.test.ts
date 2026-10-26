import { afterEach, describe, expect, test, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';

// Listeners (health monitor, toasts) branch on the typed kind instead of the text.
const makeExchange = () => new BaseExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
const typedError = () =>
  Object.assign(new Error('API key is invalid.'), { kind: 'authInvalid', outcome: 'rejected', venueCode: '10003' });

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("the 'error' event carries the typed classification", () => {
  test('on the main thread', () => {
    const exchange = makeExchange();
    const listener = vi.fn();
    exchange.emitter.on('error', listener);
    exchange.emitAndLogError(typedError());
    expect(listener.mock.calls[0][1].classified).toEqual({
      kind: 'authInvalid',
      outcome: 'rejected',
      venueCode: '10003',
    });
  });

  test('in a worker, and not for an unclassified error', () => {
    vi.stubGlobal('window', undefined);
    const exchange = makeExchange();
    const listener = vi.fn();
    exchange.emitter.on('error', listener);
    exchange.emitAndLogError(typedError());
    exchange.emitAndLogError(new Error('plain'));
    expect(listener.mock.calls[0][1].classified).toMatchObject({ kind: 'authInvalid' });
    expect(listener.mock.calls[1][1].classified).toBeUndefined();
  });
});
