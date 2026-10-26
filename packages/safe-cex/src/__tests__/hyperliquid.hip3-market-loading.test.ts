import { describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from './reference/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../store/store.base';

// Core perps come from a module helper, not a method; this test is about the HIP-3
// loop, so the core fetch is stubbed to a minimal valid shape.
vi.mock('./reference/hyperliquid/hyperliquid.market-startup-helpers', () => ({
  fetchCoreMetaAndAssetCtxsWithFallback: vi.fn(async () => [{ universe: [] }, []]),
  fetchHip3MetaAndAssetCtxsWithRetry: vi.fn(async () => null),
}));

/**
 * Regression: `hyperliquidRestRateLimiter` paces by weight (50ms/unit) and
 * `metaAndAssetCtxs` is weight 20, so each HIP-3 dex reserves ~1s of a serial
 * timeline. Ten live dexes needed ~10s against a flat 8s budget, seven finished,
 * and the all-or-nothing branch discarded all seven — so NO HIP-3 markets showed.
 */
const DEXES = ['xyz', 'flx', 'vntl', 'hyna', 'km', 'abcd', 'cash', 'para', 'mkts', 'io'];

const createExchange = () => {
  const store = new DefaultStore();
  const exchange = new HyperliquidExchange(
    'hyperliquid',
    { testnet: false, tuning: { consoleLogging: false } } as any,
    store,
  );
  const logged: string[] = [];
  vi.spyOn(exchange, 'log').mockImplementation((message: any) => {
    logged.push(String(message));
  });
  return { exchange, logged };
};

/**
 * Drives the real loop: stubs discovery and the per-dex fetch, and makes each dex
 * cost `perDexMs` so the budget arithmetic is what decides the outcome.
 */
const runMarketLoad = async ({
  dexNames,
  perDexMs,
  failing = [],
}: {
  dexNames: string[];
  perDexMs: number;
  failing?: string[];
}) => {
  const { exchange, logged } = createExchange();
  const anyExchange = exchange as any;

  vi.spyOn(anyExchange, 'fetchHip3Dexs').mockResolvedValue(
    dexNames.map((name, index) => ({ name, dexPosition: index + 1 })),
  );
  vi.spyOn(anyExchange, 'fetchHip3Markets').mockImplementation(async (...args: unknown[]) => {
    const dexName = args[0] as string;
    await new Promise((resolve) => setTimeout(resolve, perDexMs));
    if (failing.includes(dexName)) return null;
    return {
      markets: [{ id: `${dexName}:A`, symbol: `${dexName.toUpperCase()}|A-PERP` } as any],
      tickers: [{ symbol: `${dexName.toUpperCase()}|A-PERP` } as any],
    };
  });
  // Core perps, spot and fees are not under test.
  vi.spyOn(anyExchange, 'fetchFee').mockResolvedValue(null);
  vi.spyOn(anyExchange, 'fetchSpotMarketsAndTickers').mockResolvedValue({ markets: [], tickers: [] });
  vi.spyOn(anyExchange, 'fetchMarketsHelper').mockReturnValue({ markets: [], tickers: [] });

  const [markets] = await anyExchange.fetchMarketsAndTickers();
  return { markets, logged, hip3DexNames: exchange.hip3DexNames as string[] };
};

describe('Hyperliquid HIP-3 market loading', () => {
  it('keeps the dexes that loaded when the budget runs out mid-list', async () => {
    // 10 dexes at 900ms each needs 9s; the cap for this list is 20s, so to force a
    // partial we squeeze it: 2s per dex of real cost against the derived budget.
    const { markets, hip3DexNames, logged } = await runMarketLoad({
      dexNames: DEXES,
      perDexMs: 2_400,
    });

    // The old code returned zero here. Anything above zero is the fix.
    expect(markets.length).toBeGreaterThan(0);
    expect(hip3DexNames.length).toBe(markets.length);
    // And it says which ones it never reached, rather than going quiet.
    expect(logged.some((line) => line.includes('ran out of its'))).toBe(true);
  }, 40_000);

  it('sizes the budget from the dex count, so ten dexes fit where a flat 8s could not', async () => {
    // 900ms x 10 = 9s of work: over the old flat 8s, under the derived 20s. So this
    // case passes only because the budget scales with the number of dexes.
    const { markets, hip3DexNames } = await runMarketLoad({ dexNames: DEXES, perDexMs: 900 });

    expect(hip3DexNames).toEqual(DEXES);
    expect(markets.length).toBe(DEXES.length);
  }, 40_000);

  it('reports a dex that failed instead of counting it as loaded with no markets', async () => {
    const { markets, hip3DexNames, logged } = await runMarketLoad({
      dexNames: DEXES,
      perDexMs: 10,
      failing: ['km', 'abcd'],
    });

    expect(hip3DexNames).not.toContain('km');
    expect(hip3DexNames).not.toContain('abcd');
    // The other eight are unaffected — one bad dex is not the others' problem.
    expect(markets.length).toBe(DEXES.length - 2);
    expect(logged.some((line) => line.includes('failed to load') && line.includes('km'))).toBe(true);
  }, 40_000);
});
