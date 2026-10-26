import { describe, expect, it, vi } from 'vitest';

import { DefaultStore } from '../../store/store.base';
import {
  parseGammaMarketTokens,
  PolymarketExchange,
  precisionFromTickSize,
} from '../reference/polymarket/polymarket.exchange';
import type { GammaMarketInfo } from '../reference/polymarket/polymarket.types';

const gammaMarket = {
  id: '123',
  question: 'Will the V2 adapter work?',
  conditionId: `0x${'ab'.repeat(32)}`,
  slug: 'v2-adapter',
  resolutionSource: '',
  endDate: '2026-12-31T00:00:00Z',
  liquidity: '1000',
  startDate: '2026-01-01T00:00:00Z',
  fee: '0',
  description: 'fixture from the documented Gamma shape',
  outcomes: '["Yes","No"]',
  outcomePrices: '["0.4","0.6"]',
  volume: '10',
  active: true,
  closed: false,
  marketType: 'normal',
  clobTokenIds: '["111","222"]',
  orderPriceMinTickSize: 0.01,
  orderMinSize: 5,
  negRisk: true,
  enableOrderBook: true,
  acceptingOrders: true,
} satisfies GammaMarketInfo;

describe('Polymarket Gamma and Data API contracts', () => {
  it('zips JSON-encoded token IDs with outcomes and rejects malformed pairs', () => {
    expect(parseGammaMarketTokens(gammaMarket)).toEqual([
      { tokenId: '111', outcome: 'Yes' },
      { tokenId: '222', outcome: 'No' },
    ]);
    expect(parseGammaMarketTokens({ ...gammaMarket, outcomes: '["Yes"]' })).toEqual([]);
    expect(parseGammaMarketTokens({ ...gammaMarket, clobTokenIds: 'not-json' })).toEqual([]);
  });

  it('derives precision from the market tick instead of hardcoding four decimals', () => {
    expect(precisionFromTickSize(0.01)).toBe(2);
    expect(precisionFromTickSize('0.001')).toBe(3);
    expect(precisionFromTickSize('0.0025')).toBe(4);
  });

  it('builds pUSD markets with Gamma min-size, tick-size, and neg-risk metadata', async () => {
    const exchange = new PolymarketExchange(
      'polymarket',
      { publicDataOnly: true } as any,
      new DefaultStore(),
    );
    (exchange as any).api = { getGammaMarkets: vi.fn().mockResolvedValue([gammaMarket]) };

    const markets = await exchange.fetchMarkets();

    expect(markets).toHaveLength(2);
    expect(markets[0]).toMatchObject({
      id: '111',
      symbol: 'v2-adapter-Yes',
      quote: 'pUSD',
      precision: { amount: 2, price: 2 },
      limits: { amount: { min: 5 } },
      info: { tickSize: 0.01, negRisk: true },
    });
    exchange.dispose();
  });

  it('scopes discovery only when explicit marketSlugs are supplied and keeps every outcome', async () => {
    for (const marketSlugs of [undefined, ['v2-adapter', 'another-market']]) {
      const exchange = new PolymarketExchange('polymarket', { key: '', secret: '', publicDataOnly: true, polymarket: { marketSlugs } }, new DefaultStore());
      const getGammaMarkets = vi.fn().mockResolvedValue([gammaMarket]);
      (exchange as any).api = { getGammaMarkets };
      await expect(exchange.fetchMarkets()).resolves.toHaveLength(2);
      expect(getGammaMarkets).toHaveBeenCalledWith({ active: true, closed: false, ...(marketSlugs === undefined ? {} : { slug: marketSlugs }) });
      exchange.dispose();
    }
  });

  it('rejects invalid explicit marketSlugs before market discovery', async () => {
    for (const marketSlugs of [[], [''], [' '], ['same', 'same'], [1], 'cafe', Array.from({ length: 101 }, (_, i) => String(i))]) {
      const exchange = new PolymarketExchange('polymarket', { key: '', secret: '', publicDataOnly: true, polymarket: { marketSlugs: marketSlugs as string[] } }, new DefaultStore());
      const getGammaMarkets = vi.fn();
      (exchange as any).api = { getGammaMarkets };
      await expect(exchange.fetchMarkets()).rejects.toThrow(/marketSlugs/);
      expect(getGammaMarkets).not.toHaveBeenCalled();
      exchange.dispose();
    }
  });

  it('maps current Data API holdings instead of replaying trade history', async () => {
    const store = new DefaultStore();
    const exchange = new PolymarketExchange(
      'polymarket',
      {
        key: `0x${'11'.repeat(32)}`,
        publicDataOnly: true,
      } as any,
      store,
    );
    const getPositions = vi.fn().mockResolvedValue([
      {
        proxyWallet: '0x1111111111111111111111111111111111111111',
        asset: '111',
        conditionId: gammaMarket.conditionId,
        size: 12.5,
        avgPrice: 0.4,
        currentValue: 7.5,
        cashPnl: 2.5,
        curPrice: 0.6,
        redeemable: false,
        mergeable: false,
        title: gammaMarket.question,
        slug: gammaMarket.slug,
        outcome: 'Yes',
        negativeRisk: true,
      },
    ]);
    (exchange as any).api = {
      getGammaMarkets: vi.fn().mockResolvedValue([gammaMarket]),
      getPositions,
    };
    await exchange.fetchMarkets();

    const positions = await exchange.fetchPositions();

    expect(getPositions).toHaveBeenCalledWith(expect.stringMatching(/^0x[0-9a-f]{40}$/i));
    expect(positions).toEqual([
      expect.objectContaining({
        symbol: 'v2-adapter-Yes',
        entryPrice: 0.4,
        contracts: 12.5,
        notional: 7.5,
        unrealizedPnl: 2.5,
      }),
    ]);
    exchange.dispose();
  });
});
