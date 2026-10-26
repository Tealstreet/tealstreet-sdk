vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions, PlaceOrderOpts } from '../../types';

import { MessageChannel } from 'node:worker_threads';

import { afterEach, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderType } from '../../types';
import { adoptEngineCapabilities, engineCapabilities } from '../fixtures/engineCapabilities';

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

const setup = (name: ExchangeName, options: Partial<ExchangeOptions> = {}) => {
  const exchange = new RustEngineExchange(
    name,
    { key: '', secret: '', publicDataOnly: true, ...options },
    new DefaultStore(),
  );
  exchanges.push(exchange);
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  return exchange;
};

it.each([
  [false, false],
  [true, false],
  [false, true],
  [true, true],
])(
  'forwards saved Bybit inverse=%s and options=%s flags to the real host constructor boundary',
  async (enableInverseMarkets, enableOptionMarkets) => {
    const constructor = vi.fn();
    class Engine {
      constructor(venue: string, options: string) {
        constructor(venue, JSON.parse(options));
      }
      capabilities() {
        return engineCapabilities('bybit', { options: enableOptionMarkets });
      }
      free() {}
    }
    setWasmEngineLoader(async () => ({ WasmEngine: Engine }) as unknown as WasmEngineModule);
    const exchange = setup('bybit', { enableInverseMarkets, enableOptionMarkets });
    await (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost();
    expect(constructor).toHaveBeenCalledExactlyOnceWith(
      'bybit',
      expect.objectContaining({ enableInverseMarkets, enableOptionMarkets }),
    );
  },
);

it.each(['binance', 'okx', 'grvt', 'bybitdemo', 'bybitinverse', 'bybitspot', 'hyrotrader'] as const)(
  'rejects option queries and IV batches for unsupported %s before an engine command',
  async (name) => {
    const exchange = setup(name, { enableOptionMarkets: true });
    const command = vi.fn();
    Object.assign(exchange, { host: { command, dispose: vi.fn() } });
    adoptEngineCapabilities(exchange);
    expect(() => exchange.fetchOptionChain({ underlying: 'BTC' })).toThrow(/Options are not enabled/);
    expect(() => exchange.fetchOptionExpiries('BTC')).toThrow(/Options are not enabled/);
    expect(() => exchange.fetchAccountGreeks()).toThrow(/Options are not enabled/);
    const plain: PlaceOrderOpts = {
      symbol: 'BTCUSDT',
      side: OrderSide.Buy,
      type: OrderType.Limit,
      price: 100,
      amount: 1,
    };
    await expect(exchange.placeOrders([plain, { ...plain, impliedVolatility: 0.5 }])).rejects.toThrow(
      /Options are not enabled/,
    );
    expect(command).not.toHaveBeenCalled();
  },
);

it('rejects legacy TS public providers before native product construction', async () => {
  const constructor = vi.fn();
  class Engine {
    constructor(venue: string, options: string) {
      constructor(venue, JSON.parse(options));
    }
    capabilities() {
      return engineCapabilities('bybit', { options: true });
    }
    free() {}
  }
  setWasmEngineLoader(async () => ({ WasmEngine: Engine }) as unknown as WasmEngineModule);
  const exchange = setup('bybit', {
    enableInverseMarkets: true,
    enableOptionMarkets: true,
    externalPublicData: { type: 'websocket', url: 'wss://fixture.invalid', exchangeId: 'bybit' },
  });
  await expect((exchange as unknown as { ensureHost(): Promise<void> }).ensureHost()).rejects.toThrow(
    'Rust externalPublicData websocket gateways are unsupported',
  );
  expect(constructor).not.toHaveBeenCalled();
});

it('keeps combined native products enabled with an explicit shared-feed policy', async () => {
  const constructor = vi.fn();
  class Engine {
    constructor(venue: string, options: string) {
      constructor(venue, JSON.parse(options));
    }
    capabilities() {
      return engineCapabilities('bybit', { options: true });
    }
    free() {}
  }
  setWasmEngineLoader(async () => ({ WasmEngine: Engine }) as unknown as WasmEngineModule);
  const channel = new MessageChannel();
  const exchange = setup('bybit', {
    enableInverseMarkets: true,
    enableOptionMarkets: true,
    publicFeed: { mode: 'shared', port: channel.port2 as unknown as MessagePort },
  });
  try {
    await (exchange as unknown as { ensureHost(): Promise<void> }).ensureHost();
  } finally {
    channel.port1.close();
    channel.port2.close();
  }
  expect(constructor).toHaveBeenCalledExactlyOnceWith(
    'bybit',
    expect.objectContaining({
      enableInverseMarkets: true,
      enableOptionMarkets: true,
      publicFeed: { mode: 'shared' },
    }),
  );
});

it('rejects non-finite IV before JSON could erase it and forwards supported option query fields verbatim', async () => {
  const exchange = setup('bybit', { enableOptionMarkets: true });
  const command = vi.fn().mockResolvedValue([]);
  Object.assign(exchange, { host: { command, dispose: vi.fn() } });
  adoptEngineCapabilities(exchange, { options: true });
  for (const impliedVolatility of [NaN, Infinity, 0, -1]) {
    await expect(
      exchange.placeOrder({
        symbol: 'BTC-25DEC26-100000-C',
        side: OrderSide.Buy,
        type: OrderType.Limit,
        amount: 1,
        impliedVolatility,
      }),
    ).rejects.toThrow(/finite and positive/);
  }
  expect(command).not.toHaveBeenCalled();
  for (const field of ['expiry', 'strikeMin', 'strikeMax'] as const) {
    expect(() => exchange.fetchOptionChain({ underlying: 'BTC', [field]: NaN })).toThrow(/must be finite/);
  }
  expect(command).not.toHaveBeenCalled();
  const query = {
    underlying: 'BTC',
    right: 'call' as const,
    expiry: 1_800_000_000_000,
    strikeMin: 90_000,
    strikeMax: 110_000,
  };
  await exchange.fetchOptionChain(query);
  expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'fetchOptionChain', query });
});
