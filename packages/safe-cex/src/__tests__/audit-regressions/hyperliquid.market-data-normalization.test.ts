import { describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../../store/store.base';
import type { AssetCtx, Meta } from '../../exchanges/hyperliquid/hyperliquid.types';

const createExchange = (): HyperliquidExchange =>
  new HyperliquidExchange(
    'hyperliquid',
    {
      key: '0xkey',
      secret: '0xsecret',
      applicationId: '0xuser',
      tuning: { consoleLogging: false },
    },
    new DefaultStore(),
  );

const wldMeta: Meta = {
  universe: [{ name: 'WLD', szDecimals: 1, maxLeverage: 10, onlyIsolated: false }],
};

const wldAssetCtx: AssetCtx = {
  funding: '0.0000125',
  openInterest: '76651392.0',
  prevDayPx: '0.34085',
  dayNtlVlm: '4732907.5820689993',
  dayBaseVlm: '13876581.8999999966',
  premium: '-0.0002042484',
  oraclePx: '0.34272',
  markPx: '0.34254',
  midPx: '0.34255',
  impactPxs: ['0.34249', '0.34265'],
};

describe('Hyperliquid market-data normalization', () => {
  it('maps core perp public context fields to ticker values in UI units', () => {
    const exchange = createExchange();

    const { markets, tickers } = (exchange as any).fetchMarketsHelper(wldMeta, [wldAssetCtx], null);
    exchange.dispose();

    expect(markets[0].symbol).toBe('WLD-PERP');
    expect(tickers[0]).toMatchObject({
      symbol: 'WLD-PERP',
      volume: 13_876_581.8999999966,
      quoteVolume: 4_732_907.5820689993,
    });
    expect(tickers[0].openInterest).toBeCloseTo(76_651_392 * 0.34254, 6);
    expect(tickers[0].percentage).toBeCloseTo(((0.34254 - 0.34085) / 0.34085) * 100, 10);
  });

  it('maps HIP-3 perp public context fields the same way as core perps', async () => {
    const exchange = createExchange();
    (exchange as any).xhr = {
      post: vi.fn().mockResolvedValue({
        data: [
          { universe: [{ name: 'vWLD', szDecimals: 1, maxLeverage: 10, onlyIsolated: false }] },
          [wldAssetCtx],
        ],
      }),
    };

    const { tickers } = await (exchange as any).fetchHip3Markets('vntls', 1, null);
    exchange.dispose();

    expect(tickers[0]).toMatchObject({
      symbol: 'VNTLS|VWLD-PERP',
      volume: 13_876_581.8999999966,
      quoteVolume: 4_732_907.5820689993,
    });
    expect(tickers[0].openInterest).toBeCloseTo(76_651_392 * 0.34254, 6);
    expect(tickers[0].percentage).toBeCloseTo(((0.34254 - 0.34085) / 0.34085) * 100, 10);
  });
});
