import { describe, expect, it } from 'vitest';

import { marketSymbolExists, resolveMarketSymbol } from './resolveMarketSymbol';

const market = (symbol: string, base: string, quote: string) => ({ symbol, base, quote }) as never;

const MARKETS = [
  market('BTCUSDT', 'BTC', 'USDT'),
  market('BTCUSD', 'BTC', 'USD'),
  market('BTCPERP', 'BTC', 'USDC'),
  market('ETHUSDT', 'ETH', 'USDT'),
  market('ETHUSD', 'ETH', 'USD'),
  market('ETHFIUSDT', 'ETHFI', 'USDT'),
];

describe('resolveMarketSymbol', () => {
  it('resolves a bare ticker to the USDT market', () => {
    expect(resolveMarketSymbol(MARKETS, 'btc')).toBe('BTCUSDT');
    expect(resolveMarketSymbol(MARKETS, 'ETH')).toBe('ETHUSDT');
  });

  it('keeps an exact symbol as the exchange spells it', () => {
    expect(resolveMarketSymbol(MARKETS, 'btcusdt')).toBe('BTCUSDT');
    expect(resolveMarketSymbol(MARKETS, 'ETHUSD')).toBe('ETHUSD');
  });

  // Suffix picks the settlement currency, so an inverse contract is not
  // silently rewritten to the linear one.
  it('routes a USD suffix to the inverse market, USDC to the USDC one', () => {
    expect(resolveMarketSymbol(MARKETS, 'BTCUSD')).toBe('BTCUSD');
    expect(resolveMarketSymbol(MARKETS, 'BTCUSDC')).toBe('BTCPERP');
  });

  // Longest base wins, or ETHFI resolves through the ETH market.
  it('prefers the longest matching base', () => {
    expect(resolveMarketSymbol(MARKETS, 'ETHFI')).toBe('ETHFIUSDT');
  });

  it('returns the input uppercased when nothing matches', () => {
    expect(resolveMarketSymbol(MARKETS, 'nosuchthing')).toBe('NOSUCHTHING');
    expect(resolveMarketSymbol(undefined, 'btc')).toBe('BTC');
    expect(resolveMarketSymbol(MARKETS, '  ')).toBe('');
  });
});

describe('marketSymbolExists', () => {
  it('is case-insensitive and rejects an unlisted symbol', () => {
    expect(marketSymbolExists(MARKETS, 'btcusdt')).toBe(true);
    expect(marketSymbolExists(MARKETS, 'BTC')).toBe(false);
    expect(marketSymbolExists(undefined, 'BTCUSDT')).toBe(false);
  });
});
