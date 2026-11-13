import type { ExchangeName, ExchangeOptions } from '../../types';
import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';

import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

const endpoints = vi.hoisted(() => ({
  productionRest: { livenet: '', testnet: '' },
  productionPublic: { livenet: '', testnet: '' },
  productionPrivate: { livenet: '', testnet: '' },
  simulatorRest: { livenet: '', testnet: '' },
  simulatorPublic: { livenet: '', testnet: '' },
  simulatorPrivate: { livenet: '', testnet: '' },
}));

vi.mock('../../exchanges/bybit/bybit.types', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../exchanges/bybit/bybit.types')>()),
  HYROTRADER_PROD_URL: endpoints.productionRest,
  HYROTRADER_PROD_WS_URL: { public: { linear: endpoints.productionPublic }, private: endpoints.productionPrivate },
  HYROTRADER_SIMULATOR_URL: endpoints.simulatorRest,
  HYROTRADER_SIMULATOR_WS_URL: { public: { linear: endpoints.simulatorPublic }, private: endpoints.simulatorPrivate },
}));

const exchanges: RustEngineExchange[] = [];
const loaderFailure = new Error('configured venue reached the wasm loader');
const loader = vi.fn(async () => {
  throw loaderFailure;
});
const create = (options: Partial<ExchangeOptions> = {}, name: ExchangeName = 'hyrotrader') => {
  const exchange = new RustEngineExchange(
    name,
    { key: '', secret: '', publicDataOnly: true, ...options },
    new DefaultStore(),
  );
  exchanges.push(exchange);
  return exchange;
};
const configure = (mode: 'production' | 'simulator') => {
  for (const part of ['Rest', 'Public', 'Private'] as const) {
    const url = `${part === 'Rest' ? 'https' : 'wss'}://configured.invalid`;
    Object.assign(endpoints[`${mode}${part}`], { livenet: url, testnet: url });
  }
};

beforeEach(() => {
  for (const urls of Object.values(endpoints)) Object.assign(urls, { livenet: '', testnet: '' });
  loader.mockRejectedValue(loaderFailure);
  setWasmEngineLoader(loader);
});
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
});

describe('HyroTrader Rust startup configuration', () => {
  it('refuses explicit production with missing livenet instead of loading the simulator', async () => {
    configure('simulator');
    const io = vi.fn(() => { throw new Error('Unconfigured production must not send requests'); });
    const transport = vi.fn((): Transport => ({
      now: () => 0, request: io, connect: io, setTimeout: vi.fn(), clearTimeout: vi.fn(),
    }));
    setRustEngineTransport(transport);
    try {
      const exchange = create({ extra: { hyrotraderMode: 'production' } });
      exchange.emitter.on('error', vi.fn());
      await expect(exchange.start()).rejects.toMatchObject({
        message: 'HyroTrader production URLs are not configured in this build.',
        kind: 'invalidInput', outcome: 'notSent',
      });
      expect(loader).not.toHaveBeenCalled();
      expect(transport).not.toHaveBeenCalled();
      expect(io).not.toHaveBeenCalled();
    } finally { setRustEngineTransport(nativeTransport); }
  });

  it('uses configured production testnet without requiring production livenet', async () => {
    configure('production');
    configure('simulator');
    endpoints.productionRest.livenet = '';
    endpoints.simulatorRest.testnet = '';
    const exchange = create({ testnet: true, extra: { hyrotraderMode: 'production' } });
    await expect(exchange.start()).rejects.toBe(loaderFailure);
    expect(loader).toHaveBeenCalledOnce();
  });

  it.each([false, true])('emits a typed unconfigured-build error before loading wasm (testnet=%s)', async (testnet) => {
    const exchange = create({ testnet });
    const reported = vi.fn();
    exchange.emitter.on('error', reported);
    const started = exchange.start();
    await expect(started).rejects.toMatchObject({
      message: 'HyroTrader production URLs are not configured in this build.',
      kind: 'invalidInput',
      outcome: 'notSent',
      errorReported: true,
    });
    expect(exchange.start()).toBe(started);
    expect(reported).toHaveBeenCalledOnce();
    expect(reported).toHaveBeenCalledWith(
      'HyroTrader production URLs are not configured in this build.',
      expect.objectContaining({ classified: expect.objectContaining({ kind: 'invalidInput', outcome: 'notSent' }) }),
    );
    expect(loader).not.toHaveBeenCalled();
  });

  it('does not borrow production endpoints for an unconfigured demo', async () => {
    configure('production');
    const exchange = create({ extra: { hyrotraderMode: 'demo' } });
    exchange.emitter.on('error', vi.fn());
    await expect(exchange.start()).rejects.toMatchObject({
      message: 'HyroTrader simulator URLs are not configured in this build.',
      kind: 'invalidInput',
    });
    expect(loader).not.toHaveBeenCalled();
  });

  it.each(['productionRest', 'productionPublic', 'productionPrivate'] as const)(
    'requires %s on the selected network before loading wasm',
    async (endpoint) => {
      configure('production');
      endpoints[endpoint].testnet = '';
      const exchange = create({ testnet: true });
      exchange.emitter.on('error', vi.fn());
      await expect(exchange.start()).rejects.toMatchObject({
        message: 'HyroTrader production URLs are not configured in this build.',
        kind: 'invalidInput',
      });
      expect(loader).not.toHaveBeenCalled();
    },
  );

  it('does not apply HyroTrader endpoint requirements to Bybit', async () => {
    const exchange = create({}, 'bybit');
    await expect(exchange.start()).rejects.toBe(loaderFailure);
    expect(loader).toHaveBeenCalledOnce();
    expect(loader).toHaveBeenCalledWith('bybit');
  });

  it.each(['production', 'simulator'] as const)(
    'preserves configured %s selection and the loader failure',
    async (mode) => {
      configure(mode);
      const exchange = create({ extra: { hyrotraderMode: mode === 'simulator' ? 'demo' : 'production' } });
      await expect(exchange.start()).rejects.toBe(loaderFailure);
      expect(loader).toHaveBeenCalledOnce();
      expect(loader).toHaveBeenCalledWith('hyrotrader');
    },
  );
});
