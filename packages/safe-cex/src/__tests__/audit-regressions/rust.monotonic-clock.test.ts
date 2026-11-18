// @vitest-environment node
import { createHmac } from 'node:crypto';
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


it.each(['pacing wait', 'between group sends'])('keeps the engine alive across a wall step during %s', async (where) => {
  const origin = 1_800_000_000_000;
  let wall = origin;
  let protocolUtc = origin;
  let protocolRaw = origin;
  vi.spyOn(Date, 'now').mockImplementation(() => wall);
  vi.spyOn(performance, 'now').mockReturnValue(0); // A suspended monotonic clock.
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => wall);
  const idle = () => '[]';
  const preparations = ['p1:7:1:1', 'p1:8:1:1'];
  const permit = 'g1;r1:7:1:1:1;r1:8:1:2:1';
  const packet = (request: number) => ({ request, method: 'POST', url: `https://fixture.invalid/${request}`, headers: [], timeoutMs: 1000, body: '{}' });
  const sent: number[] = [];
  const received: any[] = [];
  const failed: number[] = [];
  const timers: Array<() => void> = [];
  let prepared = 0;
  let claims = 0;
  let commandId: number | undefined;
  let host: any;
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('binance'),
    syncHostTime: (now, utc) => {
      expect(Boolean(host?.managedHttp.groupHandoff)).toBe(false);
      protocolRaw = now; protocolUtc = utc; return true;
    },
    start: idle,
    // Model the engine signing entry; real Binance timestamp/HMAC coverage is
    // in history_clock_resync.rs. No real request or credential is used here.
    command: (json, now) => {
      const command = JSON.parse(json);
      if (command.command === undefined) return '[]';
      commandId = command.command;
      const timestamp = protocolUtc + now - protocolRaw;
      const query = `timestamp=${timestamp}`;
      const signature = createHmac('sha256', 'fixture').update(query).digest('hex');
      return JSON.stringify([{ op: 'http', request: 9, method: 'GET', url: `https://fixture.invalid/signed?${query}&signature=${signature}`, headers: [] }]);
    },
    httpStart: idle, httpChunk: idle,
    httpEnd: () => JSON.stringify([{ op: 'done', command: commandId, result: '[]' }]),
    httpFailed: (id) => { failed.push(id); return '[]'; },
    wsOpen: idle, wsMessage: idle, wsClose: idle, wsRouteExhausted: idle, timer: idle,
    takeFrame: () => undefined, snapshotFrame: () => new Uint8Array(),
  };
  Object.assign(engine, {
    httpPreparationPending: (token: string) => preparations.includes(token),
    httpGroupPending: (token: string) => token === permit,
    httpPrepared: () => ++prepared === 2 ? JSON.stringify([{ op: 'managedHttp', phase: 'bindGroup', permit,
      members: preparations.map((preparation, i) => ({ preparation, packet: packet(i + 7) })) }]) : '[]',
    httpPreparationFailed: () => { throw new Error('a clock step is not a preparation failure'); },
    httpClaimGroup: (_: string, __: number, bounds: Float64Array) => {
      if (++claims === 1 && where === 'pacing wait') { bounds[0] = origin + 250; return 2; }
      bounds.fill(origin + 1000); return 1;
    },
    prepareDispatch: idle,
  });
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  installNativeStartupFrames(engine, store);
  setWasmEngineLoader(async () => ({ WasmEngine: function () { return engine; } as unknown as WasmEngineModule['WasmEngine'] }));
  setRustEngineTransport();
  const exchange = new RustEngineExchange('binance', { key: 'fixture', secret: 'fixture' }, store);
  exchanges.push(exchange);
  await exchange.start();
  host = (exchange as any).host;
  host.transport.setTimeout = (fn: () => void) => { timers.push(fn); return timers.length; };
  host.transport.clearTimeout = () => {};
  host.transport.request = async (request: any) => {
    received.push(request); return { status: 200, body: new Uint8Array() };
  };
  host.transport.prepareHttp = async () => ({ bind: (request: any) => ({ ready: () => true, dispatch: () => {
    sent.push(request.request);
    if (where === 'between group sends' && request.request === 7) wall += 2 * 3_600_000;
    return new Promise(() => {});
  } }) });
  for (let i = 0; i < preparations.length; i++) host.execute({ op: 'managedHttp', phase: 'prepare',
    preparation: preparations[i], intent: 'primary', packet: packet(i + 7) });
  for (let i = 0; i < 30; i++) await Promise.resolve();
  if (where === 'pacing wait') {
    expect(sent).toEqual([]);
    expect(claims).toBe(1);
    expect(timers).toHaveLength(1);
    wall += 2 * 3_600_000;
    timers[0](); // The real managed-HTTP status-2 continuation.
    for (let i = 0; i < 30; i++) await Promise.resolve();
  }
  expect(host.dead).toBe(false);
  expect(failed).toEqual([]);
  expect(sent).toEqual([7, 8]);
  await host.command({ op: 'fetchMarkets' });
  expect(received).toHaveLength(1);
  const query = `timestamp=${origin + 2 * 3_600_000}`;
  expect(received[0].url).toBe(`https://fixture.invalid/signed?${query}&signature=${createHmac('sha256', 'fixture').update(query).digest('hex')}`);
  expect(host.dead).toBe(false);
});
