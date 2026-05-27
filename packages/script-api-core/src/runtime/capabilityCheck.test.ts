import { describe, expect, it } from 'vitest';

import type { Capabilities } from '../api/capabilities.types';

import {
  MAX_RUN_COMMAND_DEPTH,
  assertIdempotentForRetry,
  assertWithinStorageQuota,
  validateExchangeCapability,
  validateReadCapability,
  validateRunCommandCapability,
  validateWriteCapability,
} from './capabilityCheck';

const baseCaps = (overrides: Partial<Capabilities> = {}): Capabilities => ({
  accounts: ['mybybit'],
  writes: ['placeOrder'],
  idempotent: false,
  storage: { enabled: true, quotaBytes: 102400 },
  ...overrides,
});

describe('validateReadCapability', () => {
  it('allows everything when capabilities.reads is undefined (v1 default)', () => {
    expect(() => validateReadCapability('getBalance', baseCaps())).not.toThrow();
    expect(() => validateReadCapability('getOrderbook', baseCaps())).not.toThrow();
  });

  it('allows methods in the reads allowlist', () => {
    const caps = baseCaps({ reads: ['getBalance', 'getOrders'] });
    expect(() => validateReadCapability('getBalance', caps)).not.toThrow();
    expect(() => validateReadCapability('getOrders', caps)).not.toThrow();
  });

  it('denies methods not in the reads allowlist', () => {
    const caps = baseCaps({ reads: ['getBalance'] });
    expect(() => validateReadCapability('getOrderbook', caps)).toThrow(/capability denied: read 'getOrderbook'/);
  });

  it('denies all reads when the allowlist is an empty array', () => {
    const caps = baseCaps({ reads: [] });
    expect(() => validateReadCapability('getBalance', caps)).toThrow(/capability denied/);
  });
});

describe('validateWriteCapability', () => {
  it('allows methods in the writes allowlist', () => {
    const caps = baseCaps({ writes: ['placeOrder', 'cancelOrder'] });
    expect(() => validateWriteCapability('placeOrder', caps)).not.toThrow();
    expect(() => validateWriteCapability('cancelOrder', caps)).not.toThrow();
  });

  it('denies methods not in the writes allowlist', () => {
    const caps = baseCaps({ writes: ['placeOrder'] });
    expect(() => validateWriteCapability('cancelOrder', caps)).toThrow(/capability denied: write 'cancelOrder'/);
  });

  it('denies all writes when the allowlist is empty', () => {
    const caps = baseCaps({ writes: [] });
    expect(() => validateWriteCapability('placeOrder', caps)).toThrow(/capability denied/);
  });
});

describe('validateRunCommandCapability', () => {
  const enabledCaps = baseCaps({
    runCommand: { enabled: true, allowedVerbs: ['buy', 'sell', 'cancel'] },
  });

  it('denies when runCommand block is omitted', () => {
    expect(() => validateRunCommandCapability('buy 1', baseCaps(), 1)).toThrow(/runCommand is not enabled/);
  });

  it('denies when runCommand is explicitly disabled', () => {
    const caps = baseCaps({ runCommand: { enabled: false, allowedVerbs: ['buy'] } });
    expect(() => validateRunCommandCapability('buy 1', caps, 1)).toThrow(/runCommand is not enabled/);
  });

  it('allows verbs in the allowlist', () => {
    expect(() => validateRunCommandCapability('buy 1', enabledCaps, 1)).not.toThrow();
    expect(() => validateRunCommandCapability('cancel @x', enabledCaps, 1)).not.toThrow();
  });

  it("denies verbs not in the allowlist (the script's narrower-than-default case)", () => {
    // Exact-string assertion — load-bearing per SPEC §4.2 / Phase 2 brief.
    expect(() => validateRunCommandCapability('twap chase 1 BTC', enabledCaps, 1)).toThrow(
      "capability denied: runCommand verb 'twap' not in allowlist",
    );
  });

  it('denies empty / whitespace-only commands', () => {
    expect(() => validateRunCommandCapability('', enabledCaps, 1)).toThrow(/requires a verb/);
    expect(() => validateRunCommandCapability('   ', enabledCaps, 1)).toThrow(/requires a verb/);
  });

  it('allows depth up to MAX_RUN_COMMAND_DEPTH', () => {
    for (let depth = 1; depth <= MAX_RUN_COMMAND_DEPTH; depth++) {
      expect(() => validateRunCommandCapability('buy 1', enabledCaps, depth)).not.toThrow();
    }
  });

  it('denies when depth exceeds the cap', () => {
    expect(() => validateRunCommandCapability('buy 1', enabledCaps, MAX_RUN_COMMAND_DEPTH + 1)).toThrow(
      /recursion depth/,
    );
  });

  it('matches the verb on the first whitespace-separated token only', () => {
    // The `cancel` verb is allowed; trailing args don't matter.
    expect(() => validateRunCommandCapability('cancel @x,@y --reduce', enabledCaps, 1)).not.toThrow();
  });
});

describe('validateExchangeCapability', () => {
  it('is a no-op when capabilities.exchanges is undefined', () => {
    expect(() => validateExchangeCapability('bybit', baseCaps())).not.toThrow();
  });

  it('allows exchanges in the allowlist', () => {
    const caps = baseCaps({ exchanges: ['bybit', 'hyperliquid'] });
    expect(() => validateExchangeCapability('bybit', caps)).not.toThrow();
    expect(() => validateExchangeCapability('hyperliquid', caps)).not.toThrow();
  });

  it('denies exchanges not in the allowlist', () => {
    const caps = baseCaps({ exchanges: ['bybit'] });
    expect(() => validateExchangeCapability('hyperliquid', caps)).toThrow(/exchange 'hyperliquid' not in/);
  });
});

describe('assertWithinStorageQuota', () => {
  it('throws when storage is disabled', () => {
    const caps = baseCaps({ storage: { enabled: false, quotaBytes: 102400 } });
    expect(() => assertWithinStorageQuota(0, 10, caps)).toThrow(/storage is not enabled/);
  });

  it('allows writes that land at or below the quota', () => {
    const caps = baseCaps({ storage: { enabled: true, quotaBytes: 100 } });
    expect(() => assertWithinStorageQuota(50, 50, caps)).not.toThrow();
    expect(() => assertWithinStorageQuota(0, 100, caps)).not.toThrow();
  });

  it('denies writes that push past the quota', () => {
    const caps = baseCaps({ storage: { enabled: true, quotaBytes: 100 } });
    expect(() => assertWithinStorageQuota(50, 51, caps)).toThrow(/storage quota exceeded/);
  });
});

describe('assertIdempotentForRetry', () => {
  it('allows scripts marked idempotent: true', () => {
    const caps = baseCaps({ idempotent: true });
    expect(() => assertIdempotentForRetry('myScript', caps)).not.toThrow();
  });

  it('denies non-idempotent scripts with the exact spec error string', () => {
    const caps = baseCaps({ idempotent: false });
    expect(() => assertIdempotentForRetry('myScript', caps)).toThrow(
      // SPEC §4.3 / §7.5 — load-bearing exact-string assertion.
      "script 'myScript' is not marked idempotent; cannot retry",
    );
  });
});
