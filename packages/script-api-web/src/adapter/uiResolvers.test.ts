import { describe, expect, it } from 'vitest';

import { isQuickSizeSlot, parseUIToken, QUICK_SIZE_SLOTS, UI_TOKENS } from './uiResolvers';

describe('UI_TOKENS', () => {
  it('matches the token names the preprocessor substitutes', () => {
    expect(UI_TOKENS.click).toBe('$click');
    expect(UI_TOKENS.orderSize).toBe('$ordersize');
    expect(UI_TOKENS.quick(1)).toBe('$quick1');
    expect(UI_TOKENS.quick(7)).toBe('$quick7');
  });
});

describe('isQuickSizeSlot', () => {
  it('accepts exactly 1..7', () => {
    for (const slot of QUICK_SIZE_SLOTS) expect(isQuickSizeSlot(slot)).toBe(true);
  });

  it('rejects out-of-range and non-integers', () => {
    for (const bad of [0, 8, -1, 1.5, NaN, Infinity]) {
      expect(isQuickSizeSlot(bad)).toBe(false);
    }
  });
});

describe('parseUIToken', () => {
  // $click returns a bare numeric string; sizes return a $-prefixed token.
  it('parses both resolver token shapes', () => {
    expect(parseUIToken('50000')).toBe(50000);
    expect(parseUIToken('$100')).toBe(100);
    expect(parseUIToken('$100.50')).toBe(100.5);
  });

  it('treats a missing or empty value as unavailable', () => {
    expect(parseUIToken(undefined)).toBeUndefined();
    expect(parseUIToken('')).toBeUndefined();
    expect(parseUIToken('   ')).toBeUndefined();
  });

  // A percent-sized quick button has no notional meaning without a position,
  // so it must read as "unavailable" rather than as the bare number.
  it('rejects percent tokens rather than returning the number', () => {
    expect(parseUIToken('50%')).toBeUndefined();
    expect(parseUIToken('$50%')).toBeUndefined();
  });

  it('returns undefined for non-numeric junk', () => {
    expect(parseUIToken('abc')).toBeUndefined();
    expect(parseUIToken('$')).toBeUndefined();
  });

  it('tolerates surrounding whitespace', () => {
    expect(parseUIToken('  $100  ')).toBe(100);
  });

  // `k`/`m` quick buttons are multiplied out by the host before tokenizing,
  // so the parser only ever sees the expanded number.
  it('parses expanded k/m quick-button sizes', () => {
    expect(parseUIToken('$5000')).toBe(5000);
    expect(parseUIToken('$5000000')).toBe(5000000);
  });

  // toFixed(2) on a very large notional yields exponential form.
  it('parses exponential notation from toFixed on huge notionals', () => {
    expect(parseUIToken('$1e+21')).toBe(1e21);
  });

  // Documents a known Number() coercion: hex parses. Unreachable from the
  // real resolvers (every token is String(finite)), so it is recorded rather
  // than guarded — if a token ever arrives from elsewhere, this is the shape
  // to distrust.
  it('inherits Number() hex coercion', () => {
    expect(parseUIToken('$0x10')).toBe(16);
  });
});
