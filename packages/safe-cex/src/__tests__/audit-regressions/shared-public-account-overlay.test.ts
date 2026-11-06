import type { Market } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { BybitExchange } from '../reference/bybit/bybit.exchange';
import { BybitInverseExchange } from '../reference/bybit/bybitinverse.exchange';
import { OKXEUExchange } from '../reference/okx/okxeu.exchange';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';

// On exchange-server, accounts take markets from a keyless shared exchange,
// which cannot fetch account-scoped data. start() must load that data BEFORE
// initExternalPublicData seeds markets (the hosted store delta ignores nested
// fields), and the overlay must never mutate the shared market objects.
const sharedMarket = (symbol: string, id = symbol): Market =>
  ({
    id,
    symbol,
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.001, price: 0.1 },
    limits: { amount: { min: 0.001, max: 1000 }, leverage: { min: 1, max: 100 } },
  }) as Market;

/** Runs start() up to initExternalPublicData and returns what the overlay does at that moment. */
const overlayAtProviderSeed = async (exchange: any, shared: Market[]) => {
  exchange.options.externalPublicData = { type: 'in-process', source: {} };
  vi.spyOn(virtualClock, 'start').mockResolvedValue(undefined as any);
  let adapted: Market[] | undefined;
  vi.spyOn(exchange, 'initExternalPublicData').mockImplementation(async () => {
    adapted = exchange.adaptExternalMarkets(shared);
    throw new Error('stop');
  });
  await expect(exchange.start()).rejects.toThrow('stop');
  return adapted!;
};

const feeResponse = (symbol: string) => ({
  data: { result: { list: [{ symbol, takerFeeRate: '0.0004', makerFeeRate: '0.0001' }] } },
});

describe('account market overlay on a shared public exchange', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it.each([
    {
      name: 'bybit',
      create: () => new BybitExchange('bybit' as any, { key: 'k', secret: 's' } as any, new DefaultStore()),
      symbol: 'BTCUSDT',
    },
    {
      name: 'bybitinverse',
      create: () =>
        new BybitInverseExchange('bybitinverse' as any, { key: 'k', secret: 's' } as any, new DefaultStore()),
      symbol: 'BTCUSD',
    },
  ])('$name loads account fee rates before the provider seeds markets', async ({ create, symbol }) => {
    const exchange: any = create();
    exchange.xhr = { get: vi.fn().mockResolvedValue(feeResponse(symbol)) };
    const shared = [sharedMarket(symbol)];

    const adapted = await overlayAtProviderSeed(exchange, shared);

    expect(adapted[0]).toMatchObject({ takerFee: 0.0004, makerFee: 0.0001 });
    expect(shared[0]).not.toHaveProperty('takerFee');
  });

  it('okxeu narrows shared markets to its account instruments and takes their limits', async () => {
    const exchange: any = new OKXEUExchange(
      'okxeu' as any,
      { key: 'k', secret: 's', passphrase: 'p' } as any,
      new DefaultStore(),
    );
    const accountInstrument = {
      instId: 'BTC-USDT-SWAP',
      instType: 'SWAP',
      ctType: 'linear',
      uly: 'BTC-USDT',
      settleCcy: 'USDT',
      ctValCcy: 'BTC',
      state: 'live',
      lotSz: '0.01',
      tickSz: '0.1',
      minSz: '0.01',
      ctVal: '0.01',
      lever: '20',
      maxIcebergSz: '1000',
      maxLmtSz: '1000',
      maxMktSz: '1000',
      maxStopSz: '1000',
      maxTriggerSz: '1000',
      maxTwapSz: '1000',
    };
    exchange.xhr = {
      get: vi.fn((_url: string, opts: any) =>
        Promise.resolve({ data: { data: opts?.params?.instType === 'SWAP' ? [accountInstrument] : [] } }),
      ),
    };
    const shared = [
      sharedMarket('BTC-USDT-SWAP-sym', 'BTC-USDT-SWAP'),
      sharedMarket('ETH-USDT-SWAP-sym', 'ETH-USDT-SWAP'),
    ];

    const adapted = await overlayAtProviderSeed(exchange, shared);

    expect(adapted.map((m) => m.id)).toEqual(['BTC-USDT-SWAP']);
    expect(adapted[0].limits.leverage.max).toBe(20);
    expect(shared[0].limits.leverage.max).toBe(100);
  });
});
