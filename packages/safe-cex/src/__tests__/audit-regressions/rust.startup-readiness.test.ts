import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions } from '../../types';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { Frame, LOADED_FLAG, SECTION } from '@tealstreet/safe-cex-wasm/wire';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameApplier } from '../../store/frameApplier';
import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';
import { ExchangeWorker } from '../../webworker/ExchangeWorker';
import { setMainAcceptsFrames } from '../../webworker/frameForwarding';

vi.mock('../../utils/virtualClock', () => ({
  virtualClock: { start: vi.fn(async () => undefined), getCurrentTimeMs: () => Date.now() },
}));
const PRIVATE = LOADED_FLAG.MARKETS | LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS | LOADED_FLAG.POSITIONS;
const ALL = PRIVATE | LOADED_FLAG.TICKERS;
const align8 = (n: number) => Math.ceil(n / 8) * 8;

// Independent format-1 encoder: a funded private position and native-loaded
// flags, with no ticker table until the scripted native ticker seed succeeds.
function nativeFrame(loaded: number, includeEth = false): Uint8Array {
  const sections: Array<{ kind: number; bytes: Uint8Array }> = [];
  const table = (kind: number, words: number[], values: number[]) => {
    const bytes = new Uint8Array(align8(words.length * 4) + values.length * 8);
    const view = new DataView(bytes.buffer);
    words.forEach((value, i) => view.setUint32(i * 4, value, true));
    values.forEach((value, i) => view.setFloat64(align8(words.length * 4) + i * 8, value, true));
    sections.push({ kind, bytes });
  };
  const marketValues = [0.001, 0.1, 0.001, 1000, 1, 50, NaN, 1, NaN, NaN];
  table(SECTION.MARKETS, includeEth ? [2, 0, 0, 1, 2, 3, 3, 4, 2, 9, 9] : [1, 0, 0, 1, 2, 9], includeEth ? marketValues.flatMap((value) => [value, value]) : marketValues);
  table(SECTION.POSITIONS, [1, 0, 0x20200], [100, 200, 10, 0, NaN, 2, 50, 20, 1000]);
  table(SECTION.ORDERS, [0], []);
  table(SECTION.BALANCE, [], [20, 980, 1000, 0]);
  if (loaded & LOADED_FLAG.TICKERS)
    table(SECTION.TICKERS, [1, 0], [99, 101, 100, 100, 100, 0, 0, 0, 0, 1, 100]);
  table(SECTION.STATUS, [loaded, 0], [1]);
  const strings = ['BTCUSDT', 'BTC', 'USDT', 'ETHUSDT', 'ETH'].map((s) => new TextEncoder().encode(s));
  const bytes = new Uint8Array(align8(4 + (strings.length + 1) * 4 + strings.reduce((n, s) => n + s.length, 0)));
  const view = new DataView(bytes.buffer);
  view.setUint32(0, strings.length, true);
  let end = 0;
  strings.forEach((s, i) => {
    view.setUint32(4 + i * 4, end, true);
    bytes.set(s, 4 + (strings.length + 1) * 4 + end);
    end += s.length;
  });
  view.setUint32(4 + strings.length * 4, end, true);
  sections.push({ kind: SECTION.STRINGS, bytes });
  const buffer = new ArrayBuffer(16 + sections.reduce((n, s) => n + 8 + s.bytes.length, 0));
  const header = new DataView(buffer);
  header.setUint32(0, 0x46584353, true);
  header.setUint16(4, 1, true);
  header.setUint16(6, sections.length, true);
  let at = 16;
  for (const section of sections) {
    header.setUint16(at, section.kind, true);
    header.setUint32(at + 4, section.bytes.length, true);
    new Uint8Array(buffer, at + 8, section.bytes.length).set(section.bytes);
    at += 8 + section.bytes.length;
  }
  return new Uint8Array(buffer);
}
class ScriptedEngine implements WasmEngineLike {
  static instances: ScriptedEngine[] = [];
  static initial = PRIVATE;
  static startupLog?: 'warning' | 'error';
  pending?: Uint8Array;
  state = ScriptedEngine.initial;
  includeEth = false;
  completeBalanceOnEth = false;
  acceptedSharedSymbols = new Set<string>();
  commands: Array<{ op: string }> = [];
  constructor() { ScriptedEngine.instances.push(this); }
  emit(loaded: number) { this.state = loaded; this.pending = nativeFrame(loaded, this.includeEth); }
  start = () => {
    this.emit(this.state);
    return ScriptedEngine.startupLog ? JSON.stringify([{
      op: 'log', level: ScriptedEngine.startupLog,
      message: ScriptedEngine.startupLog === 'warning' ? 'Public ticker read failed; retry scheduled' : 'Private account credentials rejected',
    }]) : '';
  };
  command = (json: string) => {
    const command = JSON.parse(json);
    this.commands.push(command);
    if (command.op === 'publicFeedUpdate') {
      for (const ticker of command.tickers ?? []) {
        if (ticker.symbol === 'BTCUSDT' || (this.includeEth && ticker.symbol === 'ETHUSDT')) this.acceptedSharedSymbols.add(ticker.symbol);
      }
      if (this.completeBalanceOnEth && this.acceptedSharedSymbols.has('ETHUSDT')) this.emit(PRIVATE);
    }
    // Shared seed intentionally emits no changed frame: the existing explicit
    // startup waiter wake must continue supporting unchanged native snapshots.
    return command.command === undefined ? '' : JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]);
  };
  takeFrame = () => { const pending = this.pending; this.pending = undefined; return pending; };
  snapshotFrame = () => nativeFrame(this.state, this.includeEth);
  wsOpen = () => ''; wsMessage = () => ''; wsClose = () => '';
  httpStart = () => ''; httpChunk = () => ''; httpEnd = () => ''; httpFailed = () => ''; timer = () => '';
}
const cleanups: Array<() => void> = [];
const options = (extra: Partial<ExchangeOptions> = {}): ExchangeOptions => ({
  key: 'scripted', secret: 'scripted', engine: 'rust', transport: directTransport, ...extra,
});
const setup = (extra: Partial<ExchangeOptions> = {}) => {
  const store = new DefaultStore();
  const exchange = new RustEngineExchange('binance', options(extra), store);
  cleanups.push(() => exchange.dispose());
  return { exchange, store };
};
beforeEach(() => {
  vi.useFakeTimers();
  ScriptedEngine.instances.length = 0;
  ScriptedEngine.initial = PRIVATE;
  ScriptedEngine.startupLog = undefined;
  setWasmEngineLoader(async () => ({ WasmEngine: ScriptedEngine }));
  setRustEngineTransport((): Transport => ({
    now: () => Date.now(),
    request: async () => { throw new Error('Unexpected network I/O'); },
    connect: () => { throw new Error('Unexpected socket I/O'); },
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  }));
});
afterEach(() => {
  cleanups.splice(0).reverse().forEach((cleanup) => cleanup());
  setMainAcceptsFrames(false);
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
  vi.useRealTimers();
});
it.each([{}, { privateOnly: true }])('starts a loaded private account despite ticker outage: %j', async (extra) => {
  const { exchange, store } = setup(extra);
  let started = false;
  const startup = exchange.start().then(() => { started = true; }, () => {});
  await vi.advanceTimersByTimeAsync(250);
  expect(store.loaded).toMatchObject({ markets: true, balance: true, orders: true, positions: true, tickers: false });
  expect(store.positions[0]).toMatchObject({ symbol: 'BTCUSDT', contracts: 2, notional: 200 });
  expect(store.balance.total).toBe(1000);
  expect(started).toBe(true);
  await startup;
});
it.each([LOADED_FLAG.BALANCE, LOADED_FLAG.ORDERS, LOADED_FLAG.POSITIONS])('still waits for private flag %i', async (missing) => {
  ScriptedEngine.initial = PRIVATE & ~missing;
  const { exchange } = setup();
  let started = false;
  const startup = exchange.start().then(() => { started = true; }, () => {});
  await vi.advanceTimersByTimeAsync(250);
  expect(started).toBe(false);
  ScriptedEngine.instances[0].emit(PRIVATE);
  await vi.advanceTimersByTimeAsync(250);
  expect(started).toBe(true);
  await startup;
});
it('keeps public-only readiness dependent on the native ticker seed', async () => {
  const { exchange, store } = setup({ publicDataOnly: true });
  let started = false;
  const startup = exchange.start().then(() => { started = true; }, () => {});
  await vi.advanceTimersByTimeAsync(250);
  expect(started).toBe(false);
  ScriptedEngine.instances[0].emit(ALL);
  await vi.advanceTimersByTimeAsync(250);
  expect(started).toBe(true);
  expect(store.tickersBySymbol.BTCUSDT.last).toBe(100);
  await startup;
});
it('validates a loaded native private account during a public ticker outage', async () => {
  const { exchange } = setup();
  let result: unknown;
  const validation = exchange.validateAccount().then((value) => { result = value; });
  await vi.advanceTimersByTimeAsync(250);
  expect(result).toMatchObject({ error: '', balance: { total: 1000 } });
  await validation;
});
it('a retryable public warning does not invalidate the funded native private account', async () => {
  ScriptedEngine.startupLog = 'warning';
  const { exchange } = setup();
  let result: unknown;
  const validation = exchange.validateAccount().then((value) => { result = value; });
  await vi.advanceTimersByTimeAsync(250);
  expect(result).toMatchObject({ error: '', balance: { total: 1000 } });
  expect(exchange.store.positions[0].contracts).toBe(2);
  expect(exchange.store.loaded.tickers).toBe(false);
  await validation;
});
it('private authentication errors still reject account validation', async () => {
  ScriptedEngine.startupLog = 'error';
  const { exchange } = setup();
  const validation = exchange.validateAccount();
  await vi.advanceTimersByTimeAsync(250);
  await expect(validation).resolves.toEqual({ error: expect.stringContaining('Private account credentials rejected') });
});
it('starts the actual worker and keeps the funded native frame usable beyond the old startup deadline', async () => {
  setMainAcceptsFrames(true);
  const frames: ArrayBuffer[] = [];
  const worker = new ExchangeWorker({ postMessage(message: { op: string; data: unknown }, transfer?: Transferable[]) {
    const copied = structuredClone(message, transfer ? { transfer } : undefined);
    if (copied.op === 'frame') frames.push(copied.data as ArrayBuffer);
  } } as unknown as Worker, 'scripted-account', 'binance', options(), {} as never, {} as never, 'scripted-session');
  cleanups.push(() => worker.dispose());
  let started: boolean | undefined;
  const startup = worker.start().then((value) => { started = value; });
  await vi.advanceTimersByTimeAsync(250);
  expect(started).toBe(true);
  expect(worker.store.loaded.tickers).toBe(false);
  const consumer = new DefaultStore();
  const applier = new FrameApplier(consumer);
  frames.forEach((frame) => applier.apply(new Frame(frame)));
  expect(consumer.positions[0]).toMatchObject({ contracts: 2, notional: 200 });
  expect(consumer.balance.total).toBe(1000);
  expect(consumer.loaded.tickers).toBe(false);
  await vi.advanceTimersByTimeAsync(30_000);
  expect(worker.exchange?.isDisposed).toBe(false);
  await startup;
});

it('preserves shared-catalog validation and requires a real matching seed even after native account and local quotes load', async () => {
  ScriptedEngine.initial = ALL;
  const listeners = new Set<(event: { data: unknown }) => void>();
  const send = (data: unknown) => listeners.forEach((listener) => listener({ data }));
  const requests: Array<{ op: string }> = [];
  const port = {
    addEventListener: (name: string, listener: (event: { data: unknown }) => void) => { if (name === 'message') listeners.add(listener); },
    removeEventListener: (_name: string, listener: (event: { data: unknown }) => void) => listeners.delete(listener),
    start() {}, close() {},
    postMessage(message: { op: string }) {
      requests.push(message);
      if (message.op === 'hello') queueMicrotask(() => send({ op: 'ready', markets: [{
        id: 'BTCUSDT', symbol: 'BTCUSDT', contractSize: 1, isSpot: false, isPerpetual: true, isInverse: false, isExpiring: false,
      }] }));
      if (message.op === 'ping') queueMicrotask(() => send({ op: 'pong' }));
    },
  } as unknown as MessagePort;
  const { exchange, store } = setup({ publicFeed: { mode: 'shared', port } });
  let started = false;
  const startup = exchange.start().then(() => { started = true; }, () => {});
  await vi.advanceTimersByTimeAsync(250);
  expect(store.loaded.tickers).toBe(true);
  expect(requests).toContainEqual({ op: 'snapshot' });
  expect(started).toBe(false);
  send({ op: 'publicFeedUpdate', tickers: [{ symbol: 'UNKNOWN', last: 100 }] });
  await vi.advanceTimersByTimeAsync(0);
  expect(started).toBe(false);
  send({ op: 'publicFeedUpdate', tickers: [{ symbol: 'BTCUSDT', last: 100 }] });
  await vi.advanceTimersByTimeAsync(0);
  expect(ScriptedEngine.instances[0].pending).toBeUndefined();
  expect(started).toBe(true);
  await startup;
});

it('requests a fresh shared snapshot only when repaired native metadata gains symbols', async () => {
  ScriptedEngine.initial = PRIVATE & ~LOADED_FLAG.BALANCE;
  const listeners = new Set<(event: { data: unknown }) => void>();
  const send = (data: unknown) => listeners.forEach((listener) => listener({ data }));
  const requests: Array<{ op: string }> = [];
  const market = (symbol: string) => ({ id: symbol, symbol, contractSize: 1, isSpot: false, isPerpetual: true, isInverse: false, isExpiring: false });
  const port = {
    addEventListener: (name: string, listener: (event: { data: unknown }) => void) => { if (name === 'message') listeners.add(listener); },
    removeEventListener: (_name: string, listener: (event: { data: unknown }) => void) => listeners.delete(listener),
    start() {}, close() {},
    postMessage(message: { op: string }) {
      requests.push(message);
      if (message.op === 'hello') queueMicrotask(() => send({ op: 'ready', markets: [market('BTCUSDT'), market('ETHUSDT')] }));
      if (message.op === 'ping') queueMicrotask(() => send({ op: 'pong' }));
      if (message.op === 'snapshot') queueMicrotask(() => send({ op: 'publicFeedUpdate', tickers: [{ symbol: 'BTCUSDT', last: 100 }, { symbol: 'ETHUSDT', last: 20 }] }));
    },
  } as unknown as MessagePort;
  const { exchange, store } = setup({ publicFeed: { mode: 'shared', port } });
  let started = false;
  const startup = exchange.start().then(() => { started = true; });
  await vi.advanceTimersByTimeAsync(250);
  const engine = ScriptedEngine.instances[0];
  expect(engine.acceptedSharedSymbols.has('ETHUSDT')).toBe(false);
  expect(started).toBe(false);
  expect(requests.filter((request) => request.op === 'snapshot')).toHaveLength(1);
  engine.completeBalanceOnEth = true;
  engine.includeEth = true;
  engine.emit(engine.state);
  await vi.advanceTimersByTimeAsync(250);
  expect(requests.filter((request) => request.op === 'snapshot')).toHaveLength(2);
  expect(engine.acceptedSharedSymbols.has('ETHUSDT')).toBe(true);
  expect(store.marketsBySymbol.ETHUSDT).toBeDefined();
  expect(store.loaded.balance).toBe(true);
  expect(started).toBe(true);
  // Provider acceptance emits the same native catalogue; it cannot recurse.
  engine.emit(engine.state);
  await vi.advanceTimersByTimeAsync(250);
  expect(requests.filter((request) => request.op === 'snapshot')).toHaveLength(2);
  await startup;
});
