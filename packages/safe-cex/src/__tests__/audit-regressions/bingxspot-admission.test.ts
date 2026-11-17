import type { ExchangeName, ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { exchangeRegistry } from '../../exchanges/registry';
import { assertBetaVenue, BETA_VENUES } from '../../exchanges/rust/betaVenues';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';

const name: ExchangeName = 'bingxspot';
describe('BingX Spot beta admission', () => {
  it('declares a separate enabled Spot name with no derivative features', () => {
    expect(exchangeMetadata[name]).toMatchObject({
      exchangeName: 'bingxspot',
      enabled: true,
      enabledInDev: true,
      enableInPreview: true,
      hasEmptyPositions: true,
      prettyName: 'BingX Spot',
    });
    expect(exchangeMetadata[name].features.positions).toBe(false);
  });
  it('keeps the existing signing contract and resolves the Rust factory', async () => {
    const options: ExchangeOptions = { key: '', secret: '' };
    expect(deriveBridgeConstruction(name, options)).toMatchObject({ spot: true, dispatchSigning: true });
    expect(deriveRemoteSigningPolicy(name)).toMatchObject({
      enabled: true,
      payloadSupported: false,
      protectedHeaders: ['X-BX-APIKEY'],
    });
    expect(BETA_VENUES).toContain(name);
    expect(() => assertBetaVenue(name)).not.toThrow();
    await expect(exchangeRegistry[name]()).resolves.toBeTypeOf('function');
  });
});
