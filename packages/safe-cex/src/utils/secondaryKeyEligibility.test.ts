import { describe, expect, it } from 'vitest';

import { isEligibleForSecondaryKey } from './secondaryKeyEligibility';
import { TransportMetadata } from './transportResolution';

// resolveAccountTransport treats all-undefined metadata as "no forced proxy/tunnel".
const META = {} as TransportMetadata;

describe('isEligibleForSecondaryKey', () => {
  it('eligible: supported exchange on a non-Direct (proxied) primary', () => {
    expect(isEligibleForSecondaryKey('bybit', { tealstreetProxy: true }, META)).toBe(true);
  });

  it('eligible: supported exchange on a tunneled primary', () => {
    expect(isEligibleForSecondaryKey('bybit', { tealstreetTunnel: true }, META)).toBe(true);
  });

  it('not eligible: supported exchange but Direct primary', () => {
    expect(isEligibleForSecondaryKey('bybit', { tealstreetProxy: false, tealstreetTunnel: false }, META)).toBe(false);
  });

  it('not eligible: wallet/single-keypair venue even when proxied', () => {
    expect(isEligibleForSecondaryKey('hyperliquid', { tealstreetProxy: true }, META)).toBe(false);
  });
});
