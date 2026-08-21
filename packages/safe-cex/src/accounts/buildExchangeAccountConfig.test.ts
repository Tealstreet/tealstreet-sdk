import { describe, expect, it } from 'vitest';

import { accountToExchangeOptions } from '../createExchangeFromAccount';
import {
  buildExchangeAccountConfig,
  ExchangeAccountConfigError,
  getExchangeAccountCredentialSchema,
  getSupportedExchangeAccountNames,
} from './buildExchangeAccountConfig';

describe('buildExchangeAccountConfig', () => {
  it('builds and normalizes standard API key accounts', () => {
    const account = buildExchangeAccountConfig({
      exchange: 'bybit',
      name: ' bybit-main ',
      apiKey: ' key ',
      apiSecret: ' secret ',
      tealstreetProxy: true,
    });

    expect(account).toMatchObject({
      exchange: 'bybit',
      name: 'bybit-main',
      apiKey: 'key',
      apiSecret: 'secret',
      tealstreetProxy: true,
      proxyRegion: 'SG',
    });
  });

  it('requires the exchange-specific fields web renders for Paradex', () => {
    expect(() =>
      buildExchangeAccountConfig({
        exchange: 'paradex',
        name: 'pd',
        apiKey: '0xprivate',
        paradexAddress: '0xmain',
      }),
    ).toThrowError(ExchangeAccountConfigError);
  });

  it('builds Paradex accounts in the same saved shape consumed by createExchangeFromAccount', () => {
    const account = buildExchangeAccountConfig({
      exchange: 'paradex',
      name: 'pd',
      apiKey: '0xsubkey-private',
      paradexAddress: '0xmain-account',
      subkeyAddress: '0xsubkey-public',
      tealstreetProxy: true,
    });

    expect(account).toMatchObject({
      exchange: 'paradex',
      apiKey: '0xsubkey-private',
      apiSecret: '0xsubkey-private',
      paradexAddress: '0xmain-account',
      subkeyAddress: '0xsubkey-public',
    });
    expect(accountToExchangeOptions(account)).toMatchObject({
      key: '0xsubkey-private',
      secret: '0xsubkey-private',
      paradexAddress: '0xmain-account',
      subkeyAddress: '0xsubkey-public',
    });
  });

  it('accepts CLI-friendly Paradex aliases but persists canonical fields', () => {
    const account = buildExchangeAccountConfig({
      exchange: 'paradex',
      name: 'pd',
      privateKey: '0xsubkey-private',
      walletAddress: '0xmain-account',
      subkeyAddress: '0xsubkey-public',
    });

    expect(account).toMatchObject({
      apiKey: '0xsubkey-private',
      apiSecret: '0xsubkey-private',
      paradexAddress: '0xmain-account',
      subkeyAddress: '0xsubkey-public',
    });
    expect(account).not.toHaveProperty('privateKey');
    expect(account).not.toHaveProperty('walletAddress');
  });

  it('derives required credential fields from metadata', () => {
    expect(getExchangeAccountCredentialSchema('paradex')?.requiredFields).toEqual([
      'apiKey',
      'paradexAddress',
      'subkeyAddress',
    ]);
    expect(getExchangeAccountCredentialSchema('okx')?.requiredFields).toEqual(['apiKey', 'apiSecret', 'passphrase']);
    expect(getExchangeAccountCredentialSchema('lighter')?.requiredFields).toEqual([
      'apiKey',
      'apiSecret',
      'applicationId',
      'apiKeyIndex',
    ]);
  });

  it('lists Paradex as an addable exchange from safe-cex metadata', () => {
    expect(getSupportedExchangeAccountNames()).toContain('paradex');
  });

  it('preserves Bybit Brazil site id only for non-demo Bybit account variants', () => {
    expect(
      buildExchangeAccountConfig({
        exchange: 'bybit',
        name: 'bybit-main',
        apiKey: 'key',
        apiSecret: 'secret',
        bybitSiteId: 'BRA_BTL',
      }).bybitSiteId,
    ).toBe('BRA_BTL');
    expect(
      buildExchangeAccountConfig({
        exchange: 'bybitinverse',
        name: 'bybit-inverse',
        apiKey: 'key',
        apiSecret: 'secret',
        bybitSiteId: 'BRA_BTL',
      }).bybitSiteId,
    ).toBe('BRA_BTL');
    expect(
      buildExchangeAccountConfig({
        exchange: 'bybitspot',
        name: 'bybit-spot',
        apiKey: 'key',
        apiSecret: 'secret',
        bybitSiteId: 'BRA_BTL',
      }).bybitSiteId,
    ).toBe('BRA_BTL');
    expect(
      buildExchangeAccountConfig({
        exchange: 'bybitdemo',
        name: 'bybit-demo',
        apiKey: 'key',
        apiSecret: 'secret',
        bybitSiteId: 'BRA_BTL',
      }),
    ).not.toHaveProperty('bybitSiteId');
  });

  it('does not add a Bybit Brazil field when it is unset', () => {
    expect(
      buildExchangeAccountConfig({
        exchange: 'bybit',
        name: 'bybit-main',
        apiKey: 'key',
        apiSecret: 'secret',
      }),
    ).not.toHaveProperty('bybitSiteId');
  });
});
