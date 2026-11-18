import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { Market, StoreData } from '../types';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { consumerFrame } from '../__tests__/cutover/consumerFrame';
import { engineCapabilities } from '../__tests__/fixtures/engineCapabilities';
import { setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { virtualClock } from '../utils/virtualClock';
import { ExchangeWorker } from './ExchangeWorker';
import { setMainAcceptsFrames } from './frameForwarding';
import { setTwapLockManager } from './twap/twapLock';

// A frame-fed page engine ingests account sections only: a worker forwarding
// its frames must keep posting the books and candles its subscriptions asked for.
const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};
const current: Partial<StoreData> = {
  markets: [market],
  tickers: [],
  orders: [],
  positions: [],
  balance: { total: 10, free: 10, used: 0, upnl: 0 },
  loaded: { markets: true, tickers: true, orders: true, positions: true, balance: true },
};
let engine: BookEngine;
class BookEngine implements WasmEngineLike {
  pending = true;
  constructor() {
    engine = this;
  }
  capabilities = () => engineCapabilities('binance');
  start = () => '[]';
  command = (json: string) => {
    const command = JSON.parse(json);
    return command.command === undefined
      ? '[]'
      : JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]);
  };
  // Every flush carries a BTCUSDT book (and a candle and a fill) beside the account.
  takeFrame = () => {
    if (!this.pending) return undefined;
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
  const messages: Array<{ op: string; data?: unknown; snapshot?: boolean }> = [];
  const worker = new ExchangeWorker(
    { postMessage: (message: never) => messages.push(message) } as unknown as Worker,
    'bear5',
    'binance',
    { key: '', secret: '', engine: 'rust', publicDataOnly: true },
    {} as never,
    {} as never,
    'fixture',
  );
  workers.push(worker);
  const started = worker.start();
  await vi.advanceTimersByTimeAsync(250);
  await started;
  worker.handlePayload({ op: 'subscribeOrderBook', data: { symbol: 'BTCUSDT', subId: 'book-1' } } as never);
  return { worker, messages };
};

const flush = async () => {
  engine.pending = true;
  await vi.advanceTimersByTimeAsync(300);
};

beforeEach(() => {
  setTwapLockManager(null);
  vi.useFakeTimers();
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  setRustEngineTransport(() => transport);
  setWasmEngineLoader(async () => ({ WasmEngine: BookEngine as never }));
});
afterEach(() => {
  workers.splice(0).forEach((worker) => worker.dispose());
  setMainAcceptsFrames(false);
  vi.useRealTimers();
  vi.restoreAllMocks();
});

it('keeps posting listenOrderBook after forwardFrames hands its frames to a page engine', async () => {
  setMainAcceptsFrames(false);
  const { worker, messages } = await startedWorker();
  worker.handlePayload({ op: 'forwardFrames' } as never);
  messages.length = 0;
  await flush();
  expect(messages.some((message) => message.op === 'frame')).toBe(true);
  expect(messages.some((message) => message.op === 'listenOrderBook')).toBe(true);
});

it('stays quiet on books only where main decodes them from the frames itself', async () => {
  setMainAcceptsFrames(true);
  const { messages } = await startedWorker();
  messages.length = 0;
  await flush();
  expect(messages.some((message) => message.op === 'frame')).toBe(true);
  expect(messages.some((message) => message.op === 'listenOrderBook')).toBe(false);
});
