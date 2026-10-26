vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

// @vitest-environment node
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeAccount, ExchangeName, ExchangeOptions } from '../../types';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import {
  RUST_VENUES,
  RustEngineExchange,
  setRustEngineTransport,
  setWasmEngineLoader,
} from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { nativeReadyFrame } from './rust.native-startup.fixtures';

// Audited TS getAccount implementations and inherited product aliases. Keep this
// independent from production dispatch, including Kraken Spot's balance command.
const commandAliases: ExchangeName[] = [
  'apexomni',
  'asterdex',
  'backpack',
  'binance',
  'binancepm',
  'bingx',
  'bitget',
  'bitgetinverse',
  'bitgetusdc',
  'bybit',
  'bybitinverse',
  'bybitspot',
  'bybitdemo',
  'cryptocom',
  'grvt',
  'kucoin',
  'kucoinspot',
  'lighter',
  'lighter-robinhood',
  'mexc',
  'paradex',
  'phemex',
  'polymarket',
  'weex',
  'woo',
];
const coinbaseAliases = ['coinbase', 'coinbasespot'] as const;
const orderlyAliases = ['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi'] as const;
const unsupportedAliases: ExchangeName[] = [
  'binancespot',
  'binanceinverse',
  'bitrue',
  'bitunix',
  'blofin',
  'breakoutprop',
  'hyrotrader',
  'hyperliquid',
  'hyperliquidspot',
  'kraken',
  'krakenspot',
  'okx',
  'okxeu',
  'okxdemo',
  'okxeudemo',
  'txflow',
];
const identity: ExchangeAccount = {
  userId: '900719925474099312345',
  subId: '900719925474099312346',
  affiliateId: '000000000000000017',
};
const accounts: RustEngineExchange[] = [];

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, 'trace').mockImplementation(() => {});
});
afterEach(() => {
  accounts.splice(0).forEach((account) => account.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function fixture(name: ExchangeName, options: Partial<ExchangeOptions> = {}) {
  let value: ExchangeAccount = identity;
  let failure: unknown;
  let nativeError: string | undefined;
  const commands: Array<{ op: string; command?: number }> = [];
  const idle = () => '[]';
  const native: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    start: vi.fn(idle),
    command(json) {
      // The caller wait is engine receipt metadata (pinned by the host tests),
      // not part of the native command these regressions compare.
      const { timeoutMs: _wait, ...command } = JSON.parse(json);
      commands.push(command);
      if (command.command === undefined) return '[]';
      if (failure !== undefined) throw failure;
      return JSON.stringify([
        ...(nativeError ? [{ op: 'commandError', command: command.command, message: nativeError }] : []),
        {
          op: 'done',
          command: command.command,
          ...(nativeError ? { error: nativeError } : { result: JSON.stringify(value) }),
        },
      ]);
    },
    takeFrame: nativeReadyFrame,
    snapshotFrame: () =>
      new Uint8Array(
        consumerFrame({
          markets: [],
          tickers: [],
          loaded: {
            markets: false,
            tickers: false,
            orders: false,
            positions: false,
            balance: false,
          },
        }),
      ),
    wsOpen: idle,
    wsMessage: idle,
    wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
  };
  const construct = vi.fn(function (_name: string, _options: string) {
    return native;
  });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const readOptions: Partial<ExchangeOptions> = orderlyAliases.includes(name as (typeof orderlyAliases)[number])
    ? { key: '11111111111111111111111111111111', secret: '11111111111111111111111111111111' }
    : name === 'asterdex'
      ? { extra: { asterdexApiVersion: 'v1' } }
      : name === 'polymarket'
        ? { key: `0x${'11'.repeat(32)}` }
        : name === 'paradex'
          ? { paradexAddress: '0x123', subkeyAddress: '0x456' }
          : ['lighter', 'lighter-robinhood'].includes(name)
            ? { publicDataOnly: true, key: '', secret: '', applicationId: '0' }
            : {};
  const store = new DefaultStore();
  const exchange = new RustEngineExchange(
    name,
    {
      key: 'primary-key',
      secret: 'primary-secret',
      passphrase: 'primary-passphrase',
      applicationId: 'configured-account',
      publicDataOnly: false,
      tuning: { consoleLogging: false },
      ...readOptions,
      ...options,
    },
    store,
  );
  accounts.push(exchange);
  const errors = vi.fn();
  exchange.on('error', errors);
  return {
    exchange,
    store,
    construct,
    native,
    errors,
    commands,
    queries: () => commands.filter((command) => command.command !== undefined),
    result: (result: ExchangeAccount) => {
      value = result;
    },
    failure: (error: unknown) => {
      failure = error;
    },
    nativeError: (message: string) => {
      nativeError = message;
    },
  };
}

it('retains all historical identity cases and covers the beta alias subset', () => {
  const inventory = [...commandAliases, ...coinbaseAliases, ...orderlyAliases, ...unsupportedAliases];
  expect(new Set(inventory).size).toBe(49);
  expect(inventory.filter((name) => RUST_VENUES.has(name)).sort()).toEqual([...RUST_VENUES].sort());
  expect(commandAliases.length + coinbaseAliases.length + orderlyAliases.length).toBe(33);
});

it.each(commandAliases)(
  '%s returns exact native identity through one cold command without starting reconciliation',
  async (name) => {
    const r = fixture(name);
    const before = r.store.serialize();
    await expect(r.exchange.getAccount()).resolves.toEqual(identity);
    expect(r.queries()).toEqual([{ op: 'fetchAccount', command: 1 }]);
    expect(r.construct).toHaveBeenCalledOnce();
    expect(r.native.start).not.toHaveBeenCalled();
    expect(r.store.serialize()).toEqual(before);
    expect(r.errors).not.toHaveBeenCalled();
  },
);

it.each(unsupportedAliases)(
  '%s rejects unsupported identity from its capability record without commands or startup',
  async (name) => {
    const r = fixture(name);
    // HyroTrader's URLs come from build-time env, so here its host refuses to build.
    await expect(r.exchange.getAccount()).rejects.toThrow(
      name === 'hyrotrader' ? /Not implemented|URLs are not configured/ : /Not implemented/,
    );
    expect(r.native.start).not.toHaveBeenCalled();
    expect(r.queries()).toEqual([]);
    // Identity reads leave failure visibility to their caller.
    expect(r.errors).not.toHaveBeenCalled();
  },
);

it.each(coinbaseAliases)('%s asks the started engine for its native portfolio identity', async (name) => {
  const r = fixture(name);
  const result = r.exchange.getAccount();
  await vi.advanceTimersByTimeAsync(250);
  await expect(result).resolves.toEqual(identity);
  expect(r.native.start).toHaveBeenCalledOnce();
  expect(r.queries()).toEqual([{ op: 'fetchAccount', command: 1 }]);
});

it.each(orderlyAliases)('%s asks the started engine for its Orderly identity', async (name) => {
  const r = fixture(name);
  const result = r.exchange.getAccount();
  await vi.advanceTimersByTimeAsync(250);
  await expect(result).resolves.toEqual(identity);
  expect(r.native.start).toHaveBeenCalledOnce();
  expect(r.queries()).toEqual([{ op: 'fetchAccount', command: 1 }]);
});

it.each<ExchangeAccount>([
  { userId: 'mexc:native-primary-key-fingerprint' },
  { userId: 'weex:native-primary-key-fingerprint' },
  { userId: '0', subId: null, affiliateId: null },
  { userId: 'masked-***', affiliateId: '0' },
])('retains native identity namespaces and missing/null fields %#', async (result) => {
  const r = fixture('mexc');
  r.result(result);
  const received = await r.exchange.getAccount();
  expect(received).toEqual(result);
  expect(Object.keys(received)).toEqual(Object.keys(result));
});

it('shares one host across concurrent cold identity reads without starting streams', async () => {
  const r = fixture('backpack');
  await expect(Promise.all([r.exchange.getAccount(), r.exchange.getAccount()])).resolves.toEqual([identity, identity]);
  expect(r.construct).toHaveBeenCalledOnce();
  expect(r.queries().map(({ command }) => command)).toEqual([1, 2]);
  expect(r.native.start).not.toHaveBeenCalled();
});

it.each(['lighter', 'lighter-robinhood'] as const)(
  '%s permits unsigned index-zero identity without a signer',
  async (name) => {
    const r = fixture(name);
    r.result({ userId: '0', affiliateId: '900719925474099312346' });
    await expect(r.exchange.getAccount()).resolves.toEqual({ userId: '0', affiliateId: '900719925474099312346' });
    const nativeOptions = JSON.parse(r.construct.mock.calls[0][1]);
    expect(nativeOptions).toMatchObject({ publicDataOnly: true, key: '', secret: '', applicationId: '0' });
    expect(nativeOptions).not.toHaveProperty('apiKeyIndex');
    expect(r.native.start).not.toHaveBeenCalled();
  },
);

it('propagates a missing native identity rejection once without empty-account fallback or retry', async () => {
  const r = fixture('phemex');
  r.nativeError('Phemex account-position response did not include a user ID');
  await expect(r.exchange.getAccount()).rejects.toThrow('did not include a user ID');
  expect(r.errors).not.toHaveBeenCalled();
  expect(r.queries()).toHaveLength(1);
  expect(r.exchange.isDisposed).toBe(false);
});

it('preserves an original frozen failure and does not construct another host to retry it', async () => {
  const r = fixture('woo');
  const failure = Object.freeze(Object.assign(new Error('query transport failed'), { code: 'CONTROLLED_READ' }));
  r.failure(failure);
  await expect(r.exchange.getAccount()).rejects.toBe(failure);
  expect(r.errors).not.toHaveBeenCalled();
  expect(r.construct).toHaveBeenCalledOnce();
  expect(r.queries()).toHaveLength(1);
});

it('propagates initialization failure without dispatching a native query', async () => {
  const r = fixture('kucoin');
  const failure = new Error('WASM loader failed');
  setWasmEngineLoader(async () => {
    throw failure;
  });
  await expect(r.exchange.getAccount()).rejects.toBe(failure);
  expect(r.construct).not.toHaveBeenCalled();
  expect(r.queries()).toEqual([]);
});

it('fences disposed identity reads without cached state or reconstruction', async () => {
  const r = fixture('binance');
  r.exchange.dispose();
  await expect(r.exchange.getAccount()).rejects.toThrow('disposed');
  expect(r.construct).not.toHaveBeenCalled();
  expect(r.queries()).toEqual([]);
});

it('keeps a native identity trap terminal and never retries it or exposes cached account options', async () => {
  const r = fixture('grvt');
  r.failure(new WebAssembly.RuntimeError('unreachable'));
  await expect(r.exchange.getAccount()).rejects.toMatchObject({ code: 'ENGINE_DEAD' });
  await expect(r.exchange.getAccount()).rejects.toMatchObject({ code: 'ENGINE_DEAD' });
  expect(r.queries()).toHaveLength(1);
  expect(r.construct).toHaveBeenCalledOnce();
  expect(r.errors).toHaveBeenCalledOnce();
});
