import { describe, expect, it, vi } from 'vitest';

import { BitmexExchange } from '../exchanges/bitmex/bitmex.exchange';

const createExchangeLike = (tickersBySymbol: Record<string, any>) => {
  const store = {
    subscribe: vi.fn(),
    tickersBySymbol,
    marketsBySymbol: {},
  } as any;

  const exchange = new BitmexExchange(
    'bitmex' as any,
    {
      key: 'key',
      secret: 'secret',
      testnet: false,
    } as any,
    store,
  );

  (exchange as any).currencies = {
    XBt: { precision: 0.00000001, majorCurrency: 'XBT' },
    USDt: { precision: 0.000001, majorCurrency: 'USDT' },
    USD: { precision: 1, majorCurrency: 'USD' },
    MAMUSd: { precision: 0.000001, majorCurrency: 'MAMUSD' },
  };

  return exchange;
};

describe('BitmexExchange balance snapshot parsing', () => {
  it('ignores zeroed MAM rows and keeps aggregate single-asset balances', () => {
    const exchange = createExchangeLike({
      XBTUSDT: { last: 50000 },
    });

    const parsed = exchange.parseBalanceSnapshot([
      {
        currency: 'XBt',
        walletBalance: 100000,
        availableMargin: 100000,
        marginBalance: 100000,
        unrealisedPnl: 0,
      },
      {
        currency: 'MAMUSD',
        walletBalance: 0,
        availableMargin: 0,
        marginBalance: 0,
        unrealisedPnl: 0,
      },
    ] as Array<Record<string, any>>);

    expect(parsed.total).toBeCloseTo(50, 8);
    expect(parsed.free).toBeCloseTo(50, 8);
    expect(parsed.upnl).toBe(0);
    expect(parsed.used).toBe(0);
  });

  it('uses MAM row when it carries active collateral', () => {
    const exchange = createExchangeLike({
      XBTUSDT: { last: 50000 },
    });

    const parsed = exchange.parseBalanceSnapshot([
      {
        currency: 'XBt',
        walletBalance: 100000,
        availableMargin: 100000,
        marginBalance: 100000,
        unrealisedPnl: 0,
      },
      {
        currency: 'MAMUSD',
        walletBalance: 2500000,
        availableMargin: 2000000,
        marginBalance: 2100000,
        initMargin: 100000,
        unrealisedPnl: -100000,
      },
    ] as Array<Record<string, any>>);

    expect(parsed.total).toBeCloseTo(2.5, 8);
    expect(parsed.free).toBeCloseTo(2, 8);
    expect(parsed.used).toBeCloseTo(0.1, 8);
    expect(parsed.upnl).toBeCloseTo(-0.1, 8);
  });

  it('falls back to XBTUSD ticker when XBTUSDT is missing', () => {
    const exchange = createExchangeLike({
      XBTUSD: { last: 60000 },
    });

    const parsed = exchange.parseBalanceSnapshot([
      {
        currency: 'XBt',
        walletBalance: 100000,
        availableMargin: 100000,
        marginBalance: 100000,
        unrealisedPnl: 0,
      },
    ] as Array<Record<string, any>>);

    expect(parsed.total).toBeCloseTo(60, 8);
    expect(parsed.free).toBeCloseTo(60, 8);
  });
});
