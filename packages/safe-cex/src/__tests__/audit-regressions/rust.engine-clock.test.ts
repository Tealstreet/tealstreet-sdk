vi.mock('../../exchanges/bybit/bybit.types', async (importOriginal) => {
  const { configuredHyrotraderEndpoints } = await import('../fixtures/configuredHyrotraderEndpoints');
  return {
    ...(await importOriginal<typeof import('../../exchanges/bybit/bybit.types')>()),
    ...configuredHyrotraderEndpoints,
  };
});

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import type { HttpResponse, Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName } from '../../types';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { afterAll, afterEach, beforeEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RUST_VENUES, RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { virtualClock } from '../../utils/virtualClock';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

vi.mock('@tealstreet/safe-cex-wasm/host', async (original) => ({
  ...await original<typeof import('@tealstreet/safe-cex-wasm/host')>(), nativeTransport: vi.fn(),
}));
vi.mock('../../utils/runtime', async (original) => ({
  ...await original<typeof import('../../utils/runtime')>(), isNodeOrBunRuntime: () => false,
}));

const EPOCH = 1_760_000_000_000;
const running: RustEngineExchange[] = [];
let monotonic = 0;
beforeEach(() => {
  monotonic = 0;
  (virtualClock as any).lastAfterClockStepSync = null;
  vi.spyOn(performance, 'now').mockImplementation(() => monotonic);
  virtualClock.setServerTimeSource(async () => EPOCH);
});
afterEach(() => {
  running.splice(0).forEach((exchange) => exchange.dispose());
  virtualClock.stop();
  virtualClock.setServerTimeSource(undefined);
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
afterAll(() => {
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
});

function fixture(name: ExchangeName, now: () => number, legacyClock?: boolean, http = false, privateAccount = false) {
  const seen: Array<[string, number]> = [];
  const commands: Array<Record<string, unknown>> = [];
  let socket!: Parameters<Transport['connect']>[1];
  let completeHttp: ((response: HttpResponse) => void) | undefined;
  const timers: Array<{ fn: () => void; ms: number }> = [];
  const record = (event: string, time: number) => { seen.push([event, time]); return '[]'; };
  const engine: WasmEngineLike = {
    capabilities: () => JSON.stringify({ ...JSON.parse(engineCapabilities(name)), ...(legacyClock === undefined ? {} : { hostClock: legacyClock }) }),
    start: (time) => {
      record('start', time);
      return JSON.stringify([{ op: 'wsConnect', socket: 1, url: 'wss://clock.invalid/private', channel: 'private' },
        { op: 'setTimer', timer: 7, at: time + 1500 },
        ...(http ? [{ op: 'http', request: 3, method: 'GET', url: 'https://clock.invalid/probe', route: 'direct', headers: [] }] : [])]);
    },
    command: (json, time) => {
      const command = JSON.parse(json);
      commands.push(command);
      record('command', time);
      return command.command === undefined ? '[]' : JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]);
    },
    wsOpen: (_id, time) => record('open', time),
    wsMessage: (_id, _text, time) => record('message', time),
    wsRouteExhausted: () => '',
    wsClose: (_id, _code, time) => record('close', time),
    httpStart: (_id, _status, _headers, time) => record('httpStart', time),
    httpChunk: (_id, _data, time) => record('httpChunk', time),
    httpEnd: (_id, time) => record('httpEnd', time),
    httpFailed: (_id, _error, time) => record('httpFailed', time),
    timer: (_id, time) => record('timer', time),
    takeFrame: () => undefined,
    snapshotFrame: () => new Uint8Array(),
  };
  const transport: Transport = {
    now,
    connect: (_url, callbacks) => { socket = callbacks; return { send: () => {}, close: () => {} }; },
    request: () => new Promise((done) => { completeHttp = done; }),
    setTimeout: (fn, ms) => { const timer = { fn, ms }; timers.push(timer); return timer; },
    clearTimeout: () => {},
  };
  vi.mocked(nativeTransport).mockImplementation(() => transport);
  const constructed = vi.fn(function () { return engine; });
  setWasmEngineLoader(async () => ({ WasmEngine: constructed as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  installNativeStartupFrames(engine, store);
  const frame = engine.takeFrame!;
  engine.takeFrame = (time, marketOnly) => { record('frame', time); return frame(time, marketOnly); };
  const exchange = new RustEngineExchange(name, { key: privateAccount ? 'replay-key' : '', secret: privateAccount ? 'replay-secret' : '', publicDataOnly: !privateAccount }, store);
  running.push(exchange);
  return { exchange, engine, constructed, transport, seen, commands, timers, socket: () => socket, hasHttp: () => Boolean(completeHttp),
    completeHttp: (response: HttpResponse) => completeHttp!(response) };
}

it.each([...RUST_VENUES])('keeps the production scheduling clock monotonic for %s after synchronization', async (name) => {
  let synced = EPOCH;
  vi.spyOn(Date, 'now').mockReturnValue(EPOCH + 600_000);
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => synced);
  const f = fixture(name, () => EPOCH + 900_000);
  await f.exchange.start();
  expect(f.seen.find(([event]) => event === 'start')).toEqual(['start', EPOCH]);
  expect(f.timers.some(({ ms }) => ms === 1500)).toBe(true);
  synced += 2345;
  monotonic += 2345;
  await f.exchange.setLeverage('BTCUSDT', 2);
  expect(f.seen.filter(([event]) => event === 'command').at(-1)).toEqual(['command', synced]);
  expect(f.seen.filter(([event]) => event === 'frame').at(-1)).toEqual(['frame', synced]);
  f.socket().onOpen();
  f.socket().onMessage('native-auth-frame');
  expect(f.seen.filter(([event]) => ['open', 'message'].includes(event))).toEqual([['open', synced], ['message', synced]]);
  synced -= 3456;
  f.timers.find(({ ms }) => ms === 1500)!.fn();
  f.socket().onClose(1006);
  expect(f.seen.find(([event]) => event === 'timer')).toEqual(['timer', EPOCH + monotonic]);
  expect(f.seen.find(([event]) => event === 'close')).toEqual(['close', EPOCH + monotonic]);
});

it('uses the stable scheduling clock for every streamed HTTP completion callback', async () => {
  let synced = EPOCH;
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockImplementation(() => synced);
  const f = fixture('mexc', () => EPOCH + 600_000, undefined, true);
  await f.exchange.start();
  await vi.waitFor(() => expect(f.hasHttp()).toBe(true));
  synced += 1234;
  monotonic += 1234;
  f.completeHttp({ status: 200, headers: '', body: new TextEncoder().encode('{}') });
  await vi.waitFor(() => expect(f.seen).toContainEqual(['httpEnd', synced]));
  expect(f.seen.filter(([event]) => event.startsWith('http'))).toEqual([
    ['httpStart', synced], ['httpChunk', synced], ['httpEnd', synced],
  ]);
});

it('holds simultaneous default engines behind the same initial clock synchronization', async () => {
  let resolve!: (time: number) => void;
  const source = vi.fn(() => new Promise<number>((done) => { resolve = done; }));
  virtualClock.setServerTimeSource(source);
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(EPOCH);
  const first = fixture('mexc', () => EPOCH + 600_000);
  const starts = vi.spyOn(virtualClock, 'start');
  const a = first.exchange.start();
  await vi.waitFor(() => expect(source).toHaveBeenCalledOnce());
  const second = fixture('phemex', () => EPOCH + 600_000);
  const b = second.exchange.start();
  try {
    await vi.waitFor(() => expect(starts).toHaveBeenCalledTimes(2));
    await new Promise((done) => setTimeout(done, 0));
    expect(first.seen.some(([event]) => event === 'start')).toBe(false);
    expect(second.seen.some(([event]) => event === 'start')).toBe(false);
  } finally {
    resolve(EPOCH);
    await Promise.all([a, b]);
  }
  expect(source).toHaveBeenCalledOnce();
  expect(first.seen).toContainEqual(['start', EPOCH]);
  expect(second.seen).toContainEqual(['start', EPOCH]);
});

it('settles a disposed account while other accounts keep waiting for shared clock initialization', async () => {
  let resolve!: (time: number) => void;
  const source = vi.fn(() => new Promise<number>((done) => { resolve = done; }));
  virtualClock.setServerTimeSource(source);
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(EPOCH);
  const first = fixture('mexc', () => EPOCH + 600_000);
  let outcome = 'pending';
  const a = first.exchange.start().then(() => { outcome = 'started'; }, () => { outcome = 'disposed'; });
  await vi.waitFor(() => expect(source).toHaveBeenCalledOnce());
  const second = fixture('phemex', () => EPOCH + 600_000);
  const b = second.exchange.start();
  try {
    first.exchange.dispose();
    await vi.waitFor(() => expect(outcome).toBe('disposed'));
    expect(first.constructed).not.toHaveBeenCalled();
    expect(second.constructed).not.toHaveBeenCalled();
    expect(second.seen.some(([event]) => event === 'start')).toBe(false);
  } finally {
    resolve(EPOCH);
    await Promise.all([a, b]);
  }
  expect(first.constructed).not.toHaveBeenCalled();
  expect(second.seen).toContainEqual(['start', EPOCH]);
  expect(source).toHaveBeenCalledOnce();
});

it.each([true, false])('preserves an explicit tape clock despite a legacy hostClock=%s field', async (legacy) => {
  let tape = EPOCH + 100;
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(EPOCH + 900_000);
  const f = fixture('bitunix', () => tape, legacy);
  setRustEngineTransport(() => f.transport);
  await f.exchange.start();
  expect(f.seen).toContainEqual(['start', tape]);
  tape -= 500;
  f.socket().onOpen();
  expect(f.seen).toContainEqual(['open', tape]);
});

it('validates a local schedule deadline against its explicit engine clock', async () => {
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(EPOCH + 900_000);
  const f = fixture('hyperliquid', () => EPOCH);
  setRustEngineTransport(() => f.transport);
  await expect(f.exchange.scheduleCancel(NaN)).rejects.toThrow('safe integer');
  expect(f.constructed).not.toHaveBeenCalled();
  await f.exchange.start();
  await expect(f.exchange.scheduleCancel(EPOCH + 5000)).resolves.toBeUndefined();
  expect(f.commands).toContainEqual(expect.objectContaining({ op: 'scheduleCancel', time: EPOCH + 5000 }));
});

it.each(['woo', 'bybit'] as const)('defaults %s journal bounds to its explicit engine clock', async (name) => {
  vi.spyOn(Date, 'now').mockReturnValue(EPOCH + 600_000);
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(EPOCH + 900_000);
  const f = fixture(name, () => EPOCH, undefined, false, true);
  setRustEngineTransport(() => f.transport);
  await f.exchange.start();
  await f.exchange.fetchHistoricalPositions({ account: 'clock-audit' });
  const queries = f.commands.filter((command) => ['fetchOrderHistory', 'fetchClosedPositions'].includes(String(command.op)));
  expect(queries.length).toBeGreaterThan(0);
  expect(queries.every((command) => command.to === EPOCH)).toBe(true);
});

it('restores the synchronized production clock after an explicit host override', async () => {
  setRustEngineTransport();
  vi.spyOn(virtualClock, 'getCurrentTimeMs').mockReturnValue(EPOCH);
  const f = fixture('mexc', () => EPOCH + 900_000);
  await f.exchange.start();
  expect(f.seen).toContainEqual(['start', EPOCH]);
});

it.each([
  { jump: 600_000, worker: false },
  { jump: -600_000, worker: false },
  { jump: 600_000, worker: true },
  { jump: -600_000, worker: true },
])('refreshes a stale offset after a $jump ms wall step without suspend (worker=$worker)', async ({ jump, worker }) => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(EPOCH - jump);
  let finishSync!: (utc: number) => void;
  const source = vi.fn()
    .mockResolvedValueOnce(EPOCH)
    .mockImplementation(() => new Promise<number>((resolve) => { finishSync = resolve; }));
  virtualClock.setServerTimeSource(source);
  setRustEngineTransport();
  const f = fixture('binance', () => EPOCH);
  let protocolRaw = EPOCH;
  let protocolUtc = EPOCH;
  const syncHostTime = vi.fn((now: number, utc: number) => { protocolRaw = now; protocolUtc = utc; return true; });
  f.engine.syncHostTime = syncHostTime;
  // Model the signing clock at the engine boundary; real timestamp/HMAC
  // coverage lives in the native engine's clock regressions.
  const signingTimes: number[] = [];
  const command = f.engine.command;
  f.engine.command = (json, now) => {
    if (JSON.parse(json).command !== undefined) signingTimes.push(protocolUtc + now - protocolRaw);
    return command(json, now);
  };
  await f.exchange.start();
  expect(virtualClock.getCurrentTimeMs()).toBe(EPOCH);
  expect(source).toHaveBeenCalledOnce();
  const transport: Transport = (f.exchange as any).host.transport;
  // Initial time is already supplied by the page; exercise the worker's
  // exceptional resync path without introducing routine worker polling.
  if (worker) vi.stubGlobal('window', undefined);

  monotonic = 12_000;
  vi.setSystemTime(EPOCH - jump + monotonic);
  await f.exchange.setLeverage('BTCUSDT', 2);
  expect(source).toHaveBeenCalledOnce(); // Ordinary elapsed time needs no sync.

  vi.setSystemTime(EPOCH + monotonic); // The user fixes their wall clock.
  const syncCount = syncHostTime.mock.calls.length;
  expect(transport.now()).toBe(EPOCH + monotonic);
  expect(source).toHaveBeenCalledOnce();
  expect(syncHostTime).toHaveBeenCalledTimes(syncCount); // Clock reads cannot reenter Rust.
  await f.exchange.setLeverage('BTCUSDT', 2);
  expect(source).toHaveBeenCalledTimes(2);
  expect(virtualClock.getCurrentTimeMs()).toBe(EPOCH + monotonic + jump);
  const sharedSync = virtualClock.syncClientServerTime({ afterClockStep: true });
  expect(source).toHaveBeenCalledTimes(2); // An outstanding sync is shared.

  finishSync(EPOCH + monotonic);
  await sharedSync;
  expect(virtualClock.getCurrentTimeMs()).toBe(EPOCH + monotonic);
  await f.exchange.setLeverage('BTCUSDT', 2);
  expect(signingTimes.at(-1)).toBe(EPOCH + monotonic);
  expect(transport.now()).toBe(EPOCH + monotonic);
  expect(f.seen.filter(([event]) => event === 'command').at(-1)).toEqual(['command', EPOCH + monotonic]);
});

it.each([
  { jump: 600_000, worker: false },
  { jump: -600_000, worker: false },
  { jump: 600_000, worker: true },
  { jump: -600_000, worker: true },
])('recovers a second $jump ms wall step at the cooldown boundary (worker=$worker)', async ({ jump, worker }) => {
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] });
  vi.setSystemTime(EPOCH - 600_000);
  const source = vi.fn(async () => EPOCH + monotonic);
  virtualClock.setServerTimeSource(source);
  setRustEngineTransport();
  const f = fixture('binance', () => EPOCH);
  let protocolRaw = EPOCH;
  let protocolUtc = EPOCH;
  f.engine.syncHostTime = (now, utc) => { protocolRaw = now; protocolUtc = utc; return true; };
  const signingTimes: number[] = [];
  const command = f.engine.command;
  f.engine.command = (json, now) => {
    if (JSON.parse(json).command !== undefined) signingTimes.push(protocolUtc + now - protocolRaw);
    return command(json, now);
  };
  await f.exchange.start();
  const transport: Transport = (f.exchange as any).host.transport;
  if (worker) vi.stubGlobal('window', undefined);

  vi.setSystemTime(EPOCH);
  transport.now();
  await Promise.resolve();
  await virtualClock.syncClientServerTime({ afterClockStep: true });
  expect(source).toHaveBeenCalledTimes(2);
  expect(virtualClock.getCurrentTimeMs()).toBe(EPOCH);

  monotonic = 1_000;
  await vi.advanceTimersByTimeAsync(1_000);
  vi.setSystemTime(EPOCH + monotonic + jump);
  expect(transport.now()).toBe(EPOCH + monotonic);
  await f.exchange.setLeverage('BTCUSDT', 2);
  expect(source).toHaveBeenCalledTimes(2);
  expect(virtualClock.getCurrentTimeMs()).toBe(EPOCH + monotonic + jump);

  monotonic = 14_999;
  await vi.advanceTimersByTimeAsync(13_999);
  expect(source).toHaveBeenCalledTimes(2);
  monotonic = 15_000;
  await vi.advanceTimersByTimeAsync(1);
  expect(source).toHaveBeenCalledTimes(3);
  expect(virtualClock.getCurrentTimeMs()).toBe(EPOCH + monotonic);
  await f.exchange.setLeverage('BTCUSDT', 2);
  expect(signingTimes.at(-1)).toBe(EPOCH + monotonic);
  expect(transport.now()).toBe(EPOCH + monotonic);
});

it('shares a completed wall-step resync across two default transports', async () => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(EPOCH - 600_000);
  const source = vi.fn(async () => EPOCH + monotonic);
  virtualClock.setServerTimeSource(source);
  setRustEngineTransport();
  const first = fixture('mexc', () => EPOCH);
  await first.exchange.start();
  const second = fixture('phemex', () => EPOCH);
  await second.exchange.start();
  expect(source).toHaveBeenCalledOnce();
  source.mockClear();

  vi.setSystemTime(EPOCH);
  const firstTransport: Transport = (first.exchange as any).host.transport;
  const secondTransport: Transport = (second.exchange as any).host.transport;
  firstTransport.now();
  await Promise.resolve();
  await virtualClock.syncClientServerTime({ afterClockStep: true });
  expect(source).toHaveBeenCalledOnce();

  // The second engine observes the same wall change after the first sync settled.
  secondTransport.now();
  await Promise.resolve();
  await virtualClock.syncClientServerTime({ afterClockStep: true });
  expect(source).toHaveBeenCalledOnce();
  expect(virtualClock.getCurrentTimeMs()).toBe(EPOCH);
});
