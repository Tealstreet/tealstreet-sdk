import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { binanceVenue } from '../../../../sdk-rs/hosts/wasm/js/binance-venue-fixture.mjs';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { virtualClock } from '../utils/virtualClock';
import { chaserOrderManagerInstance } from './chaserOrder/ChaserOrderManager';
import { ExchangeWorker } from './ExchangeWorker';
import { setTwapLockManager } from './twap/twapLock';
import { twapManager } from './twap/TwapManager';

// The local worker on a real `algos` build of the Binance engine (built with
// `bash sdk-rs/harness/build.sh --venue binance`): chaser and TWAP ops run in
// the engine, and their replies reach main under the TS handlers' op names.
const engine = await loadNodeEngine().catch(() => undefined);
const algos = typeof (engine?.WasmEngine.prototype as { algoStart?: unknown } | undefined)?.algoStart === 'function';

type Message = { op: string; data?: any };
type Venue = ReturnType<typeof binanceVenue>;
const workers: ExchangeWorker[] = [];

const startWorker = async () => {
  const venue: Venue = binanceVenue({ clock: 'global' });
  setRustEngineTransport(() => venue.transport);
  const messages: Message[] = [];
  const worker = new ExchangeWorker(
    { postMessage: (message: Message) => messages.push(message) } as unknown as Worker,
    'acct',
    'binance',
    { key: 'fixture-key', secret: 'fixture-secret', engine: 'rust' },
    {} as never,
    {} as never,
    'fixture',
  );
  workers.push(worker);
  const started = worker.start();
  await vi.advanceTimersByTimeAsync(1_000);
  expect(await started).toBe(true);
  expect((worker.exchange as RustEngineExchange).runsRustAlgos).toBe(true);
  return { worker, venue, messages };
};
const lastStore = (messages: Message[], field: 'activeChasers' | 'activeTWAPs') =>
  messages.filter((message) => message.op === 'store' && message.data?.[field]).at(-1)?.data[field];

beforeEach(() => {
  setTwapLockManager(null);
  vi.useFakeTimers();
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  setWasmEngineLoader(loadNodeEngine);
  vi.spyOn(chaserOrderManagerInstance, 'add');
  vi.spyOn(twapManager, 'add');
});
afterEach(() => {
  workers.splice(0).forEach((worker) => worker.dispose());
  setRustEngineTransport();
  vi.useRealTimers();
});

it.skipIf(!algos)('a local chase runs in the engine and posts chaser:complete when filled', async () => {
  const { worker, venue, messages } = await startWorker();
  worker.handlePayload({
    op: 'chaser:start',
    data: { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10, clientId: 'chase-1' },
  });
  await vi.advanceTimersByTimeAsync(300);
  expect(venue.placements).toHaveLength(1);
  expect(venue.placements[0]).toMatchObject({ type: 'LIMIT', side: 'BUY', price: '95999.9', origQty: '0.002' });
  expect(lastStore(messages, 'activeChasers')).toEqual([
    expect.objectContaining({ symbol: 'BTCUSDT', currentOrderIds: [String(venue.placements[0].orderId)] }),
  ]);

  venue.fill(venue.placements[0]);
  await vi.advanceTimersByTimeAsync(1_500);
  expect(messages).toContainEqual({
    op: 'chaser:complete',
    data: [expect.objectContaining({ clientId: 'chase-1', symbol: 'BTCUSDT', filled: 0.002, stopReason: 'Completed' })],
  });
  expect(lastStore(messages, 'activeChasers')).toEqual([]);
  expect(chaserOrderManagerInstance.add).not.toHaveBeenCalled();
  expect(chaserOrderManagerInstance.instances).toHaveLength(0);
});

it.skipIf(!algos)('a two-slice TWAP places on the host timer, persists its rows and completes', async () => {
  const { worker, venue, messages } = await startWorker();
  worker.handlePayload({
    op: 'twap:start',
    data: {
      accountName: 'acct',
      activeMarket: { symbol: 'BTCUSDT' },
      size: 0.004,
      duration: 1,
      ordersCount: 2,
      side: 'buy',
      chaserDelay: 1000,
      reduceOnly: false,
      randomnessPercentage: 0,
      useLimitOrders: false,
      useChaserOrders: false,
    },
  });
  await vi.advanceTimersByTimeAsync(300);
  expect(venue.placements.map((order: { type: string }) => order.type)).toEqual(['MARKET']);
  const persisted = messages.filter((message) => message.op === 'twap:stateUpdate').at(-1);
  // Posted as the TS TwapInstance posts it: the persisted map itself, not listener args.
  expect(Object.values(persisted!.data)).toEqual([
    expect.objectContaining({ options: expect.objectContaining({ accountName: 'acct' }) }),
  ]);
  expect(lastStore(messages, 'activeTWAPs')).toEqual([expect.objectContaining({ symbol: 'BTCUSDT', size: 0.004 })]);

  await vi.advanceTimersByTimeAsync(29_000);
  expect(venue.placements).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(2_000);
  expect(venue.placements.map((order: { origQty: string }) => order.origQty)).toEqual(['0.002', '0.002']);
  await vi.advanceTimersByTimeAsync(1_000);
  expect(messages.filter((message) => message.op === 'twap:stateUpdate').at(-1)?.data).toEqual({});
  expect(lastStore(messages, 'activeTWAPs')).toEqual([]);
  expect(twapManager.add).not.toHaveBeenCalled();
  expect(twapManager.runningTwaps.size).toBe(0);
});

it.skipIf(!algos)('disposing the worker cancels a resting chase before the engine goes', async () => {
  const { worker, venue } = await startWorker();
  worker.handlePayload({ op: 'chaser:start', data: { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10 } });
  await vi.advanceTimersByTimeAsync(300);
  expect(venue.placements).toHaveLength(1);
  worker.dispose();
  await vi.advanceTimersByTimeAsync(1_000);
  expect(venue.cancels).toEqual([String(venue.placements[0].orderId)]);
});
