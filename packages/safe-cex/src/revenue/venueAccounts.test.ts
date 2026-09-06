import type { ExchangeOptions } from '../types';

import { describe, expect, it } from 'vitest';

import { isValidVenueAccountId, resolveVenueAccount, revenueVenueOf } from './venueAccounts';

const options = (extra: Partial<ExchangeOptions>): ExchangeOptions =>
  ({ key: '0xKEY', secret: '', ...extra }) as ExchangeOptions;

const MASTER = `0x${'1'.repeat(40)}`;
const VAULT = `0x${'2'.repeat(40)}`;

describe('revenueVenueOf', () => {
  it('maps a revenue venue to itself', () => {
    expect(revenueVenueOf('hyperliquid')).toBe('hyperliquid');
  });

  it('folds hyperliquid spot onto the same venue, because it is the same wallet', () => {
    expect(revenueVenueOf('hyperliquidspot')).toBe('hyperliquid');
  });

  it('names no venue for an exchange we cannot yet earn on', () => {
    expect(revenueVenueOf('bybit')).toBeUndefined();
  });
});

describe('resolveVenueAccount', () => {
  it('reads the resolved master address', () => {
    expect(resolveVenueAccount('hyperliquid', options({ applicationId: MASTER.toUpperCase() }))).toEqual({
      venue: 'hyperliquid',
      venueAccountId: MASTER,
    });
  });

  it('prefers the vault, because a vault order fills AS the vault', () => {
    expect(resolveVenueAccount('hyperliquid', options({ applicationId: MASTER, vault: VAULT }))).toEqual({
      venue: 'hyperliquid',
      venueAccountId: VAULT,
    });
  });

  it('falls back to the key when start() never resolved an address', () => {
    // `applicationId` comes from an info call that can time out, and when it
    // does the account has no id at all — "not yet" becomes silently "never",
    // client-side, where nothing can see it. The key may be an AGENT wallet
    // that never appears in the feed; an inert row costs nothing and a missing
    // one costs the affiliate everything.
    expect(resolveVenueAccount('hyperliquid', options({ key: MASTER }))).toEqual({
      venue: 'hyperliquid',
      venueAccountId: MASTER,
    });
  });

  it('still refuses a key that is not an address', () => {
    // The format check is what makes the fallback safe.
    expect(resolveVenueAccount('hyperliquid', options({ key: 'not-an-address' }))).toBeUndefined();
  });

  it('prefers the resolved address over the key', () => {
    expect(resolveVenueAccount('hyperliquid', options({ key: VAULT, applicationId: MASTER }))).toEqual({
      venue: 'hyperliquid',
      venueAccountId: MASTER,
    });
  });

  it('treats a blank resolved value as unresolved', () => {
    expect(resolveVenueAccount('hyperliquid', options({ key: '', applicationId: '   ' }))).toBeUndefined();
  });

  it('yields nothing for an exchange with no revenue venue', () => {
    expect(resolveVenueAccount('bybit', options({ applicationId: MASTER }))).toBeUndefined();
  });

  it('refuses an id that is not a real address', () => {
    // The format check is load-bearing, not hygiene: first claim wins with no
    // signature challenge, so anything that cannot appear in the feed must not
    // be claimable at all.
    expect(resolveVenueAccount('hyperliquid', options({ applicationId: '0xAbC' }))).toBeUndefined();
  });

  it('accepts a real 20-byte address', () => {
    const address = `0x${'a1b2'.repeat(10)}`;
    expect(resolveVenueAccount('hyperliquid', options({ applicationId: address.toUpperCase() }))).toEqual({
      venue: 'hyperliquid',
      venueAccountId: address,
    });
  });
});

describe('isValidVenueAccountId', () => {
  const address = `0x${'0'.repeat(40)}`;

  it('accepts a lowercased 20-byte address', () => {
    expect(isValidVenueAccountId('hyperliquid', address)).toBe(true);
  });

  it.each([
    ['too short', `0x${'0'.repeat(39)}`],
    ['too long', `0x${'0'.repeat(41)}`],
    ['not lowercased', `0x${'A'.repeat(40)}`],
    ['no prefix', '0'.repeat(40)],
    ['trailing junk', `${address} `],
  ])('refuses one that is %s', (_why, candidate) => {
    expect(isValidVenueAccountId('hyperliquid', candidate)).toBe(false);
  });
});
