import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as fs from 'fs';
import type { Config, ExchangeAccountConfig } from './config';

// Mock the fs module
vi.mock('fs', () => ({
  chmodSync: vi.fn(),
  existsSync: vi.fn(),
  mkdirSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
}));

// Mock the os module for homedir
vi.mock('os', () => ({
  homedir: vi.fn(() => '/home/testuser'),
}));

// Import after mocking
import {
  loadConfig,
  saveConfig,
  getAccount,
  addAccount,
  removeAccount,
  setDefaultAccount,
  setDefaultSymbol,
  listAccounts,
  updateAccount,
  getActiveAccounts,
  getConfigDir,
  getConfigFilePath,
} from './config';

describe('config', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('getConfigDir', () => {
    it('returns the config directory path', () => {
      const dir = getConfigDir();
      expect(dir).toBe('/home/testuser/.tealstreet');
    });
  });

  describe('getConfigFilePath', () => {
    it('returns the config file path', () => {
      const path = getConfigFilePath();
      expect(path).toContain('.tealstreet');
      expect(path).toContain('config.json');
    });
  });

  describe('loadConfig', () => {
    it('returns empty accounts array when config file does not exist', () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      const config = loadConfig();

      expect(config).toEqual({ accounts: [] });
    });

    it('loads config from file when it exists', () => {
      const mockConfig: Config = {
        defaultAccount: 'test-account',
        accounts: [{ name: 'test-account', exchange: 'bybit', apiKey: 'key', apiSecret: 'secret' }],
      };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify(mockConfig));

      const config = loadConfig();

      expect(config).toEqual(mockConfig);
    });

    it('returns empty config on invalid JSON', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue('not valid json');

      const config = loadConfig();

      expect(config).toEqual({ accounts: [] });
    });

    it('creates config directory if it does not exist', () => {
      vi.mocked(fs.existsSync).mockImplementation((path) => {
        if (typeof path === 'string' && path.includes('config.json')) return false;
        return false;
      });

      loadConfig();

      expect(fs.mkdirSync).toHaveBeenCalledWith(expect.any(String), { recursive: true, mode: 0o700 });
    });
  });

  describe('saveConfig', () => {
    it('writes config to file as formatted JSON', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);

      const config: Config = {
        defaultAccount: 'test',
        accounts: [{ name: 'test', exchange: 'binance', apiKey: 'key', apiSecret: 'secret' }],
      };

      saveConfig(config);

      expect(fs.writeFileSync).toHaveBeenCalledWith(
        expect.stringContaining('config.json'),
        JSON.stringify(config, null, 2),
        { mode: 0o600 },
      );
    });

    it('keeps the credentials file owner-only, including one written before', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);

      saveConfig({ accounts: [] });

      expect(fs.chmodSync).toHaveBeenCalledWith(expect.stringContaining('config.json'), 0o600);
    });

    it('creates directory if it does not exist', () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);

      saveConfig({ accounts: [] });

      expect(fs.mkdirSync).toHaveBeenCalledWith(expect.any(String), { recursive: true, mode: 0o700 });
    });
  });

  describe('getAccount', () => {
    it('returns account by name', () => {
      const account: ExchangeAccountConfig = {
        name: 'my-account',
        exchange: 'bybit',
        apiKey: 'key',
        apiSecret: 'secret',
      };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [account] }));

      const result = getAccount('my-account');

      expect(result).toEqual(account);
    });

    it('returns default account when no name provided', () => {
      const account: ExchangeAccountConfig = {
        name: 'default-account',
        exchange: 'bybit',
        apiKey: 'key',
        apiSecret: 'secret',
      };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(
        JSON.stringify({ defaultAccount: 'default-account', accounts: [account] }),
      );

      const result = getAccount();

      expect(result).toEqual(account);
    });

    it('returns first account if no default set', () => {
      const account: ExchangeAccountConfig = {
        name: 'first-account',
        exchange: 'bybit',
        apiKey: 'key',
        apiSecret: 'secret',
      };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [account] }));

      const result = getAccount();

      expect(result).toEqual(account);
    });

    it('returns undefined for non-existent account', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [] }));

      const result = getAccount('non-existent');

      expect(result).toBeUndefined();
    });
  });

  describe('addAccount', () => {
    it('adds new account to config', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [] }));

      const newAccount: ExchangeAccountConfig = {
        name: 'new-account',
        exchange: 'binance',
        apiKey: 'key',
        apiSecret: 'secret',
      };

      addAccount(newAccount);

      expect(fs.writeFileSync).toHaveBeenCalledWith(
        expect.any(String),
        expect.stringContaining('new-account'),
        expect.anything(),
      );
    });

    it('updates existing account with same name', () => {
      const existingAccount: ExchangeAccountConfig = {
        name: 'test-account',
        exchange: 'bybit',
        apiKey: 'old-key',
        apiSecret: 'old-secret',
      };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [existingAccount] }));

      const updatedAccount: ExchangeAccountConfig = {
        name: 'test-account',
        exchange: 'bybit',
        apiKey: 'new-key',
        apiSecret: 'new-secret',
      };

      addAccount(updatedAccount);

      const writeCall = vi.mocked(fs.writeFileSync).mock.calls[0];
      const savedConfig = JSON.parse(writeCall[1] as string);
      expect(savedConfig.accounts).toHaveLength(1);
      expect(savedConfig.accounts[0].apiKey).toBe('new-key');
    });

    it('sets first account as default', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [] }));

      const newAccount: ExchangeAccountConfig = {
        name: 'first-account',
        exchange: 'binance',
        apiKey: 'key',
        apiSecret: 'secret',
      };

      addAccount(newAccount);

      const writeCall = vi.mocked(fs.writeFileSync).mock.calls[0];
      const savedConfig = JSON.parse(writeCall[1] as string);
      expect(savedConfig.defaultAccount).toBe('first-account');
    });
  });

  describe('removeAccount', () => {
    it('tombstones account by name (sets deletedAt, keeps row for sync)', () => {
      const account: ExchangeAccountConfig = {
        name: 'to-remove',
        exchange: 'bybit',
        apiKey: 'key',
        apiSecret: 'secret',
      };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [account] }));

      const result = removeAccount('to-remove');

      expect(result).toBe(true);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls[0];
      const savedConfig = JSON.parse(writeCall[1] as string);
      // Row kept as tombstone so cross-device sync (CLI ↔ web) propagates the delete.
      expect(savedConfig.accounts).toHaveLength(1);
      expect(savedConfig.accounts[0].name).toBe('to-remove');
      expect(typeof savedConfig.accounts[0].deletedAt).toBe('number');
      expect(typeof savedConfig.accounts[0].updatedAt).toBe('number');
    });

    it('returns false when the name does not exist locally (default — no synthetic tombstone)', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [] }));

      const result = removeAccount('non-existent');

      // User-facing default: a typo on `tealstreet account remove` reports
      // "not found" instead of silently creating a junk tombstone row.
      expect(result).toBe(false);
      expect(fs.writeFileSync).not.toHaveBeenCalled();
    });

    it('creates a tombstone-only row when opt-in flag is set', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [] }));

      const result = removeAccount('non-existent', { createTombstoneIfMissing: true });

      // Listener-sync path: a delete that originated remotely for an account
      // the CLI never had still propagates forward.
      expect(result).toBe(true);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls[0];
      const savedConfig = JSON.parse(writeCall[1] as string);
      expect(savedConfig.accounts).toHaveLength(1);
      expect(savedConfig.accounts[0]).toMatchObject({
        name: 'non-existent',
        deletedAt: expect.any(Number),
        updatedAt: expect.any(Number),
      });
    });

    it('updates default account when removing current default', () => {
      const accounts: ExchangeAccountConfig[] = [
        { name: 'default', exchange: 'bybit', apiKey: 'key1', apiSecret: 'secret1' },
        { name: 'other', exchange: 'binance', apiKey: 'key2', apiSecret: 'secret2' },
      ];
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ defaultAccount: 'default', accounts }));

      removeAccount('default');

      const writeCall = vi.mocked(fs.writeFileSync).mock.calls[0];
      const savedConfig = JSON.parse(writeCall[1] as string);
      // Tombstoned default falls through to the next live account.
      expect(savedConfig.defaultAccount).toBe('other');
    });
  });

  describe('setDefaultAccount', () => {
    it('sets default account when account exists', () => {
      const account: ExchangeAccountConfig = {
        name: 'test-account',
        exchange: 'bybit',
        apiKey: 'key',
        apiSecret: 'secret',
      };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [account] }));

      const result = setDefaultAccount('test-account');

      expect(result).toBe(true);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls[0];
      const savedConfig = JSON.parse(writeCall[1] as string);
      expect(savedConfig.defaultAccount).toBe('test-account');
    });

    it('returns false when account does not exist', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [] }));

      const result = setDefaultAccount('non-existent');

      expect(result).toBe(false);
    });
  });

  describe('setDefaultSymbol', () => {
    it('sets default symbol', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [] }));

      setDefaultSymbol('ETHUSDT');

      const writeCall = vi.mocked(fs.writeFileSync).mock.calls[0];
      const savedConfig = JSON.parse(writeCall[1] as string);
      expect(savedConfig.defaultSymbol).toBe('ETHUSDT');
    });
  });

  describe('listAccounts', () => {
    it('returns all accounts', () => {
      const accounts: ExchangeAccountConfig[] = [
        { name: 'acc1', exchange: 'bybit', apiKey: 'key1', apiSecret: 'secret1' },
        { name: 'acc2', exchange: 'binance', apiKey: 'key2', apiSecret: 'secret2' },
      ];
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts }));

      const result = listAccounts();

      expect(result).toEqual(accounts);
    });
  });

  describe('updateAccount', () => {
    it('updates account fields', () => {
      const account: ExchangeAccountConfig = {
        name: 'test-account',
        exchange: 'bybit',
        apiKey: 'key',
        apiSecret: 'secret',
        active: false,
      };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [account] }));

      const result = updateAccount('test-account', { active: true });

      expect(result).toBe(true);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls[0];
      const savedConfig = JSON.parse(writeCall[1] as string);
      expect(savedConfig.accounts[0].active).toBe(true);
    });

    it('returns false for non-existent account', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts: [] }));

      const result = updateAccount('non-existent', { active: true });

      expect(result).toBe(false);
    });
  });

  describe('getActiveAccounts', () => {
    it('returns only active accounts', () => {
      const accounts: ExchangeAccountConfig[] = [
        { name: 'active1', exchange: 'bybit', apiKey: 'key1', apiSecret: 'secret1', active: true },
        { name: 'inactive', exchange: 'binance', apiKey: 'key2', apiSecret: 'secret2', active: false },
        { name: 'active2', exchange: 'okx', apiKey: 'key3', apiSecret: 'secret3', active: true },
      ];
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts }));

      const result = getActiveAccounts();

      expect(result).toHaveLength(2);
      expect(result.map((a) => a.name)).toEqual(['active1', 'active2']);
    });

    it('returns empty array when no accounts are active', () => {
      const accounts: ExchangeAccountConfig[] = [
        { name: 'acc1', exchange: 'bybit', apiKey: 'key1', apiSecret: 'secret1' },
      ];
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ accounts }));

      const result = getActiveAccounts();

      expect(result).toHaveLength(0);
    });
  });
});
