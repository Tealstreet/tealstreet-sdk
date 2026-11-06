import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { isBetaVenue } from '../../exchanges/rust/betaVenues';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { SECONDARY_READ_KEYS_ENABLED } from '../../secondaryReadKeys';
import { DefaultStore } from '../../store/store.base';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

// Every venue that accepted a saved read key (engine.ts secondaryReadHeaders, plus Crypto.com).
const READ_KEY_VENUES: ExchangeName[] = [
  'binance',
  'binancepm',
  'binancespot',
  'binanceinverse',
  'asterdex',
  'bybit',
  'bybitinverse',
  'bybitspot',
  'bybitdemo',
  'hyrotrader',
  'okx',
  'okxeu',
  'okxdemo',
  'okxeudemo',
  'phemex',
  'backpack',
  'kraken',
  'krakenspot',
  'bitget',
  'bitgetusdc',
  'bitgetinverse',
  'kucoin',
  'kucoinspot',
  'mexc',
  'bitunix',
  'bingx',
  'bitrue',
  'weex',
  'blofin',
  'woo',
  'coinbase',
  'coinbasespot',
  'cryptocom',
];
const READ_PROXY = 'https://read-egress.invalid/api/public_proxy';

const exchanges: RustEngineExchange[] = [];
afterEach(() => exchanges.splice(0).forEach((exchange) => exchange.dispose()));

const construct = async (name: ExchangeName, options: Partial<ExchangeOptions>) => {
  const empty = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    start: empty,
    command: (json) => JSON.stringify([{ op: 'done', command: JSON.parse(json).command, result: 'null' }]),
    takeFrame: () =>
      new Uint8Array(
        consumerFrame({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } }),
      ),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: empty,
    wsMessage: empty,
    wsClose: empty,
    wsRouteExhausted: empty,
    httpStart: empty,
    httpChunk: empty,
    httpEnd: empty,
    httpFailed: empty,
    timer: empty,
  };
  const constructed = vi.fn(function (_venue: string, _options: string, _signerSetup?: string) {
    return engine;
  });
  setWasmEngineLoader(async () => ({ WasmEngine: constructed as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store);
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange(
    name,
    { key: 'primary-key', secret: 'primary-secret', passphrase: 'primary-pass', ...options } as ExchangeOptions,
    store,
  );
  exchanges.push(exchange);
  const logs: string[] = [];
  exchange.emitter.on('log', (message: string) => logs.push(String(message)));
  await exchange.start();
  return { exchange, constructed, logs };
};

it('ships with secondary read keys bypassed', () => {
  expect(SECONDARY_READ_KEYS_ENABLED).toBe(false);
});

// HyroTrader is the Bybit family, which is covered; it needs simulator URLs to start.
it.each(READ_KEY_VENUES.filter((name) => isBetaVenue(name) && name !== 'hyrotrader'))(
  '%s ignores a saved read key silently: nothing reaches signer setup or the engine',
  async (name) => {
    const { exchange, constructed, logs } = await construct(name, {
      secondary_key: 'read-key',
      secondary_secret: 'read-secret',
      secondary_passphrase: 'read-pass',
      secondaryProxyUrl: READ_PROXY,
    });
    expect(constructed).toHaveBeenCalledOnce();
    const [, engineOptions, signerSetup] = constructed.mock.calls[0]!;
    expect(engineOptions).not.toMatch(/read-key|read-secret|read-pass/);
    expect(JSON.parse(engineOptions)).not.toHaveProperty('readCredentials');
    expect(JSON.parse(engineOptions)).not.toHaveProperty('readCredentialsValid');
    if (signerSetup) {
      expect(signerSetup).not.toMatch(/read-key|read-secret|read-pass/);
      expect(JSON.parse(signerSetup)).not.toHaveProperty('secondary');
    }
    for (const field of ['secondary_key', 'secondary_secret', 'secondary_passphrase'])
      expect(exchange.options).not.toHaveProperty(field);
    // The read egress stays for unsigned public GETs; Coinbase drops it as it always has.
    expect(exchange.options.secondaryProxyUrl).toBe(name.startsWith('coinbase') ? undefined : READ_PROXY);
    expect(logs.join('\n')).not.toMatch(/read credentials|read-only|secondary/i);
  },
);

it('a partial saved read key no longer refuses the account', async () => {
  const { constructed } = await construct('okx', { secondary_key: 'read-key' });
  expect(constructed).toHaveBeenCalledOnce();
});

// Independent post-merge-review controls: primary identity must survive secondary metadata deprecation.
it.each([false, true])('review3093: secondary metadata is ignored with partial=%s', async (partial) => {
  const { constructed } = await construct('okx', {
    secondary_key: 'read-key',
    ...(partial ? {} : { secondary_secret: 'read-secret', secondary_passphrase: 'read-pass' }),
    localSigner: { primary: { stableId: 'primary-owner' }, secondary: { stableId: 'read-owner' } },
  });
  expect(constructed).toHaveBeenCalledOnce();
  const setup = JSON.parse(constructed.mock.calls[0]![2]!);
  expect(setup.primary.stableId).toBe('primary-owner');
  expect(setup).not.toHaveProperty('secondary');
});
