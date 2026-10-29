import { describe, expect, it } from 'vitest';

import { privateProxyIpsVultrTokyo } from '@tealstreet/proxy-ips-lists/src';
import {
  exchangeMetadata,
  exchangePrettyNames,
  getExchangeMetadata,
  getUniqueLatencyDomains,
  isExchangeSupportedOnMobile,
  latencyDomainToExchanges,
} from './exchangeMetadata';
import { ExchangeName, MarginType, PositionMode } from './types';

describe('exchangeMetadata', () => {
  describe('exchangeMetadata constant', () => {
    it('should have metadata for common exchanges', () => {
      expect(exchangeMetadata.binance).toBeDefined();
      expect(exchangeMetadata.bybit).toBeDefined();
      expect(exchangeMetadata.okx).toBeDefined();
    });

    it('should have required fields for each exchange', () => {
      const exchanges = Object.keys(exchangeMetadata) as ExchangeName[];

      exchanges.forEach((exchangeName) => {
        const metadata = exchangeMetadata[exchangeName];
        expect(metadata.exchangeName).toBe(exchangeName);
        expect(typeof metadata.prettyName).toBe('string');
        expect(Array.isArray(metadata.positionModes)).toBe(true);
        expect(Array.isArray(metadata.marginTypes)).toBe(true);
        expect(typeof metadata.doesNotSupportOpenInterest).toBe('boolean');
      });
    });

    it('should have valid position modes', () => {
      const exchanges = Object.keys(exchangeMetadata) as ExchangeName[];

      exchanges.forEach((exchangeName) => {
        const metadata = exchangeMetadata[exchangeName];
        metadata.positionModes.forEach((mode) => {
          expect([PositionMode.OneWay, PositionMode.Hedged]).toContain(mode);
        });
      });
    });

    it('should have valid margin types', () => {
      const exchanges = Object.keys(exchangeMetadata) as ExchangeName[];

      exchanges.forEach((exchangeName) => {
        const metadata = exchangeMetadata[exchangeName];
        metadata.marginTypes.forEach((type) => {
          expect([MarginType.Cross, MarginType.Isolated]).toContain(type);
        });
      });
    });

    it('should have features object for each exchange', () => {
      const exchanges = Object.keys(exchangeMetadata) as ExchangeName[];

      exchanges.forEach((exchangeName) => {
        const metadata = exchangeMetadata[exchangeName];
        expect(metadata.features).toBeDefined();
        expect(typeof metadata.features.kline).toBe('boolean');
        expect(typeof metadata.features.orderbook).toBe('boolean');
        expect(typeof metadata.features.orders).toBe('boolean');
        expect(typeof metadata.features.positions).toBe('boolean');
        expect(typeof metadata.features.balance).toBe('boolean');
      });
    });
  });

  describe('getExchangeMetadata', () => {
    it('should return metadata for a valid exchange', () => {
      const metadata = getExchangeMetadata('binance');
      expect(metadata).toBeDefined();
      expect(metadata.exchangeName).toBe('binance');
    });

    it('should reject an unknown exchange instead of applying Bybit metadata', () => {
      expect(() => getExchangeMetadata('unknown-exchange' as ExchangeName)).toThrow(
        'Unknown exchange metadata: unknown-exchange',
      );
    });

    it('should return unified metadata for bybit when isUnified is true', () => {
      const metadata = getExchangeMetadata('bybit', { isUnified: true });
      expect(metadata).toBeDefined();
      expect(metadata.exchangeName).toBe('bybit');
    });

    it('should return regular metadata for bybit when isUnified is false', () => {
      const metadata = getExchangeMetadata('bybit', { isUnified: false });
      expect(metadata).toBeDefined();
      expect(metadata.exchangeName).toBe('bybit');
    });

    it('should ignore options for non-bybit exchanges', () => {
      const metadata = getExchangeMetadata('binance', { isUnified: true });
      expect(metadata.exchangeName).toBe('binance');
    });
  });

  describe('exchangePrettyNames', () => {
    it('should be an object with exchange names as keys', () => {
      expect(typeof exchangePrettyNames).toBe('object');
    });

    it('should have pretty names for common exchanges', () => {
      expect(exchangePrettyNames.binance).toBeDefined();
      expect(exchangePrettyNames.bybit).toBeDefined();
      expect(exchangePrettyNames.okx).toBeDefined();
    });

    it('should have string values', () => {
      const keys = Object.keys(exchangePrettyNames) as ExchangeName[];
      keys.forEach((key) => {
        expect(typeof exchangePrettyNames[key]).toBe('string');
        expect(exchangePrettyNames[key].length).toBeGreaterThan(0);
      });
    });

    it('should have pretty name for Binance', () => {
      expect(exchangePrettyNames.binance).toBe('Binance');
    });

    it('should have pretty name for Bybit', () => {
      expect(exchangePrettyNames.bybit).toBe('Bybit');
    });

    it('advertises opening trigger orders for implemented derivatives variants only', () => {
      expect(exchangeMetadata.bybit.supportsOpeningTriggerOrders).toBe(true);
      expect(exchangeMetadata.bybitdemo.supportsOpeningTriggerOrders).toBe(true);
      expect(exchangeMetadata.bybitinverse.supportsOpeningTriggerOrders).toBe(true);
      expect(exchangeMetadata.bybitspot.supportsOpeningTriggerOrders).toBe(false);
      expect(exchangeMetadata.okx.supportsOpeningTriggerOrders).toBe(true);
      expect(exchangeMetadata.okxdemo.supportsOpeningTriggerOrders).toBe(true);
      expect(exchangeMetadata.okxeu.supportsOpeningTriggerOrders).toBe(true);
    });
  });

  describe('latencyDomainToExchanges', () => {
    it('should be a Map', () => {
      expect(latencyDomainToExchanges instanceof Map).toBe(true);
    });

    it('should have exchange arrays as values', () => {
      for (const [domain, exchanges] of latencyDomainToExchanges) {
        expect(typeof domain).toBe('string');
        expect(Array.isArray(exchanges)).toBe(true);
        expect(exchanges.length).toBeGreaterThan(0);
      }
    });

    it('should only contain valid exchange names', () => {
      const validExchangeNames = Object.keys(exchangeMetadata);

      for (const [, exchanges] of latencyDomainToExchanges) {
        exchanges.forEach((exchangeName) => {
          expect(validExchangeNames).toContain(exchangeName);
        });
      }
    });
  });

  describe('getUniqueLatencyDomains', () => {
    it('should return an array', () => {
      const domains = getUniqueLatencyDomains();
      expect(Array.isArray(domains)).toBe(true);
    });

    it('should return unique domains', () => {
      const domains = getUniqueLatencyDomains();
      const uniqueDomains = [...new Set(domains)];
      expect(domains.length).toBe(uniqueDomains.length);
    });

    it('should return the same domains as latencyDomainToExchanges keys', () => {
      const domains = getUniqueLatencyDomains();
      const mapKeys = Array.from(latencyDomainToExchanges.keys());
      expect(domains.sort()).toEqual(mapKeys.sort());
    });

    it('should return strings', () => {
      const domains = getUniqueLatencyDomains();
      domains.forEach((domain) => {
        expect(typeof domain).toBe('string');
      });
    });
  });

  describe('specific exchange metadata', () => {
    it('should have bybit metadata with correct values', () => {
      const bybit = exchangeMetadata.bybit;
      expect(bybit.exchangeName).toBe('bybit');
      expect(bybit.supportsTestnet).toBe(false);
      expect(bybit.enabled).toBe(true);
    });

    it('should have binance metadata with correct values', () => {
      const binance = exchangeMetadata.binance;
      expect(binance.exchangeName).toBe('binance');
      expect(binance.defaultQuote).toBe('USDT');
      expect(binance.doesNotSupportOpenInterest).toBe(true);
    });

    it('should have hyperliquid metadata', () => {
      const hl = exchangeMetadata.hyperliquid;
      expect(hl.exchangeName).toBe('hyperliquid');
      expect(hl.prettyName).toBe('Hyperliquid');
    });

    it('should require proxy routing for OKX EU without changing global OKX', () => {
      expect(exchangeMetadata.okx.requiresCors).toBe(false);
      expect(exchangeMetadata.okxeu.requiresCors).toBe(true);
      expect(exchangeMetadata.okxeu.privateProxyIps.length).toBeGreaterThan(0);
    });

    it('should keep lighter availability flags enabled', () => {
      const lighter = exchangeMetadata.lighter;

      expect(lighter.exchangeName).toBe('lighter');
      expect(lighter.enabled).toBe(true);
      expect(lighter.enabledInDev).toBe(true);
      expect(lighter.enableInPreview).toBe(true);
      expect(lighter.requiresApplicationId).toBe(true);
      expect(lighter.requiresApiKeyIndex).toBe(true);
    });

    it('should keep Coinbase Spot enabled in every web environment', () => {
      const coinbaseSpot = exchangeMetadata.coinbasespot;

      expect(coinbaseSpot.exchangeName).toBe('coinbasespot');
      expect(coinbaseSpot.enabled).toBe(true);
      expect(coinbaseSpot.enabledInDev).toBe(true);
      expect(coinbaseSpot.enableInPreview).toBe(true);
    });

    it('should keep HyroTrader dev-only until the private relay or HTTPS path is ready', () => {
      // Plain HTTP simulator private REST/WS works for local dev, but preview
      // and prod need Hyro HTTPS/WSS private endpoints or a Tealstreet relay.
      const hyrotrader = exchangeMetadata.hyrotrader;

      expect(hyrotrader.exchangeName).toBe('hyrotrader');
      expect(hyrotrader.enabled).toBe(false);
      expect(hyrotrader.enabledInDev).toBe(true);
      expect(hyrotrader.enableInPreview).toBe(false);
    });

    it('should keep Robinhood Lighter enabled for preview without production rollout', () => {
      const lighterRobinhood = exchangeMetadata['lighter-robinhood'];

      expect(lighterRobinhood.exchangeName).toBe('lighter-robinhood');
      expect(lighterRobinhood.enabled).toBe(false);
      expect(lighterRobinhood.enabledInDev).toBe(true);
      expect(lighterRobinhood.enableInPreview).toBe(true);
      expect(lighterRobinhood.defaultQuote).toBe('USDG');
      expect(lighterRobinhood.supportsWsRoute).toBe(true);
    });
  });

  describe('configuration invariants', () => {
    const exchanges = Object.keys(exchangeMetadata) as ExchangeName[];

    it('exchanges with requiresCors should have non-empty privateProxyIps', () => {
      // Some exchanges use CORS proxy without IP whitelisting (e.g., Polymarket)
      const exemptFromProxyIps = new Set(['polymarket']);
      exchanges.forEach((name) => {
        const m = exchangeMetadata[name];
        if (m.requiresCors && !exemptFromProxyIps.has(name)) {
          expect(
            m.privateProxyIps.length,
            `${name}: requiresCors is true but privateProxyIps is empty`,
          ).toBeGreaterThan(0);
        }
      });
    });

    // Since AWS_EXIT.md step 3e every private path egresses from the Vultr Tokyo fleet,
    // so the form must never again suggest an AWS pool to whitelist on master.
    it('every whitelist the account form can show is the Vultr Tokyo fleet', () => {
      exchanges.forEach((name) => {
        const m = exchangeMetadata[name];
        const lists = [m.privateProxyIps, ...Object.values(m.privateProxyRegions)].filter((ips) => ips.length);
        lists.forEach((ips) => expect(ips, name).toEqual(privateProxyIpsVultrTokyo));
      });
    });

    it('exchanges with requiresCors and no tunnel support should not silently get tunnel forced on', () => {
      // If an exchange has requiresCors (which enables tealstreetProxy), and does not
      // support tunnel (supportsTunnel: false, no enforceWsTunnel), verify that
      // createExchangeFromAccount won't accidentally force tealstreetTunnel on.
      // enforceWsTunnel: true is an explicit override that makes supportsTunnel: false ok.
      exchanges.forEach((name) => {
        const m = exchangeMetadata[name];
        if (m.requiresCors && m.supportsTunnel === false && !m.enforceWsTunnel) {
          // This config is valid (e.g., Crypto.com) — just documenting that these
          // exchanges use CORS proxy for REST but not the WS tunnel adapter.
          expect(m.forbidCors, `${name}: requiresCors with supportsTunnel false is valid`).not.toBe(true);
        }
      });
    });

    it('enabled exchanges should not have enabledInDev: false', () => {
      exchanges.forEach((name) => {
        const m = exchangeMetadata[name];
        if (m.enabled) {
          expect(m.enabledInDev, `${name}: enabled in production but disabled in dev — likely a mistake`).toBe(true);
        }
      });
    });

    it('exchangeName field should match the registry key', () => {
      exchanges.forEach((name) => {
        const m = exchangeMetadata[name];
        expect(
          m.exchangeName,
          `${name}: exchangeName field "${m.exchangeName}" does not match registry key "${name}"`,
        ).toBe(name);
      });
    });

    it('exchanges with requiresCors should not have forbidCors', () => {
      exchanges.forEach((name) => {
        const m = exchangeMetadata[name];
        if (m.requiresCors) {
          expect(m.forbidCors, `${name}: requiresCors and forbidCors are both true — contradictory`).not.toBe(true);
        }
      });
    });

    it('every exchange should have a non-empty prettyName', () => {
      exchanges.forEach((name) => {
        const m = exchangeMetadata[name];
        expect(m.prettyName.length, `${name}: prettyName is empty`).toBeGreaterThan(0);
      });
    });

    it('supportsPublicOnly should be a boolean for every exchange', () => {
      exchanges.forEach((name) => {
        const m = exchangeMetadata[name];
        expect(typeof m.supportsPublicOnly, `${name}: supportsPublicOnly is not a boolean`).toBe('boolean');
      });
    });

    it('known public-only exchanges should have supportsPublicOnly: true', () => {
      const expectedTrue: ExchangeName[] = [
        'apexomni',
        'asterdex',
        'binance',
        'binanceinverse',
        'binancespot',
        'binancepm',
        'bingx',
        'bitget',
        'bitrue',
        'bitunix',
        'blofin',
        'breakoutprop',
        'bybit',
        'bybitdemo',
        'hyrotrader',
        'bybitspot',
        'bybitinverse',
        'coinbase',
        'cryptocom',
        'grvt',
        'hyperliquid',
        'hyperliquidspot',
        'kraken',
        'kucoin',
        'kucoinspot',
        'lighter',
        'lighter-robinhood',
        'mexc',
        'okx',
        'okxdemo',
        'paradex',
        'phemex',
        'polymarket',
        'weex',
        'woo',
      ];
      expectedTrue.forEach((name) => {
        expect(exchangeMetadata[name].supportsPublicOnly, `${name}: expected supportsPublicOnly to be true`).toBe(true);
      });
    });

    it('supports exactly the mobile-enabled exchange allow-list', () => {
      const mobileSupported = exchanges.filter((name) => exchangeMetadata[name].supportedOnMobile).sort();

      expect(mobileSupported).toEqual([
        'binance',
        'binanceinverse',
        'binancepm',
        'binancespot',
        'bitget',
        'blofin',
        'bybit',
        'bybitdemo',
        'bybitinverse',
        'bybitspot',
        'hyperliquid',
        'kraken',
        'lighter',
        'okx',
      ]);

      mobileSupported.forEach((name) => {
        expect(isExchangeSupportedOnMobile(name), `${name}: helper should match metadata`).toBe(true);
      });

      [
        'bitgetinverse',
        'bitgetusdc',
        'hyrotrader',
        'hyperliquidspot',
        'krakenspot',
        'lighter-robinhood',
        'okxdemo',
        'okxeu',
        'okxeudemo',
      ].forEach((name) => {
        expect(isExchangeSupportedOnMobile(name), `${name}: variant should not be mobile-enabled`).toBe(false);
      });

      expect(isExchangeSupportedOnMobile('unknown-exchange')).toBe(false);
    });

    it('exchanges not verified for public-only should have supportsPublicOnly: false', () => {
      const expectedFalse: ExchangeName[] = ['backpack', 'bitgetinverse', 'bitgetusdc', 'coinbasespot', 'txflow'];
      expectedFalse.forEach((name) => {
        expect(exchangeMetadata[name].supportsPublicOnly, `${name}: expected supportsPublicOnly to be false`).toBe(
          false,
        );
      });
    });

    it('keeps Binance inverse production-enabled after the COIN-M rollout', () => {
      expect(exchangeMetadata.binanceinverse.enabled).toBe(true);
      expect(exchangeMetadata.binanceinverse.enabledInDev).toBe(true);
      expect(exchangeMetadata.binanceinverse.enableInPreview).toBe(true);
      expect(exchangeMetadata.binanceinverse.requiresCors).toBe(true);
      expect(exchangeMetadata.binanceinverse.privateProxyRegions).toHaveProperty('JA');
      expect(exchangeMetadata.binanceinverse.supportsTestnet).toBe(false);
      expect(exchangeMetadata.binanceinverse.requiresTickersForMarketsLoaded).toBe(true);
      expect(exchangeMetadata.binanceinverse.latencyDomain).toBe('dapi.binance.com');
    });
  });
});
