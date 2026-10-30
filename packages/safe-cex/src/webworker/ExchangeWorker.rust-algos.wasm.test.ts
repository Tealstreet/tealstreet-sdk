import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { binanceVenue } from '../../../../sdk-rs/hosts/wasm/js/binance-venue-fixture.mjs';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { virtualClock } from '../utils/virtualClock';
import { ExchangeWorker } from './ExchangeWorker';
import { setTwapLockManager, type TwapLockManager } from './twap/twapLock';

// The local worker on a real `algos` build of the Binance engine (built with
// `bash sdk-rs/harness/build.sh --venue binance`): chaser and TWAP ops run in
// the engine, and their replies reach main under the worker's algo op names.
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
  // Posted as the persisted map itself, not listener args.
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

const TWAP_OPTIONS = {
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
};
/** Web Locks one origin's tabs share. */
const heldLocks = new Map<string, string>();
const locksFor = (tab: string): TwapLockManager => ({
  async request(name, _options, callback) {
    if (heldLocks.has(name)) return callback(null);
    heldLocks.set(name, tab);
    try {
      await callback({ name });
    } finally {
      if (heldLocks.get(name) === tab) heldLocks.delete(name);
    }
  },
});
const errorsIn = (messages: Message[]) =>
  messages.filter((message) => message.op === 'error').map((message) => JSON.stringify(message.data));
const lastPersisted = (messages: Message[]) => messages.filter((message) => message.op === 'twap:stateUpdate').at(-1)?.data;
const restoredRow = (inFlight: Record<string, unknown>) => ({
  id: 'restored-twap',
  options: TWAP_OPTIONS,
  state: {
    accountName: 'acct',
    activeMarket: { symbol: 'BTCUSDT' },
    size: 0.004,
    ordersSizes: [0.002],
    side: 'buy',
    status: 'running',
    ordersCount: 2,
    ordersExecuted: 0,
    nextOrderAt: Date.now(),
    inFlight,
  },
});

it.skipIf(!algos)('a started Rust TWAP holds its cross-tab lock until it completes', async () => {
  heldLocks.clear();
  setTwapLockManager(locksFor('this-tab'));
  const { worker, messages } = await startWorker();
  worker.handlePayload({ op: 'twap:start', data: TWAP_OPTIONS });
  await vi.advanceTimersByTimeAsync(300);
  const [id] = Object.keys(lastPersisted(messages));
  expect(heldLocks.get(`tealstreet-twap:acct:${id}`)).toBe('this-tab');
  await vi.advanceTimersByTimeAsync(31_000);
  expect(lastPersisted(messages)).toEqual({});
  expect(heldLocks.size).toBe(0);
});

it.skipIf(!algos)('a restored Rust TWAP re-places an in-flight slice the venue never received, under its lock', async () => {
  heldLocks.clear();
  setTwapLockManager(locksFor('this-tab'));
  const { worker, venue, messages } = await startWorker();
  const clientOrderId = 'x-s3vFFRQp0123456789abcdef01234567';
  worker.handlePayload({ op: 'twap:state', data: [restoredRow({ size: 0.002, clientOrderId })] });
  await vi.advanceTimersByTimeAsync(300);
  expect(venue.lookups).toEqual([clientOrderId]);
  expect(venue.placements.map((order: { origQty: string }) => order.origQty)).toEqual(['0.002']);
  expect(heldLocks.get('tealstreet-twap:acct:restored-twap')).toBe('this-tab');
  expect(lastPersisted(messages)['restored-twap'].state).toMatchObject({ ordersSizes: [0.002], ordersExecuted: 1 });
  await vi.advanceTimersByTimeAsync(31_000);
  expect(venue.placements).toHaveLength(2);
});

it.skipIf(!algos)('a Rust TWAP another tab runs is not restored', async () => {
  heldLocks.clear();
  heldLocks.set('tealstreet-twap:acct:restored-twap', 'other-tab');
  setTwapLockManager(locksFor('this-tab'));
  const { worker, venue, messages } = await startWorker();
  worker.handlePayload({ op: 'twap:state', data: [restoredRow({ size: 0.002, clientOrderId: 'x-s3vFFRQp0' })] });
  await vi.advanceTimersByTimeAsync(6_000);
  expect(errorsIn(messages)).toContainEqual(expect.stringContaining('TWAP BTCUSDT not restored: it is running in another tab'));
  expect(venue.placements).toEqual([]);
  expect(venue.lookups).toEqual([]);
  expect(lastPersisted(messages)).toEqual({});
});

it.skipIf(!algos)('a nuke stops Rust TWAPs and chases in the engine and waits for the chase cancel', async () => {
  const { worker, venue, messages } = await startWorker();
  worker.handlePayload({ op: 'chaser:start', data: { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10 } });
  worker.handlePayload({ op: 'twap:start', data: TWAP_OPTIONS });
  await vi.advanceTimersByTimeAsync(300);
  const chase = lastStore(messages, 'activeChasers')[0];
  const twap = lastStore(messages, 'activeTWAPs')[0];
  const stopped = (worker.exchange as RustEngineExchange).stopAccountAlgos();
  await vi.advanceTimersByTimeAsync(1_000);
  expect(await stopped).toEqual({ stoppedChasers: [chase.id], stoppedTwaps: [twap.id] });
  const limit = venue.placements.find((order: { type: string }) => order.type === 'LIMIT');
  expect(venue.cancels).toEqual([String(limit.orderId)]);
  expect(lastPersisted(messages)).toEqual({});
  await vi.advanceTimersByTimeAsync(60_000);
  expect(venue.placements).toHaveLength(2);
});

it.skipIf(!algos)('an engine crash posts the Rust TWAPs final state so main does not restore them', async () => {
  const { worker, messages } = await startWorker();
  worker.handlePayload({ op: 'twap:start', data: TWAP_OPTIONS });
  await vi.advanceTimersByTimeAsync(300);
  expect(Object.keys(lastPersisted(messages))).toHaveLength(1);
  const host = (worker.exchange as unknown as { host: { failEngine(error: Error): void } }).host;
  host.failEngine(new Error('trap'));
  expect(lastPersisted(messages)).toEqual({});
});

/** Holds the engine load, so the account exists without its engine host until `open`. */
const holdEngineLoad = () => {
  let open!: () => void;
  const loaded = new Promise<void>((resolve) => (open = resolve));
  setWasmEngineLoader(async () => {
    await loaded;
    return loadNodeEngine();
  });
  return open;
};

it.skipIf(!algos)('an algo op that beats the engine host waits for it, then runs in the Rust algos', async () => {
  const openEngine = holdEngineLoad();
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
  await vi.waitFor(() => expect(worker.exchange).toBeTruthy());
  expect((worker.exchange as unknown as { host?: unknown }).host).toBeUndefined();
  worker.handlePayload({
    op: 'chaser:start',
    data: { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10, clientId: 'early' },
  });
  expect(venue.placements).toHaveLength(0);
  openEngine();
  await vi.advanceTimersByTimeAsync(1_000);
  expect(await started).toBe(true);
  await vi.advanceTimersByTimeAsync(300);
  expect(venue.placements).toHaveLength(1);
  expect(errorsIn(messages)).toEqual([]);
});

it.skipIf(!algos)('an algo op still waiting when the account is disposed is reported, not dropped', async () => {
  holdEngineLoad();
  const venue: Venue = binanceVenue({ clock: 'global' });
  setRustEngineTransport(() => venue.transport);
  const worker = new ExchangeWorker(
    { postMessage: () => {} } as unknown as Worker,
    'acct',
    'binance',
    { key: 'fixture-key', secret: 'fixture-secret', engine: 'rust' },
    {} as never,
    {} as never,
    'fixture',
  );
  void worker.start();
  await vi.waitFor(() => expect(worker.exchange).toBeTruthy());
  const errors: string[] = [];
  worker.exchange!.on('error', (message: string) => errors.push(message));
  worker.handlePayload({ op: 'chaser:start', data: { symbol: 'BTCUSDT', size: 0.002, side: 'buy', maxAttempts: 10 } });
  worker.dispose();
  expect(errors).toEqual(['chaser:start was not run: the binance engine never started']);
});
