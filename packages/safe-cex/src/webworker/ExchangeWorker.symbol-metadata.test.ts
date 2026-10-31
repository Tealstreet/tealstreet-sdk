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
class ReadEngine implements WasmEngineLike {
  capabilities = () => engineCapabilities(venue);
  start = () => '[]';
  command = (json: string) => {
    const command = JSON.parse(json);
    if (command.op === 'fetchLeverage') reads.push(command.symbol);
    const result =
      command.op === 'fetchLeverage' ? JSON.stringify({ buyLeverage: 10, sellLeverage: 10 }) : '[]';
    return command.command === undefined
      ? '[]'
      : JSON.stringify([{ op: 'done', command: command.command, result }]);
  };
  private pending = true;
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
