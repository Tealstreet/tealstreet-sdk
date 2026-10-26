import { describe, expect, it, vi } from 'vitest';

import { AsterDEXExchange } from '../reference/asterdex/asterdex.exchange';
import {
  ASTERDEX_MAIN_WALLET_REQUIRED_HINT,
  ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE,
  isAsterDEXMainWalletError,
} from '../../exchanges/asterdex/asterdex.types';
import { DefaultStore } from '../../store/store.base';

describe('audit regression: AsterDEX V3 auth context', () => {
  it('requires the main wallet when API-wallet credentials are entered', async () => {
    const exchange = new AsterDEXExchange(
      'asterdex',
      {
        key: '0x1111111111111111111111111111111111111111',
        secret: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
      },
      new DefaultStore(),
    );
    vi.spyOn(exchange, 'log').mockImplementation(() => undefined);

    await expect(exchange.validateAccount()).rejects.toThrow(ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE);
  });

  it('composes the canonical hint into the full user-facing message', () => {
    // `isAsterDEXMainWalletError` relies on the hint appearing verbatim inside
    // the composed message. If someone edits the message without keeping the
    // hint as a prefix substring, this test fails before routing silently breaks.
    expect(ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE).toContain(ASTERDEX_MAIN_WALLET_REQUIRED_HINT);
  });

  describe('isAsterDEXMainWalletError', () => {
    it('matches the canonical error message thrown by the exchange class', () => {
      expect(isAsterDEXMainWalletError(ASTERDEX_MAIN_WALLET_REQUIRED_MESSAGE)).toBe(true);
    });

    it('matches legacy error phrasings so stored errors and older builds still route correctly', () => {
      // Errors thrown by earlier revisions of `validateAccount`. Removing them
      // here would silently misroute rejections coming from an out-of-date
      // deploy or a persisted error string.
      expect(
        isAsterDEXMainWalletError(
          'AsterDEX v3 keys require Main Wallet Address (applicationId / auth.userAddress)',
        ),
      ).toBe(true);
      expect(
        isAsterDEXMainWalletError('Missing auth.userAddress on AsterDEX account'),
      ).toBe(true);
    });

    it('rejects unrelated exchange errors', () => {
      expect(isAsterDEXMainWalletError('Bad API key, API secret or incorrect IP whitelist.')).toBe(false);
      expect(isAsterDEXMainWalletError('Passphrase error')).toBe(false);
      expect(isAsterDEXMainWalletError('Request failed')).toBe(false);
    });

    it('treats non-string input as a non-match', () => {
      expect(isAsterDEXMainWalletError(undefined)).toBe(false);
      expect(isAsterDEXMainWalletError(null)).toBe(false);
      expect(isAsterDEXMainWalletError('')).toBe(false);
      expect(isAsterDEXMainWalletError(42)).toBe(false);
    });
  });
});
