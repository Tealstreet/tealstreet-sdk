import { describe, expect, it } from 'vitest';

import { getLighterApiBaseUrl, resolveLighterDeploymentProfile } from '../exchanges/lighter/lighter.types';

describe('resolveLighterDeploymentProfile', () => {
  it('keeps existing zkLighter mainnet values by default', () => {
    expect(resolveLighterDeploymentProfile({ exchangeName: 'lighter', testnet: false })).toMatchObject({
      apiBaseUrl: 'https://mainnet.zklighter.elliot.ai',
      network: 'livenet',
      quoteCurrency: 'USDC',
      signerChainId: 304,
      usesRestProxy: true,
      supportsWsProxy: true,
      wsUrl: 'wss://mainnet.zklighter.elliot.ai/stream',
    });
    expect(getLighterApiBaseUrl({ exchangeName: 'lighter', testnet: false })).toContain(
      'https://mainnet.zklighter.elliot.ai',
    );
  });

  it('resolves Robinhood Chain Lighter values for the exchange variant', () => {
    expect(resolveLighterDeploymentProfile({ exchangeName: 'lighter-robinhood', testnet: false })).toMatchObject({
      apiBaseUrl: 'https://api.rh.lighter.xyz',
      network: 'robinhood',
      quoteCurrency: 'USDG',
      signerChainId: 466324,
      usesRestProxy: false,
      supportsWsProxy: false,
      wsUrl: 'wss://api.rh.lighter.xyz/stream',
    });
    expect(getLighterApiBaseUrl({ exchangeName: 'lighter-robinhood', testnet: false })).toBe(
      'https://api.rh.lighter.xyz',
    );
  });
});
