import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, Market, StoreData } from '../types';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { consumerFrame } from '../__tests__/cutover/consumerFrame';
import { engineCapabilities } from '../__tests__/fixtures/engineCapabilities';
import { setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { virtualClock } from '../utils/virtualClock';
import { LogSeverity } from '../types';
import { ExchangeWorker } from './ExchangeWorker';
import { setTwapLockManager } from './twap/twapLock';

// Selecting a symbol: the page's `callMethod updateSymbolMetadata`, through the
// worker's handler and throttler, to the engine read and the frame back to main.
const market = (symbol: string): Market => ({
  id: symbol,
  symbol,
  base: symbol.replace('USDT', ''),
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
});
const current: Partial<StoreData> = {
  markets: [market('BTCUSDT'), market('LSKUSDT')],
  tickers: [],
  orders: [],
  positions: [],
  balance: { total: 10, free: 10, used: 0, upnl: 0 },
  loaded: { markets: true, tickers: true, orders: true, positions: true, balance: true },
};
let venue: ExchangeName = 'bybit';
const reads: string[] = [];
const sent: string[] = [];
let holdFrames = false;
class ReadEngine implements WasmEngineLike {
  capabilities = () => engineCapabilities(venue);
  start = () => '[]';
  command = (json: string) => {
    const command = JSON.parse(json);
    if (command.op === 'fetchLeverage') reads.push(command.symbol);
    sent.push(command.op);
    const result =
      command.op === 'fetchLeverage' ? JSON.stringify({ buyLeverage: 10, sellLeverage: 10 }) : '[]';
    return command.command === undefined
      ? '[]'
      : JSON.stringify([{ op: 'done', command: command.command, result }]);
  };
  private pending = true;
  takeFrame = () => {
    if (!this.pending || holdFrames) return undefined;
    this.pending = false;
    return new Uint8Array(consumerFrame(current, undefined, true));
  };
  snapshotFrame = () => new Uint8Array(consumerFrame(current));
  wsOpen = () => '[]';
  wsMessage = this.wsOpen;
  wsClose = this.wsOpen;
  wsRouteExhausted = this.wsOpen;
  httpStart = this.wsOpen;
  httpChunk = this.wsOpen;
  httpEnd = this.wsOpen;
  httpFailed = this.wsOpen;
  timer = this.wsOpen;
}
const transport: Transport = {
  now: () => Date.now(),
  request: async () => {
    throw new Error('No fixture HTTP');
  },
  connect: () => {
    throw new Error('No fixture sockets');
  },
  setTimeout: (callback, ms) => setTimeout(callback, ms),
  clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};
const workers: ExchangeWorker[] = [];

const startedWorker = async () => {
  const messages: Array<{ op: string; data?: any }> = [];
  const worker = new ExchangeWorker(
    { postMessage: (message: never) => messages.push(message) } as unknown as Worker,
    'bear5',
    venue,
    { key: 'key', secret: 'secret', passphrase: 'pass', engine: 'rust' } as never,
    {} as never,
    {} as never,
    'fixture',
  );
  workers.push(worker);
  const started = worker.start();
  await vi.advanceTimersByTimeAsync(250);
  await started;
  return { worker, messages };
};

const select = async (worker: ExchangeWorker, symbol: string) => {
  worker.handlePayload({
    op: 'callMethod',
    data: { requestId: `r-${symbol}`, method: 'updateSymbolMetadata', args: [symbol] },
  } as never);
  await vi.advanceTimersByTimeAsync(300);
};

beforeEach(() => {
  reads.length = 0;
  sent.length = 0;
  holdFrames = false;
  setTwapLockManager(null);
  vi.useFakeTimers();
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  setRustEngineTransport(() => transport);
  setWasmEngineLoader(async () => ({ WasmEngine: ReadEngine as never }));
});
afterEach(() => {
  workers.splice(0).forEach((worker) => worker.dispose());
  vi.useRealTimers();
  vi.restoreAllMocks();
});

for (const name of ['bybit', 'bitget'] as ExchangeName[]) {
  it(`${name}: each selected symbol is read and reaches main`, async () => {
    venue = name;
    const { worker, messages } = await startedWorker();
    await select(worker, 'BTCUSDT');
    await select(worker, 'LSKUSDT');
    expect(reads).toEqual(['BTCUSDT', 'LSKUSDT']);
    expect(Object.keys(worker.store.fetchedLeverage)).toEqual(expect.arrayContaining(['BTCUSDT', 'LSKUSDT']));
    const posted = messages.filter((message) => message.data?.fetchedLeverage).at(-1);
    expect(posted?.data.fetchedLeverage.LSKUSDT).toMatchObject({ buyLeverage: 10 });
  });
}

it('warns rather than dropping a selection before the worker engine reports capabilities', async () => {
  venue = 'bybit';
  const { worker } = await startedWorker();
  const exchange = (worker as unknown as { _exchange: { capabilityRecord?: unknown; on: Function } })._exchange;
  exchange.capabilityRecord = undefined;
  const logs = vi.fn();
  exchange.on('log', logs);
  await select(worker, 'LSKUSDT');
  expect(reads).toEqual([]);
  expect(logs).toHaveBeenCalledWith(expect.stringContaining('LSKUSDT'), LogSeverity.Warning);
});

it('a selection that beats the engine markets is read once they load', async () => {
  venue = 'bybit';
  holdFrames = true;
  const messages: Array<{ op: string; data?: any }> = [];
  const worker = new ExchangeWorker(
    { postMessage: (message: never) => messages.push(message) } as unknown as Worker,
    'bear5',
    venue,
    { key: 'key', secret: 'secret', passphrase: 'pass', engine: 'rust' } as never,
    {} as never,
    {} as never,
    'fixture',
  );
  workers.push(worker);
  const started = worker.start();
  await vi.advanceTimersByTimeAsync(50);
  await select(worker, 'LSKUSDT');
  expect(reads).toEqual([]);
  holdFrames = false;
  await vi.advanceTimersByTimeAsync(500);
  await started;
  expect(reads).toEqual(['LSKUSDT']);
  expect(messages.filter((message) => message.data?.fetchedLeverage).at(-1)?.data.fetchedLeverage.LSKUSDT).toMatchObject({
    buyLeverage: 10,
  });
});

it('calls during a cold wasm fetch wait for the engine instead of failing or being lost', async () => {
  venue = 'bybit';
  let loaded!: () => void;
  const wasm = new Promise<void>((resolve) => (loaded = resolve));
  setWasmEngineLoader(async () => {
    await wasm;
    return { WasmEngine: ReadEngine as never };
  });
  const messages: Array<{ op: string; data?: any }> = [];
  const worker = new ExchangeWorker(
    { postMessage: (message: never) => messages.push(message) } as unknown as Worker,
    'bear5',
    venue,
    { key: 'key', secret: 'secret', passphrase: 'pass', engine: 'rust' } as never,
    {} as never,
    {} as never,
    'fixture',
  );
  workers.push(worker);
  const started = worker.start();
  await vi.advanceTimersByTimeAsync(50);
  await select(worker, 'LSKUSDT');
  worker.handlePayload({
    op: 'callMethod',
    data: { requestId: 'candles', method: 'fetchOHLCV', args: [{ symbol: 'LSKUSDT', interval: '1h', limit: 1 }] },
  } as never);
  await vi.advanceTimersByTimeAsync(50);
  expect(reads).toEqual([]);
  loaded();
  await vi.advanceTimersByTimeAsync(500);
  await started;
  expect(reads).toEqual(['LSKUSDT']);
  const candles = messages.find((message) => message.op === 'callMethod:done' && message.data.requestId === 'candles');
  expect(candles?.data.success).toBe(true);
});

it('writes during a cold wasm fetch fail at once and are never sent; placeOrders queues', async () => {
  venue = 'bybit';
  let loaded!: () => void;
  const wasm = new Promise<void>((resolve) => (loaded = resolve));
  setWasmEngineLoader(async () => {
    await wasm;
    return { WasmEngine: ReadEngine as never };
  });
  const messages: Array<{ op: string; data?: any }> = [];
  const worker = new ExchangeWorker(
    { postMessage: (message: never) => messages.push(message) } as unknown as Worker,
    'bear5',
    venue,
    { key: 'key', secret: 'secret', passphrase: 'pass', engine: 'rust' } as never,
    {} as never,
    {} as never,
    'fixture',
  );
  workers.push(worker);
  const started = worker.start();
  await vi.advanceTimersByTimeAsync(50);
  const order = { symbol: 'BTCUSDT', side: 'buy', type: 'limit', amount: 1, price: 1 };
  const writes: Array<[string, unknown[]]> = [
    ['setLeverage', ['BTCUSDT', 5]],
    ['cancelOrders', [[{ id: 'a', symbol: 'BTCUSDT' }]]],
    ['cancelAllOrders', []],
    ['changeMarginType', [true, 'BTCUSDT']],
    ['changePositionMode', [true]],
    ['updateOrder', [{ order: { id: 'a', symbol: 'BTCUSDT' }, update: { price: 2 } }]],
    ['setAllLeverage', [5]],
    ['scheduleCancel', [null]],
    ['changeStpMode', ['EXPIRE_TAKER']],
  ];
  for (const [method, args] of writes) {
    worker.handlePayload({ op: 'callMethod', data: { requestId: method, method, args } } as never);
  }
  worker.handlePayload({ op: 'callMethod', data: { requestId: 'place', method: 'placeOrders', args: [[order]] } } as never);
  await vi.advanceTimersByTimeAsync(50);
  for (const [method] of writes) {
    const reply = messages.find((message) => message.op === 'callMethod:done' && message.data.requestId === method);
    expect(reply?.data.success, method).toBe(false);
    expect(reply?.data.result.message, method).toContain('not started');
  }
  expect(messages.some((message) => message.data?.requestId === 'place')).toBe(false);
  loaded();
  await vi.advanceTimersByTimeAsync(500);
  await started;
  await vi.advanceTimersByTimeAsync(500);
  for (const op of ['setLeverage', 'cancelOrders', 'setMarginType', 'setPositionMode', 'scheduleCancel', 'setStpMode']) expect(sent).not.toContain(op);
  expect(sent).toContain('placeOrders');
});

const loadingWorker = async () => {
  venue = 'bybit';
  holdFrames = true;
  const messages: Array<{ op: string; data?: any }> = [];
  const worker = new ExchangeWorker(
    { postMessage: (message: never) => messages.push(message) } as unknown as Worker,
    'bear5',
    venue,
    { key: 'key', secret: 'secret', passphrase: 'pass', engine: 'rust' } as never,
    {} as never,
    {} as never,
    'fixture',
  );
  workers.push(worker);
  void worker.start();
  await vi.advanceTimersByTimeAsync(50);
  return { worker, messages };
};
const order = { symbol: 'BTCUSDT', side: 'buy', type: 'limit', amount: 1, price: 1 };
const place = (worker: ExchangeWorker) =>
  worker.handlePayload({ op: 'callMethod', data: { requestId: 'place', method: 'placeOrders', args: [[order]] } } as never);
const reply = (messages: Array<{ op: string; data?: any }>, id: string) =>
  messages.find((message) => message.op === 'callMethod:done' && message.data.requestId === id);

it('a placement queued past the bound is refused, shown, and never sent even if the account then loads', async () => {
  const { worker, messages } = await loadingWorker();
  place(worker);
  await vi.advanceTimersByTimeAsync(29_999);
  expect(reply(messages, 'place')).toBeUndefined();
  await vi.advanceTimersByTimeAsync(1);
  expect(reply(messages, 'place')?.data).toMatchObject({ success: false, result: { outcome: 'notSent' } });
  expect(reply(messages, 'place')?.data.result.message).toContain('was not sent');
  const shown = messages.filter((message) => message.op === 'error' && String(message.data).includes('was not sent'));
  expect(shown).toHaveLength(1);
  holdFrames = false;
  await vi.advanceTimersByTimeAsync(60_000);
  expect(sent).not.toContain('placeOrders');
  expect(messages.filter((message) => message.op === 'callMethod:done' && message.data.requestId === 'place')).toHaveLength(1);
});

it('a placement whose account loads just inside the bound is sent once and never refused', async () => {
  const { worker, messages } = await loadingWorker();
  place(worker);
  await vi.advanceTimersByTimeAsync(29_000);
  holdFrames = false;
  await vi.advanceTimersByTimeAsync(990);
  await vi.advanceTimersByTimeAsync(60_000);
  expect(sent.filter((op) => op === 'placeOrders')).toHaveLength(1);
  expect(reply(messages, 'place')?.data.success).toBe(true);
  expect(messages.filter((message) => message.op === 'error')).toHaveLength(0);
});

it('other queued writes take the same bound once the engine exists', async () => {
  const { worker, messages } = await loadingWorker();
  for (const [method, args] of [
    ['cancelAllOrders', []],
    ['scheduleCancel', [null]],
    ['changeStpMode', ['EXPIRE_TAKER']],
  ] as const) {
    worker.handlePayload({ op: 'callMethod', data: { requestId: method, method, args } } as never);
  }
  await vi.advanceTimersByTimeAsync(29_999);
  expect(reply(messages, 'cancelAllOrders')).toBeUndefined();
  await vi.advanceTimersByTimeAsync(1);
  for (const method of ['cancelAllOrders', 'scheduleCancel', 'changeStpMode']) {
    expect(reply(messages, method)?.data.result, method).toMatchObject({ outcome: 'notSent' });
  }
  holdFrames = false;
  await vi.advanceTimersByTimeAsync(60_000);
  expect(sent).not.toContain('cancelSymbolOrders');
  expect(sent).not.toContain('scheduleCancel');
  expect(sent).not.toContain('setStpMode');
});
