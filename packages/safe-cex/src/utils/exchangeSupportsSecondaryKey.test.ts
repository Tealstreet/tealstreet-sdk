import { describe, expect, it } from 'vitest';

import { exchangeSupportsSecondaryKey, exchangesWithoutSecondaryKey } from './exchangeSupportsSecondaryKey';

describe('exchangeSupportsSecondaryKey', () => {
  it('is true for HMAC key+secret CEXes', () => {
    expect(exchangeSupportsSecondaryKey('bybit')).toBe(true);
    expect(exchangeSupportsSecondaryKey('binance')).toBe(true);
    expect(exchangeSupportsSecondaryKey('okx')).toBe(true);
  });

  it('is true for asterdex — V3 agent model supports a distinct read-only agent wallet', () => {
    expect(exchangeSupportsSecondaryKey('asterdex')).toBe(true);
  });

  it('is false for wallet/private-key and Orderly-ed25519 venues', () => {
    expect(exchangeSupportsSecondaryKey('hyperliquid')).toBe(false);
    expect(exchangeSupportsSecondaryKey('woofi')).toBe(false);
    expect(exchangeSupportsSecondaryKey('what')).toBe(false);
  });

  it('is false for tealstreet-dex (Orderly ed25519 family)', () => {
    expect(exchangeSupportsSecondaryKey('tealstreet-dex')).toBe(false);
  });

  it('is false for session-login venues where a distinct read key cannot authenticate', () => {
    expect(exchangeSupportsSecondaryKey('breakoutprop')).toBe(false);
  });

  it('every excluded entry is unique', () => {
    expect(new Set(exchangesWithoutSecondaryKey).size).toBe(exchangesWithoutSecondaryKey.length);
  });
});
