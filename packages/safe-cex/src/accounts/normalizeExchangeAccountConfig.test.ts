import { describe, expect, it, vi } from 'vitest';

import { normalizeExchangeAccountConfig } from './normalizeExchangeAccountConfig';

vi.mock('../exchangeMetadata', () => ({
  getExchangeMetadata: vi.fn((exchange: string) => {
    const metadataMap = {
      bybit: {
        requiresCors: false,
        privateProxyRegions: { SG: ['1.2.3.4'], JA: ['5.6.7.8'] },
      },
      binance: {
        requiresCors: false,
        privateProxyRegions: {},
      },
      bitget: {
        requiresCors: true,
        privateProxyRegions: {},
      },
      hyperliquid: {
        requiresCors: true,
        privateProxyRegions: { JA: ['5.6.7.8'], SG: ['1.2.3.4'] },
      },
    } as const;
    return metadataMap[exchange as keyof typeof metadataMap] || metadataMap.bybit;
  }),
}));

describe('normalizeExchangeAccountConfig', () => {
  it('keeps direct routing when proxy is not requested', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'bybit',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
    });

    expect(account.tealstreetProxy).toBe(false);
    expect(account.proxyRegion).toBeUndefined();
  });

  it('drops stale proxyRegion when proxy is not requested', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'bybit',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
      proxyRegion: 'JA',
    });

    expect(account.tealstreetProxy).toBe(false);
    expect(account.proxyRegion).toBeUndefined();
  });

  it('defaults proxyRegion when proxy is requested', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'bybit',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
      tealstreetProxy: true,
    });

    expect(account.tealstreetProxy).toBe(true);
    expect(account.proxyRegion).toBe('SG');
  });

  it('falls back to SG when proxy is requested and metadata has no regions', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'binance',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
      tealstreetProxy: true,
    });

    expect(account.tealstreetProxy).toBe(true);
    expect(account.proxyRegion).toBe('SG');
  });

  it('preserves explicit proxyRegion', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'bybit',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
      tealstreetProxy: true,
      proxyRegion: 'JA',
    });

    expect(account.tealstreetProxy).toBe(true);
    expect(account.proxyRegion).toBe('JA');
  });

  it('forces proxy routing for exchanges that require CORS', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'hyperliquid',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
    });

    expect(account.tealstreetProxy).toBe(true);
    expect(account.proxyRegion).toBe('JA');
  });

  it('falls back to SG for requiresCors exchanges without region metadata', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'bitget',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
    });

    expect(account.tealstreetProxy).toBe(true);
    expect(account.proxyRegion).toBe('SG');
  });

  it('turns off manually requested proxy routing on testnet', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'bybit',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
      testnet: true,
      tealstreetProxy: true,
      proxyRegion: 'JA',
    });

    expect(account.tealstreetProxy).toBe(false);
    expect(account.proxyRegion).toBeUndefined();
  });

  it('lets requiresCors win over testnet', () => {
    const account = normalizeExchangeAccountConfig({
      exchange: 'hyperliquid',
      name: 'test-account',
      apiKey: 'key',
      apiSecret: 'secret',
      testnet: true,
      tealstreetProxy: false,
    });

    expect(account.tealstreetProxy).toBe(true);
    expect(account.proxyRegion).toBe('JA');
  });
});
