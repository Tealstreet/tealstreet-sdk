import { describe, expect, it } from 'vitest';

import { isValidExchangeAccount, parseImportAccountData } from './parseImportAccountData';

describe('parseImportAccountData', () => {
  describe('valid single account (CEX)', () => {
    it('parses valid single account with apiKey/apiSecret', () => {
      const input = JSON.stringify({
        exchange: 'binance',
        apiKey: 'test-api-key',
        apiSecret: 'test-api-secret',
        name: 'My Account',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(true);
      expect(result.error).toBeUndefined();
      expect(result.data).toEqual({
        exchange: 'binance',
        apiKey: 'test-api-key',
        apiSecret: 'test-api-secret',
        name: 'My Account',
      });
    });

    it('parses account with additional optional fields', () => {
      const input = JSON.stringify({
        exchange: 'bybit',
        apiKey: 'key',
        apiSecret: 'secret',
        name: 'Bybit Account',
        passphrase: 'my-passphrase',
        testnet: true,
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(true);
      expect(result.data).toHaveProperty('passphrase', 'my-passphrase');
    });
  });

  describe('valid single account (DEX)', () => {
    it('parses valid account with privateKey', () => {
      const input = JSON.stringify({
        exchange: 'hyperliquid',
        privateKey: '0x1234567890abcdef',
        name: 'DEX Account',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(true);
      expect(result.data).toEqual({
        exchange: 'hyperliquid',
        privateKey: '0x1234567890abcdef',
        name: 'DEX Account',
      });
    });

    it('parses valid account with walletAddress', () => {
      const input = JSON.stringify({
        exchange: 'hyperliquid',
        walletAddress: '0xabc123',
        name: 'Read-only Account',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(true);
    });
  });

  describe('valid array of accounts', () => {
    it('parses array of valid accounts', () => {
      const input = JSON.stringify([
        {
          exchange: 'binance',
          apiKey: 'key1',
          apiSecret: 'secret1',
          name: 'Account 1',
        },
        {
          exchange: 'bybit',
          apiKey: 'key2',
          apiSecret: 'secret2',
          name: 'Account 2',
        },
      ]);

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(true);
      expect(Array.isArray(result.data)).toBe(true);
      expect((result.data as unknown[]).length).toBe(2);
    });
  });

  describe('invalid JSON', () => {
    it('returns error for invalid JSON', () => {
      const result = parseImportAccountData('not valid json');

      expect(result.isValid).toBe(false);
      expect(result.error).toBe('Invalid JSON format');
    });

    it('returns error for empty string', () => {
      const result = parseImportAccountData('');

      expect(result.isValid).toBe(false);
      expect(result.error).toBe('Invalid JSON format');
    });
  });

  describe('missing required fields', () => {
    it('returns error for missing name', () => {
      const input = JSON.stringify({
        exchange: 'binance',
        apiKey: 'key',
        apiSecret: 'secret',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('name');
    });

    it('returns error for missing exchange', () => {
      const input = JSON.stringify({
        name: 'My Account',
        apiKey: 'key',
        apiSecret: 'secret',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('exchange');
    });

    it('returns error for missing credentials (no apiKey/apiSecret and no privateKey/walletAddress)', () => {
      const input = JSON.stringify({
        exchange: 'binance',
        name: 'My Account',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('apiKey/apiSecret or privateKey/walletAddress');
    });

    it('returns error for partial CEX credentials (only apiKey)', () => {
      const input = JSON.stringify({
        exchange: 'binance',
        name: 'My Account',
        apiKey: 'key',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(false);
    });

    it('returns error for partial CEX credentials (only apiSecret)', () => {
      const input = JSON.stringify({
        exchange: 'binance',
        name: 'My Account',
        apiSecret: 'secret',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(false);
    });
  });

  describe('legacy "secret" field backwards compatibility', () => {
    it('treats "secret" as "apiSecret" for a single account', () => {
      const input = JSON.stringify({
        exchange: 'binance',
        apiKey: 'test-api-key',
        secret: 'legacy-secret-value',
        name: 'My Account',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(true);
      expect(result.error).toBeUndefined();
      expect((result.data as any).apiSecret).toBe('legacy-secret-value');
    });

    it('does not overwrite an existing apiSecret with secret', () => {
      const input = JSON.stringify({
        exchange: 'binance',
        apiKey: 'test-api-key',
        apiSecret: 'real-secret',
        secret: 'legacy-secret-value',
        name: 'My Account',
      });

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(true);
      expect((result.data as any).apiSecret).toBe('real-secret');
    });

    it('treats "secret" as "apiSecret" for each entry in an array', () => {
      const input = JSON.stringify([
        {
          exchange: 'binance',
          apiKey: 'k1',
          secret: 's1',
          name: 'A1',
        },
        {
          exchange: 'bybit',
          apiKey: 'k2',
          secret: 's2',
          name: 'A2',
        },
      ]);

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(true);
      const accounts = result.data as any[];
      expect(accounts[0].apiSecret).toBe('s1');
      expect(accounts[1].apiSecret).toBe('s2');
    });
  });

  describe('invalid array inputs', () => {
    it('returns error for empty array', () => {
      const result = parseImportAccountData('[]');

      expect(result.isValid).toBe(false);
      expect(result.error).toBe('Empty array - no accounts to import');
    });

    it('returns error with index for invalid account in array', () => {
      const input = JSON.stringify([
        {
          exchange: 'binance',
          apiKey: 'key',
          apiSecret: 'secret',
          name: 'Valid',
        },
        {
          exchange: 'binance',
          // Missing credentials
          name: 'Invalid',
        },
      ]);

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('Account #2');
    });
  });

  describe('invalid data types', () => {
    it('returns error for primitive values', () => {
      expect(parseImportAccountData('"string"').isValid).toBe(false);
      expect(parseImportAccountData('123').isValid).toBe(false);
      expect(parseImportAccountData('true').isValid).toBe(false);
      expect(parseImportAccountData('null').isValid).toBe(false);
    });

    it('returns error when array contains non-objects', () => {
      const input = JSON.stringify(['string', 123]);

      const result = parseImportAccountData(input);

      expect(result.isValid).toBe(false);
      expect(result.error).toContain('Account #1');
    });
  });
});

describe('isValidExchangeAccount', () => {
  describe('valid accounts', () => {
    it('returns true for valid CEX account', () => {
      const account = {
        exchange: 'binance',
        apiKey: 'key',
        apiSecret: 'secret',
        name: 'Account',
      };

      expect(isValidExchangeAccount(account)).toBe(true);
    });

    it('returns true for valid DEX account with privateKey', () => {
      const account = {
        exchange: 'hyperliquid',
        privateKey: '0x123',
        name: 'Account',
      };

      expect(isValidExchangeAccount(account)).toBe(true);
    });

    it('returns true for valid DEX account with walletAddress', () => {
      const account = {
        exchange: 'hyperliquid',
        walletAddress: '0xabc',
        name: 'Account',
      };

      expect(isValidExchangeAccount(account)).toBe(true);
    });
  });

  describe('invalid accounts', () => {
    it('returns false for null', () => {
      expect(isValidExchangeAccount(null)).toBe(false);
    });

    it('returns false for undefined', () => {
      expect(isValidExchangeAccount(undefined)).toBe(false);
    });

    it('returns false for non-objects', () => {
      expect(isValidExchangeAccount('string')).toBe(false);
      expect(isValidExchangeAccount(123)).toBe(false);
      expect(isValidExchangeAccount(true)).toBe(false);
      expect(isValidExchangeAccount([])).toBe(false);
    });

    it('returns false for missing name', () => {
      const account = {
        exchange: 'binance',
        apiKey: 'key',
        apiSecret: 'secret',
      };

      expect(isValidExchangeAccount(account)).toBe(false);
    });

    it('returns false for missing exchange', () => {
      const account = {
        name: 'Account',
        apiKey: 'key',
        apiSecret: 'secret',
      };

      expect(isValidExchangeAccount(account)).toBe(false);
    });

    it('returns false for missing credentials', () => {
      const account = {
        exchange: 'binance',
        name: 'Account',
      };

      expect(isValidExchangeAccount(account)).toBe(false);
    });

    it('returns false for partial CEX credentials', () => {
      expect(
        isValidExchangeAccount({
          exchange: 'binance',
          name: 'Account',
          apiKey: 'key',
        }),
      ).toBe(false);

      expect(
        isValidExchangeAccount({
          exchange: 'binance',
          name: 'Account',
          apiSecret: 'secret',
        }),
      ).toBe(false);
    });
  });
});
