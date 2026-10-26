import { describe, expect, it } from 'vitest';

import { reverseWsSymbol } from './reference/bitrue/bitrue.utils';

describe('Bitrue symbol normalization', () => {
  it('normalizes websocket symbols with USDT/USDC/USD quotes', () => {
    expect(reverseWsSymbol('E_BTCUSDT')).toBe('E-BTC-USDT');
    expect(reverseWsSymbol('e_ethusdc')).toBe('E-ETH-USDC');
    expect(reverseWsSymbol('btcusd')).toBe('E-BTC-USD');
  });

  it('normalizes hyphen-prefixed symbols', () => {
    expect(reverseWsSymbol('E-BTCUSDT')).toBe('E-BTC-USDT');
    expect(reverseWsSymbol('E-ETHUSDC')).toBe('E-ETH-USDC');
  });

  it('normalizes bare E prefix (no separator)', () => {
    expect(reverseWsSymbol('EBTCUSDT')).toBe('E-BTC-USDT');
  });

  it('handles already-normalized symbols without double-parsing', () => {
    expect(reverseWsSymbol('E-BTC-USDT')).toBe('E-BTC-USDT');
    expect(reverseWsSymbol('E-ETH-USD')).toBe('E-ETH-USD');
  });

  it('falls back to prefixed symbol when quote cannot be detected', () => {
    expect(reverseWsSymbol('E_DOGEUSDE')).toBe('E-DOGEUSDE');
  });
});
