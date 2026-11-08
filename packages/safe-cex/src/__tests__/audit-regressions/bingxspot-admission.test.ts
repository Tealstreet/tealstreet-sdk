import type { ExchangeName, ExchangeOptions } from '../../types';

import { describe, expect, it } from 'vitest';

import { exchangeMetadata } from '../../exchangeMetadata';
import { exchangeRegistry } from '../../exchanges/registry';
import { assertBetaVenue, BETA_VENUES } from '../../exchanges/rust/betaVenues';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';

const name: ExchangeName = 'bingxspot';
describe('BingX Spot native-only admission', () => {
  it('declares a separate Spot name with production disabled', () => {
    expect(exchangeMetadata[name]).toMatchObject({
      exchangeName: 'bingxspot',
      enabled: false,
      enabledInDev: false,
      enableInPreview: false,
      hasEmptyPositions: true,
      prettyName: 'BingX Spot',
    });
    expect(exchangeMetadata[name].features.positions).toBe(false);
  });
  it('declares the BingX hosted-signing contract without enabling factories', async () => {
    const options: ExchangeOptions = { key: '', secret: '' };
    expect(deriveBridgeConstruction(name, options)).toMatchObject({ spot: true, dispatchSigning: true });
    expect(deriveRemoteSigningPolicy(name)).toMatchObject({
      enabled: true,
      payloadSupported: false,
      protectedHeaders: ['X-BX-APIKEY'],
    });
    expect(BETA_VENUES).not.toContain(name);
    expect(() => assertBetaVenue(name)).toThrowError(/unsupported/);
    await expect(exchangeRegistry[name]()).rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
  });
});
