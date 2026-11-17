import { expect, it } from 'vitest';

import { exchangeMetadata } from './exchangeMetadata';
import { resolveAccountTransport } from './transport/tealstreet/transportResolution';

it.each(['hyperliquid', 'hyperliquidspot', 'bitunix'] as const)(
  '%s declares the tunnel it already enforces without changing routing',
  (venue) => {
    const metadata = exchangeMetadata[venue];
    expect(metadata.enforceWsTunnel).toBe(true);
    expect(metadata.supportsTunnel).toBe(true);
    for (const preferences of [{}, { tealstreetTunnel: false }, { tealstreetProxy: false, tealstreetTunnel: false }]) {
      const resolved = resolveAccountTransport(preferences, metadata);
      expect(resolved.tealstreetTunnel).toBe(true);
      expect(resolved).toEqual(resolveAccountTransport(preferences, { ...metadata, supportsTunnel: false }));
    }
  },
);
