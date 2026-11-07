// @vitest-environment node
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import { afterEach, expect, it, vi } from 'vitest';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
});

it.each([
  { name: 'forward sync', jump: 162 * 86_400_000, sleep: false },
  { name: 'backward sync', jump: -162 * 86_400_000, sleep: false },
  { name: 'two-hour laptop sleep', jump: 2 * 3_600_000, sleep: true },
  { name: 'refused sleep correction', jump: 2 * 3_600_000, sleep: true, refuseOnce: true },
])('keeps scheduling stable and repairs protocol time after $name', async ({ jump, sleep, refuseOnce }) => {
  const origin = 1_800_000_000_000;
  let monotonic = 0;
  let utc = origin;
  let wall = origin;
  vi.spyOn(Date, 'now').mockImplementation(() => wall);
  vi.spyOn(performance, 'now').mockImplementation(() => monotonic);
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => utc);
  const scheduling: number[] = [];
  const protocol: Array<[number, number]> = [];
  let refused = false;
  const idle = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('binance'),
    syncHostTime: (now, wall) => {
      if (refuseOnce && wall !== origin && !refused) { refused = true; return false; }
      protocol.push([now, wall]); return true;
    },
    start: (now) => { scheduling.push(now); return '[]'; },
    command: (json, now) => {
      scheduling.push(now);
      const command = JSON.parse(json);
      return command.command === undefined ? '[]'
        : JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]);
    },
    wsOpen: idle, wsMessage: idle, wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle, httpChunk: idle, httpEnd: idle, httpFailed: idle, timer: idle,
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
  };
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  installNativeStartupFrames(engine, store);
  setWasmEngineLoader(async () => ({ WasmEngine: function () { return engine; } as unknown as WasmEngineModule['WasmEngine'] }));
  setRustEngineTransport(); // Exercise the production default, not an injected clock.
  const exchange = new RustEngineExchange('binance', { key: 'fixture', secret: 'fixture' }, store);
  exchanges.push(exchange);
  await exchange.start();
  expect(scheduling.at(-1)).toBe(origin);
  monotonic = sleep ? 0 : 12_000;
  utc = origin + monotonic + jump;
  if (sleep) {
    wall += jump; // performance.now() does not advance across suspend.
  } else {
    // Real worker sync entrypoint, including the listener notification.
    virtualClock.syncFromServerTime(utc);
  }
  if (refuseOnce) {
    await expect((exchange as any).host.command({ op: 'fetchMarkets' })).rejects.toThrow('refused host protocol time');
    expect(scheduling.at(-1)).toBe(origin); // The failed correction cannot enter a command.
  }
  await (exchange as any).host.command({ op: 'fetchMarkets' });
  expect(scheduling.at(-1)).toBe(origin + monotonic);
  expect(protocol.at(-1)).toEqual([origin + monotonic, utc]);
  exchange.dispose();
  const observed = protocol.length;
  virtualClock.syncFromServerTime(utc + 1_000);
  expect(protocol).toHaveLength(observed); // No callbacks into a disposed engine.
});
