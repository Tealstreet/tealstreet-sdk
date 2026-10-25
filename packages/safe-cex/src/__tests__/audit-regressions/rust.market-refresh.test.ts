import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { Market } from '../../types';

import { afterEach, expect, it, vi } from 'vitest';

import { WORKER_FORWARDED_METHODS, WORKER_WRITE_METHODS } from '../../exchanges/base';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

const running: RustEngineExchange[] = [];
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  vi.restoreAllMocks();
});
const market = (symbol: string): Market => ({
  id: symbol,
  symbol,
  base: symbol,
  quote: 'USDT',
  active: true,
  precision: { amount: 1, price: 0.1 },
  limits: { amount: { min: 1, max: 100 }, leverage: { min: 1, max: 10 } },
});
async function fixture(marketRefresh = true) {
  const noop = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('bybit', { marketRefresh }),
    start: noop,
    command: noop,
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noop,
    wsMessage: noop,
    wsClose: noop, wsRouteExhausted: noop,
    httpStart: noop,
    httpChunk: noop,
    httpEnd: noop,
    httpFailed: noop,
    timer: noop,
  };
  setWasmEngineLoader(async () => ({
    WasmEngine: function () {
      return engine;
    } as unknown as WasmEngineModule['WasmEngine'],
  }));
  const store = new DefaultStore();
  store.update({
    markets: [market('BTCUSDT'), market('OLDUSDT')],
    tickers: [],
    loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
  });
  let markets = store.markets;
  installNativeStartupFrames(engine, store, () => ({ markets }));
  const exchange = new RustEngineExchange('bybit', { key: '', secret: '', publicDataOnly: true }, store);
  running.push(exchange);
  await exchange.start();
  const internals = exchange as unknown as { command(command: { op: string }): Promise<unknown> };
  const command = vi.spyOn(internals, 'command');
  const host = exchange.socketHost!;
  return {
    exchange,
    store,
    command,
    host,
    commit: () => {
      markets = [market('BTCUSDT'), market('NEWUSDT')];
    },
  };
}

it('dispatches one refresh for concurrent callers and flushes native state before diffing once', async () => {
  const f = await fixture();
  const changed = vi.fn();
  f.exchange.on('markets:changed', changed);
  let finish!: () => void;
  f.command.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = () => {
          f.commit();
          resolve();
        };
      }),
  );
  const flush = vi.spyOn(f.host, 'flush');
  const first = f.exchange.refreshMarkets();
  const concurrent = f.exchange.refreshMarkets();
  await vi.waitFor(() => expect(f.command).toHaveBeenCalledOnce());
  expect(f.command).toHaveBeenCalledWith({ op: 'refreshMarkets' });
  expect(f.store.markets.map((row) => row.id)).toEqual(['BTCUSDT', 'OLDUSDT']);
  finish();
  const result = await first;
  await expect(concurrent).resolves.toEqual(result);
  expect(flush).toHaveBeenCalledWith();
  expect(result.added.map((row) => row.id)).toEqual(['NEWUSDT']);
  expect(result.removed.map((row) => row.id)).toEqual(['OLDUSDT']);
  expect(changed).toHaveBeenCalledOnce();
  expect(changed).toHaveBeenCalledWith(result);
  f.command.mockResolvedValue(undefined);
  await expect(f.exchange.refreshMarkets()).resolves.toEqual({ added: [], removed: [] });
  expect(f.command).toHaveBeenCalledTimes(2);
  expect(changed).toHaveBeenCalledOnce();
});

it('preserves a failed refresh, and fetchMarkets/fetchTickers remain cached reads', async () => {
  const f = await fixture();
  const before = f.store.markets;
  const changed = vi.fn();
  f.exchange.on('markets:changed', changed);
  await f.exchange.fetchMarkets();
  await f.exchange.fetchTickers();
  expect(f.command).not.toHaveBeenCalled();
  f.command.mockRejectedValue(new Error('Discovery failed'));
  await expect(f.exchange.refreshMarkets()).rejects.toThrow('Discovery failed');
  expect(f.store.markets).toBe(before);
  expect(changed).not.toHaveBeenCalled();
  f.command.mockImplementation(async () => {
    f.commit();
  });
  await expect(f.exchange.refreshMarkets()).resolves.toMatchObject({ added: [{ id: 'NEWUSDT' }] });
});

it('keeps unsupported engines cached and shared consumers on their provider catalogue', async () => {
  const unsupported = await fixture(false);
  await expect(unsupported.exchange.refreshMarkets()).resolves.toEqual({ added: [], removed: [] });
  expect(unsupported.command).not.toHaveBeenCalled();
  const shared = await fixture();
  vi.spyOn(shared.exchange, 'publicFeedMode', 'get').mockReturnValue('shared');
  await expect(shared.exchange.refreshMarkets()).rejects.toThrow('Refresh the shared public feed source');
  expect(shared.command).not.toHaveBeenCalled();
});

it('forwards worker refresh to the authoritative engine without starting a mirror host', async () => {
  const construct = vi.fn();
  setWasmEngineLoader(construct);
  const exchange = new RustEngineExchange('bybit', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  running.push(exchange);
  const delta = { added: [market('NEWUSDT')], removed: [] };
  const calls: string[] = [];
  exchange.emitter.on('callMethod', ({ method, requestId }: { method: string; requestId: string }) => {
    calls.push(method);
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: delta });
  });
  await expect(exchange.refreshMarkets()).resolves.toEqual(delta);
  expect(calls).toEqual(['refreshMarkets']);
  expect(construct).not.toHaveBeenCalled();
  expect(WORKER_FORWARDED_METHODS).toContain('refreshMarkets');
  expect(WORKER_WRITE_METHODS.has('refreshMarkets')).toBe(false);
});
