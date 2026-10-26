import type { PublicDataSource } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { BinanceExchange } from '../reference/binance/binance.exchange';
import { BinancePMExchange } from '../reference/binance/binancepm.exchange';
import { InProcessPublicDataProvider } from '../../exchanges/in-process-public-data';
import { DefaultStore } from '../../store/store.base';

// The shared public exchange has no keys, so fetchMarkets builds every market
// with leverage.max = 1. An account reading those markets must overlay its own
// brackets, or the leverage slider and setLeverage clamp everything to 1x.
const sharedMarket = () => ({
  id: 'BTC/USDT:USDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.1 },
  limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 1 } },
});

const sharedSourceWith = (markets: unknown[]) => {
  const sourceStore = new DefaultStore();
  sourceStore.update({ markets: markets as any, loaded: { ...sourceStore.loaded, markets: true } });
  return sourceStore;
};

const connectToSharedSource = (exchange: BinanceExchange | BinancePMExchange, sourceStore: DefaultStore) => {
  const source = {
    store: sourceStore,
    listenOrderBook: () => () => {},
    listenOHLCV: () => () => {},
  } as unknown as PublicDataSource;
  const provider = new InProcessPublicDataProvider(exchange, source);
  provider.connect();
  return provider;
};

const variants = [
  {
    name: 'binance',
    create: () => new BinanceExchange('binance', { key: 'test-key', secret: 'test-secret' }, new DefaultStore()),
    bracketClient: (ex: any) => ex.xhr,
  },
  {
    name: 'binancepm',
    create: () => new BinancePMExchange('binancepm', { key: 'test-key', secret: 'test-secret' }, new DefaultStore()),
    bracketClient: (ex: any) => ex.papiXhr,
  },
] as const;

describe.each(variants)('$name account on a shared public exchange', ({ create, bracketClient }) => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  const mockBrackets = (exchange: any) =>
    vi.spyOn(bracketClient(exchange), 'get').mockResolvedValue({
      data: [{ symbol: 'BTCUSDT', brackets: [{ bracket: 1, initialLeverage: 20 }] }],
    } as any);

  it("overlays the account's bracket max leverage without mutating the shared markets", async () => {
    const exchange = create();
    mockBrackets(exchange);
    const shared = sharedMarket();
    const sourceStore = sharedSourceWith([shared]);
    connectToSharedSource(exchange, sourceStore);

    await (exchange as any).loadAccountMaxLeverage();

    expect(exchange.store.markets[0].limits.leverage.max).toBe(20);
    expect(shared.limits.leverage.max).toBe(1);

    sourceStore.update({ markets: [sharedMarket()] as any });

    expect(exchange.store.markets[0].limits.leverage.max).toBe(20);
  });

  // start() loads brackets before connecting: the hosted store delta ignores
  // nested fields, so the very first markets the account holds must be adapted.
  it('seeds already-adapted markets when brackets load before the provider connects', async () => {
    const exchange = create();
    mockBrackets(exchange);
    await (exchange as any).loadAccountMaxLeverage();

    connectToSharedSource(exchange, sharedSourceWith([sharedMarket()]));

    expect(exchange.store.markets[0].limits.leverage.max).toBe(20);
  });

  it('start() loads brackets before initialising external public data', async () => {
    const exchange: any = create();
    exchange.options.externalPublicData = { type: 'in-process', source: {} };
    const calls: string[] = [];
    exchange.loadAccountMaxLeverage = async () => {
      calls.push('brackets');
    };
    vi.spyOn(exchange, 'initExternalPublicData').mockImplementation(async () => {
      calls.push('init');
      throw new Error('stop');
    });

    await expect(exchange.start()).rejects.toThrow('stop');

    expect(calls).toEqual(['brackets', 'init']);
  });
});
