import type { ExchangeAccountConfig, ExchangeOptions } from './types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import './transport/registerTealstreetTransport';

import { accountToExchangeOptions, createExchangeFromAccount } from './createExchangeFromAccount';

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
  supportsSpotToggle?: boolean;
  supportsOptionsToggle?: boolean;
  supportsInverseToggle?: boolean;
}

// Mock the exchangeMetadata module. Keep the real `exchangeMetadata` record (the routing
// table needs its keys) and only override the
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
        supportsSpotToggle: true,
        supportsOptionsToggle: true,
        supportsInverseToggle: true,
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
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: true,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: false,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      binanceinverse: {
        exchangeName: 'binanceinverse',
        requiresCors: true,
        requiresPassphrase: false,
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: false,
        privateProxyRegions: { JA: ['5.6.7.8'] },
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
      kucoin: {
        exchangeName: 'kucoin',
        requiresCors: false,
        requiresPassphrase: true,
        requiresApplicationId: false,
        requiresApiKeyIndex: false,
        supportsVault: false,
        requiresOmniKeySeed: false,
        requiresParadexAddress: false,
        supportsSubaccount: false,
        enforceWsTunnel: true,
        privateProxyRegions: { SG: ['1.2.3.4'] },
      },
      kucoinspot: {
        exchangeName: 'kucoinspot',
        requiresCors: false,
        requiresPassphrase: true,
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
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  describe('product toggles', () => {
    const account = (exchange: string, toggles: Partial<ExchangeAccountConfig>): ExchangeAccountConfig => ({
      exchange: exchange as ExchangeAccountConfig['exchange'],
      name: 'toggles',
      apiKey: 'k',
      apiSecret: 's',
      ...toggles,
    });

    it.each(['enableSpotMarkets', 'enableOptionMarkets', 'enableInverseMarkets'] as const)(
      'forwards %s when the venue offers the toggle and the account turned it on',
      (toggle) => {
        expect(accountToExchangeOptions(account('bybit', { [toggle]: true }))[toggle]).toBe(true);
      },
    );

    it.each(['enableSpotMarkets', 'enableOptionMarkets', 'enableInverseMarkets'] as const)(
      'drops %s on a venue that does not offer the toggle',
      (toggle) => {
        expect(accountToExchangeOptions(account('okx', { [toggle]: true }))[toggle]).toBeUndefined();
      },
    );

    it.each(['enableSpotMarkets', 'enableOptionMarkets', 'enableInverseMarkets'] as const)(
      'leaves %s off by default',
      (toggle) => {
        expect(accountToExchangeOptions(account('bybit', {}))[toggle]).toBeUndefined();
      },
    );
  });

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

    // End-to-end: a configured secondary key must thread the AWS public CORS host into
    // options.secondaryProxyUrl, and must NOT set it when no secondary key exists.
    describe('secondaryProxyUrl wiring', () => {
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

      it('threads the public CORS host into secondaryProxyUrl, regardless of region', () => {
        // Was AWS's exchange-proxy.jp; moved to the edge Worker's /api/public_proxy (AWS_EXIT.md 3d).
        const PUBLIC = 'https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy';
        expect(accountToExchangeOptions(withSecondary('okx', 'SG')).secondaryProxyUrl).toBe(PUBLIC);
        expect(accountToExchangeOptions(withSecondary('bybit')).secondaryProxyUrl).toBe(PUBLIC);
      });

      it('sets no secondaryProxyUrl without a secondary key', () => {
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

    it('should force Binance inverse through proxy and tunnel because COIN-M writes fail browser CORS', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'binanceinverse',
        name: 'test-binance-inverse',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
      };

      const options = accountToExchangeOptions(account);

      expect(options.tealstreetProxy).toBe(true);
      expect(options.tealstreetTunnel).toBe(true);
      expect(options.proxyRegion).toBe('JA');
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

  describe('Bybit Brazil site id', () => {
    it('forwards bybitSiteId for non-demo Bybit variants', () => {
      const base = {
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        bybitSiteId: 'BRA_BTL' as const,
      };

      expect(accountToExchangeOptions({ ...base, exchange: 'bybit' }).bybitSiteId).toBe('BRA_BTL');
      expect(accountToExchangeOptions({ ...base, exchange: 'bybitinverse' }).bybitSiteId).toBe('BRA_BTL');
      expect(accountToExchangeOptions({ ...base, exchange: 'bybitspot' }).bybitSiteId).toBe('BRA_BTL');
    });

    it('drops bybitSiteId for Bybit demo and other exchanges', () => {
      const base = {
        name: 'test-account',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        bybitSiteId: 'BRA_BTL' as const,
      };

      expect(accountToExchangeOptions({ ...base, exchange: 'bybitdemo' }).bybitSiteId).toBeUndefined();
      expect(accountToExchangeOptions({ ...base, exchange: 'okx', passphrase: 'pass' }).bybitSiteId).toBeUndefined();
    });
  });

  describe('exchange-specific fields', () => {
    it('should include applicationId when exchange requires it', () => {
      const account: ExchangeAccountConfig = {
        exchange: 'lighter',
        name: 'test-lighter',
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
        exchange: 'lighter',
        name: 'test-lighter',
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

    it('should include separate public KuCoin broker attribution env for futures and spot', () => {
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_PARTNER', ' futures-partner ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_KEY', ' futures-broker-key ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_NAME', ' futures-broker ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_FUTURES_BROKER_VERIFY', '0');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_SPOT_BROKER_PARTNER', ' spot-partner ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_SPOT_BROKER_KEY', ' spot-broker-key ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_SPOT_BROKER_NAME', ' spot-broker ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_SPOT_BROKER_VERIFY', '1');

      const futuresOptions = accountToExchangeOptions({
        exchange: 'kucoin',
        name: 'test-kucoin',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        passphrase: 'test-passphrase',
      });
      const spotOptions = accountToExchangeOptions({
        exchange: 'kucoinspot',
        name: 'test-kucoinspot',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        passphrase: 'test-passphrase',
      });

      expect(futuresOptions.extra?.kucoinBroker).toEqual({
        partner: 'futures-partner',
        brokerKey: 'futures-broker-key',
        brokerName: 'futures-broker',
        verify: false,
      });
      expect(spotOptions.extra?.kucoinBroker).toEqual({
        partner: 'spot-partner',
        brokerKey: 'spot-broker-key',
        brokerName: 'spot-broker',
        verify: true,
      });
    });

    it('should use generic public KuCoin broker attribution env as a fallback', () => {
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_PARTNER', ' 3zexp ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_KEY', ' browser-broker-key ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_NAME', ' tealstreet ');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_VERIFY', '0');

      const options = accountToExchangeOptions({
        exchange: 'kucoinspot',
        name: 'test-kucoinspot',
        apiKey: 'test-key',
        apiSecret: 'test-secret',
        passphrase: 'test-passphrase',
      });

      expect(options.extra?.kucoinBroker).toEqual({
        partner: '3zexp',
        brokerKey: 'browser-broker-key',
        brokerName: 'tealstreet',
        verify: false,
      });
    });

    it('should skip public KuCoin broker attribution env when incomplete or on other exchanges', () => {
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_PARTNER', '3zexp');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_KEY', 'browser-broker-key');

      expect(
        accountToExchangeOptions({
          exchange: 'kucoin',
          name: 'test-kucoin',
          apiKey: 'test-key',
          apiSecret: 'test-secret',
          passphrase: 'test-passphrase',
        }).extra?.kucoinBroker,
      ).toBeUndefined();

      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_NAME', 'tealstreet');

      expect(
        accountToExchangeOptions({
          exchange: 'bybit',
          name: 'test-bybit',
          apiKey: 'test-key',
          apiSecret: 'test-secret',
        }).extra?.kucoinBroker,
      ).toBeUndefined();
    });

    it('should let explicit KuCoin broker overrides win over public env defaults', () => {
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_PARTNER', '3zexp');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_KEY', 'browser-broker-key');
      vi.stubEnv('NEXT_PUBLIC_KUCOIN_BROKER_NAME', 'tealstreet');

      const options = accountToExchangeOptions(
        {
          exchange: 'kucoin',
          name: 'test-kucoin',
          apiKey: 'test-key',
          apiSecret: 'test-secret',
          passphrase: 'test-passphrase',
        },
        {
          extra: {
            kucoinBroker: {
              partner: 'override-partner',
              brokerKey: 'override-key',
              brokerName: 'override-name',
              verify: true,
            },
          },
        },
      );

      expect(options.extra?.kucoinBroker).toEqual({
        partner: 'override-partner',
        brokerKey: 'override-key',
        brokerName: 'override-name',
        verify: true,
      });
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
