// @vitest-environment node
import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
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
  setRustEngineTransport(nativeTransport);
  vi.restoreAllMocks();
});

it.each([true, false])('preserves the explicit Bitunix clock despite legacy hostClock=%s', async (hostClock) => {
  const local = 1_760_000_600_000;
  let synced = 1_760_000_000_000;
  vi.spyOn(Date, 'now').mockReturnValue(local);
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => synced);
  const startTimes: number[] = [];
  const loginTimes: number[] = [];
  let handlers!: Parameters<Transport['connect']>[1];
  const idle = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => JSON.stringify({ ...JSON.parse(engineCapabilities('bitunix')), hostClock }),
    start: (now) => {
      startTimes.push(now);
      return JSON.stringify([{ op: 'wsConnect', socket: 1, url: 'wss://clock.invalid/private', channel: 'private' }]);
    },
    command: idle,
    wsOpen: (_socket, now) => { loginTimes.push(now); return '[]'; },
    wsMessage: idle,
    wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
  };
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  installNativeStartupFrames(engine, store);
  setWasmEngineLoader(async () => ({ WasmEngine: function () { return engine; } as unknown as WasmEngineModule['WasmEngine'] }));
  setRustEngineTransport(() => ({
    now: () => Date.now(),
    connect: (_url, callbacks) => { handlers = callbacks; return { send: () => {}, close: () => {} }; },
    request: () => new Promise(() => {}),
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  }));
  const exchange = new RustEngineExchange('bitunix', { key: 'replay-key', secret: 'replay-secret' }, store);
  exchanges.push(exchange);
  await exchange.start();
  expect(startTimes).toEqual([local]);
  synced += 12_345;
  handlers.onOpen();
  expect(loginTimes).toEqual([local]);
});
