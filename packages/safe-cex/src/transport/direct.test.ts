import { describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../__tests__/reference/bybit/bybit.exchange';
import { DefaultStore } from '../store/store.base';
import { directTransport } from './direct';
import { resolveTransportRouting, withPublicCorsProxy } from './registry';

describe('direct transport', () => {
  it('is the default in a realm that registered nothing', async () => {
    // A fresh registry module has nothing registered, whatever other suites did.
    vi.resetModules();
    const fresh = await import('./registry');
    expect(fresh.getTransport({}).name).toBe('direct');
    vi.resetModules();
  });

  it('sends REST straight to the venue with axios own adapter', () => {
    const exchange = new BybitExchange(
      'bybit',
      { key: 'k', secret: 's', transport: directTransport },
      new DefaultStore(),
    );
    try {
      expect(exchange.xhr.defaults.baseURL).toBe('https://api.bybit.com');
      expect(Array.isArray(exchange.xhr.defaults.adapter) || exchange.xhr.defaults.adapter === undefined).toBe(true);
    } finally {
      exchange.dispose();
    }
  });

  it('refuses an account that asks for Tealstreet routing instead of silently going direct', () => {
    expect(
      () =>
        new BybitExchange(
          'bybit',
          { key: 'k', secret: 's', tealstreetTunnel: true, transport: directTransport },
          new DefaultStore(),
        ),
    ).toThrow(/direct transport is active/);
  });

  it('resolves every socket to the venue, with no fallback or relay host', () => {
    const routing = resolveTransportRouting('bybit', { transport: directTransport });
    for (const channel of [routing.wsPublic, routing.wsPrivate, routing.wsBusiness]) {
      expect(channel).toEqual({ tunnelled: false });
    }
  });

  it('leaves one-off URLs unproxied', () => {
    expect(withPublicCorsProxy('https://api.bybit.com/v5/market/time', { transport: directTransport })).toBe(
      'https://api.bybit.com/v5/market/time',
    );
  });
});
