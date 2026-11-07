import { describe, expect, it } from 'vitest';

import {
  extractLighterTickerMarketIdFromChannel,
  parseLighterWsBookSidePrice,
  parseLighterWsNumber,
  shouldDispatchLighterPublicMessage,
} from './reference/lighter/lighter.ws-public-parsers';

describe('lighter.ws-public-parsers helpers', () => {
  it('parses numeric values from mixed candidates', () => {
    expect(parseLighterWsNumber(undefined, null, '', 'abc', '123.45')).toBe(123.45);
    expect(parseLighterWsNumber(undefined, 42)).toBe(42);
    expect(parseLighterWsNumber(undefined, null, '')).toBeUndefined();
  });

  it('parses book side price from tuple and object forms', () => {
    expect(parseLighterWsBookSidePrice(['101.5', '2'])).toBe(101.5);
    expect(parseLighterWsBookSidePrice({ price: '99.2' })).toBe(99.2);
    expect(parseLighterWsBookSidePrice({ p: '77.7' })).toBe(77.7);
    expect(parseLighterWsBookSidePrice(undefined)).toBeUndefined();
  });

  it('detects topic dispatch conditions for normal, ticker-per-market, and pong', () => {
    expect(shouldDispatchLighterPublicMessage('market_stats:all', '{"channel":"market_stats:all"}')).toBe(true);
    expect(shouldDispatchLighterPublicMessage('ticker', '{"channel":"ticker:1"}')).toBe(true);
    expect(shouldDispatchLighterPublicMessage('pong', '{"type":"pong"}')).toBe(true);
    expect(shouldDispatchLighterPublicMessage('tickers', '{"channel":"market_stats:all"}')).toBe(false);
  });

  it('extracts ticker market id from channel variants', () => {
    expect(extractLighterTickerMarketIdFromChannel('ticker:1')).toBe('1');
    expect(extractLighterTickerMarketIdFromChannel('ticker/42')).toBe('42');
    expect(extractLighterTickerMarketIdFromChannel('market_stats:all')).toBeUndefined();
  });
});
