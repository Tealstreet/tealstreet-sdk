import type { ExchangeAccountConfig, ExchangeOptions } from './types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { accountToExchangeOptions, createExchangeFromAccount } from './createExchangeFromAccount';
import { setAwsProxyMigrationEnabled } from './utils/transportResolution';

// Partial metadata for testing - only fields used by accountToExchangeOptions
interface TestMetadata {
  exchangeName: string;
  requiresCors: boolean;
  requiresPassphrase: boolean;
  requiresApplicationId: boolean;
  requiresApiKeyIndex: boolean;
  supportsVault: boolean;
  requiresOmniKeySeed: boolean;
  requiresParadexAddress: boolean;
  supportsSubaccount: boolean;
  enforceWsTunnel: boolean;
  privateProxyRegions: Record<string, string[]>;
}

// Mock the exchangeMetadata module. Keep the real `exchangeMetadata` record (the routing
// table needs its keys, now reachable via getReadFleetProxy) and only override the
// getExchangeMetadata function this suite controls.
vi.mock('./exchangeMetadata', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./exchangeMetadata')>()),
  getExchangeMetadata: vi.fn((exchange: string) => {
    // Return different metadata based on exchange
    const metadataMap: Record<string, TestMetadata> = {
      bybit: {
        exchangeName: 'bybit',
        requiresCors: false,
        requiresPassphrase: false,
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: true,
        enforceWsTunnel: false,
        privateProxyRegions: { SG: ['1.2.3.4'], TK: ['5.6.7.8'] },
      },
      okx: {
        exchangeName: 'okx',
        requiresCors: false,
        requiresPassphrase: true,
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: true,
        enforceWsTunnel: false,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      hyperliquid: {
        exchangeName: 'hyperliquid',
        requiresCors: true,
        requiresPassphrase: false,
        requiresApplicationId: true,
        requiresApiKeyIndex: false,
        supportsVault: true,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: false,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      dydx: {
        exchangeName: 'dydx',
        requiresCors: false,
        requiresPassphrase: false,
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: false,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      grvt: {
        exchangeName: 'grvt',
        requiresCors: false,
        requiresPassphrase: false,
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: false,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      paradex: {
        exchangeName: 'paradex',
        requiresCors: false,
        requiresPassphrase: false,
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: true,
        supportsSubaccount: false,
        enforceWsTunnel: false,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      arkham: {
        exchangeName: 'arkham',
        requiresCors: false,
        requiresPassphrase: false,
        requiresApplicationId: true,
        requiresApiKeyIndex: true,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: false,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      lighter: {
        exchangeName: 'lighter',
        requiresCors: false,
        requiresPassphrase: false,
        requiresApplicationId: true,
        requiresApiKeyIndex: true,
        supportsVault: false,
        requiresOmniKeySeed: true,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: true,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      asterdex: {
        exchangeName: 'asterdex',
        requiresCors: false,
        requiresPassphrase: false,
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: true,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
    };
    return metadataMap[exchange] || metadataMap.bybit;
  }),
}));

// Mock createExchange from index
vi.mock('./index', () => ({
  createExchange: vi.fn().mockResolvedValue({ name: 'mockExchange' }),
}));

describe('accountToExchangeOptions', () => {
  describe('basic credentials mapping', () => {
    it('should map apiKey and apiSecret to key and secret', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'test-api-key',
        apiSecret: 'test-api-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.key).toBe('test-api-key');
      expect(options.secret).toBe('test-api-secret');
    });

    it('should include passphrase when exchange requires it', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'okx',
        name: 'test-okx',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        passphrase: 'test-passphrase',
      };

      const options = accountToExchangeOptions(account);

      expect(options.passphrase).toBe('test-passphrase');
    });

    it('should not include passphrase when exchange does not require it', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-bybit',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        passphrase: 'test-passphrase',
      };

      const options = accountToExchangeOptions(account);

      expect(options.passphrase).toBeUndefined();
    });
  });

  describe('secondary API keys', () => {
    it('should map secondary_apiKey to secondary_key', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'primary-key',
        apiSecret: 'primary-secret',
        secondary_apiKey: 'secondary-key',
        secondary_apiSecret: 'secondary-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.secondary_key).toBe('secondary-key');
      expect(options.secondary_secret).toBe('secondary-secret');
    });

    it('should map secondary_passphrase when present', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'okx',
        name: 'test-account',
        apiKey: 'primary-key',
        apiSecret: 'primary-secret',
        secondary_apiKey: 'secondary-key',
        secondary_passphrase: 'secondary-pass',
      };

      const options = accountToExchangeOptions(account);

      expect(options.secondary_passphrase).toBe('secondary-pass');
    });

    it('should not include secondary fields when not provided', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'primary-key',
        apiSecret: 'primary-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.secondary_key).toBeUndefined();
      expect(options.secondary_secret).toBeUndefined();
    });

    // End-to-end: the per-exchange read-fleet region must thread from the account through
    // getReadFleetProxyUrl into options.secondaryProxyUrl (the unit test covers the region
    // function in isolation; this proves the wiring).
    describe('secondaryProxyUrl read-fleet region wiring (migration on)', () => {
      afterEach(() => setAwsProxyMigrationEnabled(false));

      const withSecondary = (
        exchange: ExchangeAccountConfig['exchange'],
        proxyRegion?: string,
      ): ExchangeAccountConfig => ({
        exchange,
        name: 'test-account',
        apiKey: 'primary-key',
        apiSecret: 'primary-secret',
        secondary_apiKey: 'secondary-key',
        secondary_apiSecret: 'secondary-secret',
        ...(proxyRegion ? { proxyRegion } : {}),
      });

      it('threads the per-exchange region into secondaryProxyUrl', () => {
        setAwsProxyMigrationEnabled(true);
        // okx declares the jp pool — SG must NOT override it to Singapore (parity with AWS).
        expect(accountToExchangeOptions(withSecondary('okx', 'SG')).secondaryProxyUrl).toBe(
          'https://read-fleet-jp.tealstreet.io/api/exchange_proxy',
        );
        // bybit declares the asia pool — unset region egresses Singapore.
        expect(accountToExchangeOptions(withSecondary('bybit')).secondaryProxyUrl).toBe(
          'https://read-fleet-sg.tealstreet.io/api/exchange_proxy',
        );
      });

      it('sets no secondaryProxyUrl without a secondary key, even with migration on', () => {
        setAwsProxyMigrationEnabled(true);
        const account: ExchangeAccountConfig = { exchange: 'bybit', name: 't', apiKey: 'k', apiSecret: 's' };
        expect(accountToExchangeOptions(account).secondaryProxyUrl).toBeUndefined();
      });
    });
  });

  describe('proxy settings', () => {
    it('should set tealstreetProxy when exchange requires CORS', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'hyperliquid',
        name: 'test-hl',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.tealstreetProxy).toBe(true);
    });

    it('should set proxyRegion from account when provided', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        proxyRegion: 'TK',
      };

      const options = accountToExchangeOptions(account);

      expect(options.proxyRegion).toBe('TK');
    });

    it('should auto-select first proxy region when CORS required and no region set', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'hyperliquid',
        name: 'test-hl',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.proxyRegion).toBe('SG');
    });

    it('should set tealstreetTunnel when tealstreetProxy is true', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'hyperliquid',
        name: 'test-hl',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.tealstreetTunnel).toBe(true);
    });

    it('should set tealstreetProxy when exchange enforces WS tunnel', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'lighter',
        name: 'test-lighter',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.tealstreetProxy).toBe(true);
    });

    it('should include tealstreetWsRoute when set in account', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        tealstreetWsRoute: true,
      };

      const options = accountToExchangeOptions(account);

      expect(options.tealstreetWsRoute).toBe(true);
    });
  });

  describe('testnet setting', () => {
    it('should include testnet when set to true', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        testnet: true,
      };

      const options = accountToExchangeOptions(account);

      expect(options.testnet).toBe(true);
    });

    it('should not include testnet when not set', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.testnet).toBeUndefined();
    });
  });

  describe('exchange-specific fields', () => {
    it('should include applicationId when exchange requires it', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'arkham',
        name: 'test-arkham',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        applicationId: 'app-123',
      };

      const options = accountToExchangeOptions(account);

      expect(options.applicationId).toBe('app-123');
    });

    it('should include applicationId for asterdex V3 compatibility', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'asterdex',
        name: 'test-asterdex',
        apiKey: '0xabcdefabcdefabcdefabcdefabcdefabcdefabcd',
        apiSecret: '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef',
        applicationId: '0x1234567890123456789012345678901234567890',
      };

      const options = accountToExchangeOptions(account);

      expect(options.applicationId).toBe('0x1234567890123456789012345678901234567890');
    });

    it('should include apiKeyIndex when exchange requires it', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'arkham',
        name: 'test-arkham',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        apiKeyIndex: '0',
      };

      const options = accountToExchangeOptions(account);

      expect(options.apiKeyIndex).toBe('0');
    });

    it('should include vault when exchange supports it', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'hyperliquid',
        name: 'test-hl',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        vault: 'vault-address',
      };

      const options = accountToExchangeOptions(account);

      expect(options.vault).toBe('vault-address');
    });

    it('should include omniKeySeed when exchange requires it', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'lighter',
        name: 'test-lighter',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        omniKeySeed: 'seed-value',
      };

      const options = accountToExchangeOptions(account);

      expect(options.omniKeySeed).toBe('seed-value');
    });

    it('should include paradexAddress and subkeyAddress for paradex', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'paradex',
        name: 'test-paradex',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        paradexAddress: '0xparadex',
        subkeyAddress: '0xsubkey',
      };

      const options = accountToExchangeOptions(account);

      expect(options.paradexAddress).toBe('0xparadex');
      expect(options.subkeyAddress).toBe('0xsubkey');
    });

    it('should include dydxAddress for dydx exchange', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'dydx',
        name: 'test-dydx',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        dydxAddress: 'dydx-address',
      };

      const options = accountToExchangeOptions(account);

      expect(options.dydxAddress).toBe('dydx-address');
    });

    it('should include grvt fields for grvt exchange', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'grvt',
        name: 'test-grvt',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        grvtSignerAddress: '0xsigner',
        grvtTradingAccountId: 'trading-123',
      };

      const options = accountToExchangeOptions(account);

      expect(options.grvtSignerAddress).toBe('0xsigner');
      expect(options.grvtTradingAccountId).toBe('trading-123');
    });

    it('should include subaccount when exchange supports it', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-bybit',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        subaccount: 'sub-1',
      };

      const options = accountToExchangeOptions(account);

      expect(options.subaccount).toBe('sub-1');
    });

    it('should include orderlyExchangeId when set', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        orderlyExchangeId: 'orderly-123',
      };

      const options = accountToExchangeOptions(account);

      expect(options.orderlyExchangeId).toBe('orderly-123');
    });
  });

  describe('overrides', () => {
    it('should apply overrides to options', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
      };

      const overrides: Partial<ExchangeOptions> = {
        testnet: true,
        publicDataOnly: true,
      };

      const options = accountToExchangeOptions(account, overrides);

      expect(options.testnet).toBe(true);
      expect(options.publicDataOnly).toBe(true);
    });

    it('should allow overrides to override account values', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'bybit',
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        testnet: false,
      };

      const overrides: Partial<ExchangeOptions> = {
        testnet: true,
      };

      const options = accountToExchangeOptions(account, overrides);

      expect(options.testnet).toBe(true);
    });
  });
});

describe('createExchangeFromAccount', () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    // Re-establish the mock after clearing
    const indexModule = await import('./index');
    vi.mocked(indexModule.createExchange).mockResolvedValue({ name: 'mockExchange' } as any);
  });

  it('should call createExchange with correct exchange name and options', async () => {
    // Import the createExchange mock
    const indexModule = await import('./index');
    const createExchange = indexModule.createExchange as any;

    const account: ExchangeAccountConfig = {
      exchange: 'bybit',
      name: 'test-account',
      apiKey: 'test-key',
      apiSecret: 'test-secret',
    };

    await createExchangeFromAccount(account);

    expect(createExchange).toHaveBeenCalledTimes(1);
    expect(createExchange).toHaveBeenCalledWith(
      'bybit',
      expect.objectContaining({
        key: 'test-key',
        secret: 'test-secret',
      }),
    );
  });

  it('should pass overrides to accountToExchangeOptions', async () => {
    // Import the createExchange mock
    const indexModule = await import('./index');
    const createExchange = indexModule.createExchange as any;

    const account: ExchangeAccountConfig = {
      exchange: 'bybit',
      name: 'test-account',
      apiKey: 'test-key',
      apiSecret: 'test-secret',
    };

    const overrides: Partial<ExchangeOptions> = {
      testnet: true,
    };

    await createExchangeFromAccount(account, overrides);

    expect(createExchange).toHaveBeenCalledWith(
      'bybit',
      expect.objectContaining({
        testnet: true,
      }),
    );
  });

  it('should return the exchange instance from createExchange', async () => {
    const account: ExchangeAccountConfig = {
      exchange: 'bybit',
      name: 'test-account',
      apiKey: 'test-key',
      apiSecret: 'test-secret',
    };

    const result = await createExchangeFromAccount(account);

    expect(result).toEqual({ name: 'mockExchange' });
  });
});
