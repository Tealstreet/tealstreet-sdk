vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { afterEach, expect, it, vi } from 'vitest';

import {
  RustEngineExchange,
  setWasmEngineLoader,
  WORKER_MIRROR_UNSUPPORTED_METHODS,
} from '../../exchanges/rust/rust.exchange';
import { CAPABILITY_METHODS, WORKER_FORWARDED_METHODS } from '../../exchanges/base';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { DefaultStore } from '../../store/store.base';

/**
 * The web's main-thread exchange is a keyless worker mirror (`workerMirror`).
 * A method that needs the engine or the key and runs there throws
 * "engine is not started" (setLeverage from a custom module or the agent,
 * 2026-10-30). Every public method is classified here, so the compiler
 * refuses a new one until someone decides where it runs.
 */
type PublicMethod = {
  [K in keyof RustEngineExchange]-?: NonNullable<RustEngineExchange[K]> extends (...args: never[]) => unknown
    ? K
    : never;
}[keyof RustEngineExchange];

/**
 * `worker`: needs the engine, so a mirror forwards it. `mirror`: correct locally.
 * `unsupported`: refused by name on a mirror (or absent there). `writer`: a write,
 * moving to the main-thread writer engine (19pt8s), which tightens these.
 */
const ROUTE: Record<PublicMethod, 'worker' | 'mirror' | 'unsupported' | 'writer'> = {
  // Writes.
  placeOrder: 'writer',
  placeOrders: 'writer',
  placeOrderWithClientOrderId: 'writer',
  updateOrder: 'writer',
  cancelOrders: 'writer',
  cancelSymbolOrders: 'writer',
  cancelAllOrders: 'writer',
  nuke: 'writer',
  nukePositions: 'writer',
  nukeOrders: 'writer',
  setLeverage: 'worker',
  setAllLeverage: 'worker',
  changeMarginType: 'worker',
  changePositionMode: 'worker',
  changeStpMode: 'worker',
  scheduleCancel: 'worker',
  changeAccountTier: 'writer',
  // Engine and account reads.
  refreshMarkets: 'worker',
  updateSymbolMetadata: 'worker',
  findOrderByClientOrderId: 'worker',
  getAccount: 'worker',
  fetchOrders: 'worker',
  fetchPositions: 'worker',
  fetchBalance: 'worker',
  fetchPositionMode: 'worker',
  fetchStpMode: 'worker',
  fetchLeverageForSymbol: 'worker',
  fetchSymbolMaxLeverage: 'worker',
  fetchAccountTier: 'worker',
  fetchOrderBook: 'worker',
  fetchOptionChain: 'worker',
  fetchOptionExpiries: 'worker',
  fetchAccountGreeks: 'worker',
  fetchPublicTrades: 'worker',
  fetchOHLCV: 'worker',
  fetchHistoricalFills: 'worker',
  fetchHistoricalOrders: 'worker',
  fetchIncomeHistory: 'worker',
  fetchIncomeHistoryRange: 'worker',
  fetchClosedPositions: 'worker',
  fetchClosedPnl: 'worker',
  fetchHistoricalPositions: 'worker',
  fetchEngineCapabilities: 'worker',
  loadCapabilities: 'worker',
  // Mirror-local by design.
  fetchMarkets: 'mirror', // warm start: answered from the worker's frames, never a local engine
  fetchTickers: 'mirror', // as fetchMarkets
  start: 'mirror', // a mirror never starts an engine
  signHttpRequest: 'mirror', // hosted signing runs on the browser's own signing host
  signWsAuth: 'mirror',
  onFrame: 'mirror',
  applyWorkerFrame: 'mirror',
  // Unsupported on a mirror.
  validateAccount: 'unsupported', // the add-key flow validates a fresh, non-mirror instance
  fetchEngineSnapshot: 'unsupported', // the worker's own recovery snapshot
  snapshotFrame: 'unsupported',
  fetchFillsForOrderIds: 'unsupported', // serves the worker's chasers; absent on a mirror
  snapshotOrderBook: 'mirror', // the mirror's own frame-applied books
  syncAccountState: 'mirror', // a no-op on a mirror: worker frames keep its store current
  getHealth: 'mirror',
  dispose: 'mirror',
  disablePublicWebsockets: 'mirror',
  observePublicFeed: 'mirror',
  syncSocketHealthToStore: 'mirror',
  recordAuthFailure: 'mirror',
  recordAuthSuccess: 'mirror',
  listenOrderBook: 'mirror', // subscriptions travel through the worker's own ops
  listenOrderBookForFeed: 'mirror',
  listenOHLCV: 'mirror',
  listenOHLCVForFeed: 'mirror',
  listenOHLCVs: 'mirror',
  listenOrderBookWebworker: 'mirror',
  listenOHLCVWebworker: 'mirror',
  // Store, formatting and event plumbing.
  mintClientOrderId: 'mirror',
  deriveAlgoOrdersFromNormalOrdersOpts: 'mirror',
  getOrFetchPositionsForSymbol: 'mirror',
  fetchAndSetOrders: 'mirror', // writes the store from a forwarded fetchOrders
  fetchAndSetPrivateSnapshot: 'mirror', // as fetchAndSetOrders
  removeOrders: 'mirror',
  updatePositionsInWebworker: 'mirror',
  fetchLastLogsFromWebsockets: 'mirror',
  getActiveTickerSymbols: 'mirror',
  setActiveTickerSymbols: 'mirror',
  getAllSockets: 'mirror',
  getPublicSockets: 'mirror',
  getPrivateSockets: 'mirror',
  reconnect: 'mirror',
  recoverAfterOffline: 'mirror',
  setOnlineStatus: 'mirror',
  emitAndLogError: 'mirror',
  emitWsReconnected: 'mirror',
  log: 'mirror',
  on: 'mirror',
  once: 'mirror',
  off: 'mirror',
  callMethodInWorker: 'mirror',
  afterMethodCalledInWorker: 'mirror',
  adaptExternalMarkets: 'mirror',
  noteHttpResponse: 'mirror',
  onWSPublicClose: 'mirror',
  // Main's own (none on a mirror); a frame-fed nuke asks the data source's.
  stopAccountAlgos: 'mirror',
  // The worker's own handlers call these on the engine they own; a mirror has none.
  startAlgos: 'mirror',
  algoOp: 'mirror',
  routeAlgoOp: 'mirror',
  refreshAccountSettings: 'worker',
  ingestSourceFrame: 'mirror',
};

/** Each must reach the worker by name here (a swallowed local failure would otherwise pass). */
const FORWARDED_WHOLE = new Set<string>([
  ...WORKER_FORWARDED_METHODS,
  // forwardToWorker's native account reads and Rust-only reads.
  'fetchOrders',
  'fetchPositions',
  'fetchBalance',
  'fetchPositionMode',
  'fetchStpMode',
  'changeStpMode',
  'fetchOrderBook',
  // Answer from rails when they can; this record makes them read the venue, through the worker.
  'fetchLeverageForSymbol',
  'fetchSymbolMaxLeverage',
]);

let mirror: RustEngineExchange | undefined;
afterEach(() => {
  mirror?.dispose();
  mirror = undefined;
});

it.each(Object.entries(ROUTE).filter(([, route]) => route === 'worker').map(([method]) => method))(
  'a mirror forwards %s to the worker and never runs an engine for it',
  async (method) => {
    const loader = vi.fn(() => new Promise<never>(() => {}));
    setWasmEngineLoader(loader);
    const store = new DefaultStore();
    store.update({ loaded: { ...store.loaded, markets: true } });
    mirror = new RustEngineExchange('lighter', { key: '', secret: '', workerMirror: true }, store);
    // The worker's record, with the venue-read leverage paths, as a running mirror adopts it.
    const record = JSON.parse(
      engineCapabilities('lighter', {
        leverageQuery: 'onReadAndMetadata',
        maxLeverage: 'rails',
        orderBookQuery: true,
        options: true,
        stpControls: true,
        positionHistory: 'bybit',
        closedPositions: true,
        closedPositionFormat: 'bybit',
      }),
    );
    let calls: string[] = [];
    mirror.on('callMethod', (event: { requestId: string; method: string }) => {
      calls.push(event.method);
      const result = event.method === 'fetchEngineCapabilities' ? record : [];
      mirror!.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result });
    });
    if (method !== 'loadCapabilities' && method !== 'fetchEngineCapabilities') {
      await mirror.loadCapabilities();
      calls = [];
    }
    const call = (mirror as unknown as Record<string, unknown>)[method];
    if (typeof call !== 'function') return; // capability-defined and absent on this venue
    const capability = Object.entries(CAPABILITY_METHODS).find(([, name]) => name === method)?.[0];
    if (capability && !mirror.capabilities[capability as keyof typeof mirror.capabilities]) {
      // The venue lacks the axis: its fallback answers or refuses locally, with no engine.
      await Promise.resolve()
        .then(() => (call as () => Promise<unknown>)())
        .catch(() => undefined);
      expect(loader).not.toHaveBeenCalled();
      return;
    }
    const outcome = await Promise.resolve()
      .then(() => (call as (...args: unknown[]) => unknown)({ symbol: 'BTCUSDT' }, 1))
      .then(
        () => 'resolved',
        (error: unknown) => String((error as Error)?.message ?? error),
      );
    // Forwarded, or answered (or refused by name) from the worker's record; never a local engine.
    expect(outcome).not.toMatch(/not started/);
    if (FORWARDED_WHOLE.has(method)) expect(calls).toContain(method);
    expect(loader).not.toHaveBeenCalled();
    expect(mirror.hasStarted).toBe(false);
  },
);

const mirrorFor = (name: 'lighter' | 'bybit') => {
  setWasmEngineLoader(vi.fn(() => new Promise<never>(() => {})));
  return new RustEngineExchange(name, { key: '', secret: '', workerMirror: true }, new DefaultStore());
};

it('refuses each unsupported method by name instead of "engine is not started"', () => {
  mirror = mirrorFor('bybit');
  const refused = Object.entries(ROUTE)
    .filter(([, route]) => route === 'unsupported')
    .map(([method]) => method)
    .filter((method) => method !== 'fetchFillsForOrderIds');
  expect(refused.sort()).toEqual([...WORKER_MIRROR_UNSUPPORTED_METHODS].sort());
  for (const method of WORKER_MIRROR_UNSUPPORTED_METHODS) {
    expect(() => (mirror as unknown as Record<string, () => unknown>)[method]()).toThrow(
      expect.objectContaining({
        kind: 'unsupported',
        outcome: 'notSent',
        message: `${method} is not available on the main-thread exchange: it runs in the account's worker`,
      }),
    );
  }
  expect(mirror.fetchFillsForOrderIds).toBeUndefined();
});

it('keeps unsupported methods working on an exchange that owns its engine', () => {
  setWasmEngineLoader(vi.fn(() => new Promise<never>(() => {})));
  mirror = new RustEngineExchange('bybit', { key: '', secret: '' }, new DefaultStore());
  expect(() => mirror!.snapshotFrame()).not.toThrow();
});
