import { describe, expect, it } from 'vitest';

import { HyperliquidSpotExchange } from './reference/hyperliquid/hyperliquidspot.exchange';
import { DefaultStore } from '../store/store.base';
import { ExchangeOptions } from '../types';

const createExchange = (): HyperliquidSpotExchange => {
  const options: ExchangeOptions = {
    key: '0xkey',
    secret: '0xsecret',
    applicationId: '0xuser',
    testnet: true,
    tuning: { consoleLogging: false },
  };

  return new HyperliquidSpotExchange('hyperliquidspot', options, new DefaultStore());
};

describe('HyperliquidSpotExchange balance parsing', () => {
  it('values non-USDC spot holdings when ticker ids are spot pair names', () => {
    const exchange = createExchange();

    exchange.store.update({
      markets: [
        {
          id: 'PURR/USDC',
          symbol: 'PURR/USDC',
          base: 'PURR',
          quote: 'USDC',
          isSpot: true,
          active: true,
          precision: { amount: 1, price: 0.00001 },
          limits: { amount: { min: 1, max: Number.MAX_SAFE_INTEGER }, leverage: { min: 1, max: 1 } },
          info: { hyperliquidId: 10000, tokens: [1, 0] },
        },
      ] as any,
      tickers: [
        {
          id: 'PURR/USDC',
          symbol: 'PURR/USDC',
          bid: 0,
          ask: 0,
          last: 0.2,
          mark: 0.2,
          index: 0.2,
          percentage: 0,
          fundingRate: 0,
          fundingTime: 0,
          volume: 0,
          quoteVolume: 0,
          openInterest: 0,
          info: {},
        },
      ] as any,
    });

    exchange.applySpotBalances([
      { coin: 'USDC', total: '10', hold: '1', token: 0 },
      { coin: 'PURR', total: '2000', hold: '100', token: 1 },
    ]);

    expect(exchange.store.balance.currencies?.PURR).toEqual({
      total: 2000,
      used: 100,
      free: 1900,
      upnl: 0,
    });
    expect(exchange.store.balance.coins?.PURR).toEqual({
      base: 'PURR',
      amount: 2000,
      notional: 400,
    });
    expect(exchange.store.balance.total).toBe(410);
    expect(exchange.store.balance.used).toBe(21);
    expect(exchange.store.balance.free).toBe(389);
  });

  it('treats empty spot balance snapshots as authoritative zero balance', () => {
    const exchange = createExchange();

    exchange.applySpotBalances([{ coin: 'USDC', total: '10', hold: '0', token: 0 }]);
    expect(exchange.store.balance.total).toBe(10);

    exchange.applySpotBalances([]);

    expect(exchange.store.balance).toEqual({
      total: 0,
      used: 0,
      free: 0,
      upnl: 0,
      currencies: {},
      coins: {},
    });
  });
});
