import { describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../../store/store.base';

const deferred = () => {
  let resolve!: (value: { markets: never[]; tickers: never[] }) => void;
  const promise = new Promise<{ markets: never[]; tickers: never[] }>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
};

describe('Hyperliquid HIP-3 startup budget', () => {
  it('never runs more than one per-dex metadata request concurrently', async () => {
    const exchange = new HyperliquidExchange(
      'hyperliquid',
      {
        key: '0xkey',
        secret: '0xsecret',
        applicationId: '0xuser',
        testnet: false,
        tuning: { consoleLogging: false },
      },
      new DefaultStore(),
    );

    const requests = [deferred(), deferred(), deferred()];
    let active = 0;
    let maxActive = 0;
    const started: string[] = [];

    vi.spyOn(exchange as any, 'fetchFee').mockResolvedValue(null);
    vi.spyOn(exchange as any, 'fetchHip3Dexs').mockResolvedValue([
      { name: 'one', dexPosition: 1 },
      { name: 'two', dexPosition: 2 },
      { name: 'three', dexPosition: 3 },
    ]);
    vi.spyOn(exchange as any, 'fetchMarketsHelper').mockReturnValue({ markets: [], tickers: [] });
    vi.spyOn(exchange as any, 'fetchHip3Markets').mockImplementation((...args: unknown[]) => {
      const dexName = String(args[0]);
      const index = started.length;
      started.push(dexName);
      active++;
      maxActive = Math.max(maxActive, active);
      return requests[index].promise.finally(() => {
        active--;
      });
    });
    (exchange as any).infoXhr.post = vi.fn().mockResolvedValue({ data: [{ universe: [] }, []] });

    const startup = (exchange as any).fetchMarketsAndTickers();
    await vi.waitFor(() => expect(started).toEqual(['one']));
    requests[0].resolve({ markets: [], tickers: [] });
    await vi.waitFor(() => expect(started).toEqual(['one', 'two']));
    requests[1].resolve({ markets: [], tickers: [] });
    await vi.waitFor(() => expect(started).toEqual(['one', 'two', 'three']));
    requests[2].resolve({ markets: [], tickers: [] });

    await expect(startup).resolves.toEqual([[], []]);
    expect(maxActive).toBe(1);
    expect(exchange.hip3DexNames).toEqual(['one', 'two', 'three']);
  });
});
