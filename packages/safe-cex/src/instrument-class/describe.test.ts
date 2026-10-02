import { describe, expect, it } from 'vitest';

import { describeInstrument, tradfiPreflightMessage } from './describe';

describe('describeInstrument', () => {
  it('names a crypto perpetual', () => {
    expect(
      describeInstrument('BTCUSDT', 'bybit', {
        kind: 'classified',
        assetClass: 'crypto',
        venueLabel: '',
      }),
    ).toBe('BTCUSDT on bybit: crypto perpetual');
  });

  it('names a tokenized equity with its underlying', () => {
    expect(
      describeInstrument('SHOPUSDT', 'bybit', {
        kind: 'classified',
        assetClass: 'equity',
        venueLabel: 'stock',
        underlyingTicker: 'SHOP',
      }),
    ).toBe('SHOPUSDT on bybit: tokenized equity (SHOP) — may require signing a trading agreement');
  });

  it('names a tokenized commodity with no underlying', () => {
    expect(
      describeInstrument('XAUUSDT', 'bybit', {
        kind: 'classified',
        assetClass: 'commodity',
        venueLabel: 'commodity',
      }),
    ).toBe('XAUUSDT on bybit: tokenized commodity — may require signing a trading agreement');
  });

  it('passes the reason through for an unknown classification', () => {
    expect(
      describeInstrument('SOMEUSDT', 'binance', {
        kind: 'unknown',
        reason: 'no instrument classifier implemented for binance',
      }),
    ).toBe('SOMEUSDT on binance: no instrument classifier implemented for binance');
  });
});

describe('tradfiPreflightMessage', () => {
  it('refuses a tokenized equity with the "may require" phrasing', () => {
    const msg = tradfiPreflightMessage('SHOPUSDT', 'bybit', {
      kind: 'classified',
      assetClass: 'equity',
      venueLabel: 'stock',
      underlyingTicker: 'SHOP',
    });
    expect(msg).toBe(
      "SHOPUSDT on bybit is a tokenized equity (SHOP). Opening a position may require an agreement you have not signed with the venue — place the order through the venue's own UI first to accept it.",
    );
  });

  it('returns undefined for crypto', () => {
    expect(
      tradfiPreflightMessage('BTCUSDT', 'bybit', {
        kind: 'classified',
        assetClass: 'crypto',
        venueLabel: '',
      }),
    ).toBeUndefined();
  });

  it('returns undefined for unknown — do NOT block on an unimplemented classifier', () => {
    expect(
      tradfiPreflightMessage('SOMEUSDT', 'binance', {
        kind: 'unknown',
        reason: 'no instrument classifier implemented for binance',
      }),
    ).toBeUndefined();
  });

  it('never contains "you can trade this" or "you cannot trade this"', () => {
    const msg = tradfiPreflightMessage('XAUUSDT', 'bybit', {
      kind: 'classified',
      assetClass: 'commodity',
      venueLabel: 'commodity',
    });
    expect(msg).toBeDefined();
    expect(msg!.toLowerCase()).not.toContain('you can trade');
    expect(msg!.toLowerCase()).not.toContain('you cannot trade');
  });
});
