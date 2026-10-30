import type { CommandProgress, OrderIdentityReceipt, Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, Market, StoreData } from '../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { consumerFrame } from '../__tests__/cutover/consumerFrame';
import { engineCapabilities } from '../__tests__/fixtures/engineCapabilities';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { OrderSide, OrderType } from '../types';
import { virtualClock } from '../utils/virtualClock';
import { ExchangeWorker } from './ExchangeWorker';
import { setMainAcceptsFrames } from './frameForwarding';
import startWorkerContext, { exchangeWorkers } from './store.webworker';
import { setTwapLockManager } from './twap/twapLock';

// A controllable ABI implementation, not a venue emulator. The real worker,
// Rust adapter, EngineHost and binary frame decoder own every lifecycle below.
const market: Market = {
  id: 'BTCUSDT',
  symbol: 'BTCUSDT',
  base: 'BTC',
  quote: 'USDT',
  active: true,
  precision: { amount: 0.001, price: 0.01 },
  limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
};
const sources: ControlledEngine[] = [];
let trapStartup = false;
/** Engines built next link the Rust algos; their `exec` is held, never answered. */
let algosBuild = false;
class ControlledEngine implements WasmEngineLike {
  pending = true;
  started = false;
  trap = false;
  trapWrites = false;
  trapSubscriptions = false;
  holdCommands = false;
  completeAfterMs = 0;
  progressAfterMs: number | undefined;
  receipts: OrderIdentityReceipt[] = [];
  scheduledActions = new Map<number, unknown[]>();
  commandError: Record<string, unknown> | undefined;
  commands: Array<Record<string, unknown>> = [];
  current: Partial<StoreData> = {
    markets: [market],
    tickers: [],
    orders: [],
    positions: [],
    balance: { total: 10, free: 10, used: 0, upnl: 0 },
    loaded: { markets: true, tickers: true, orders: true, positions: true, balance: true },
  };
  algoOps: Array<[string, unknown]> = [];
  algoStart?: () => void;
  algoOp?: (op: string, dataJson: string) => string;
  constructor(readonly venue = 'binance') {
    sources.push(this);
    if (!algosBuild) return;
    this.algoStart = () => {};
    this.algoOp = (op, dataJson) => {
      this.algoOps.push([op, JSON.parse(dataJson)]);
      return '[]';
    };
  }
  algoRunning = () => true;
  algoDeadline = () => undefined;
  algoEmits = () => '[]';
  algoDispose = () => '[]';
  capabilities = () => engineCapabilities(this.venue);
  start = () => {
    if (trapStartup) throw new WebAssembly.RuntimeError('startup panic');
    this.started = true;
    return '[]';
  };
  command = (json: string) => {
    const command = JSON.parse(json);
    this.commands.push(command);
    if (this.trapWrites && command.op === 'placeOrders') throw new WebAssembly.RuntimeError('placement panic');
    if (this.trapSubscriptions && command.op === 'subscribeOrderBook')
      throw new WebAssembly.RuntimeError('subscription panic');
    if (command.command !== undefined && this.progressAfterMs !== undefined) {
      const progressTimer = command.command * 2;
      const doneTimer = progressTimer + 1;
      this.scheduledActions.set(progressTimer, [
        { op: 'commandProgress', command: command.command, receipts: this.receipts },
      ]);
      this.scheduledActions.set(doneTimer, [{ op: 'done', command: command.command, result: '["late-accepted"]' }]);
      return JSON.stringify([
        { op: 'setTimer', timer: progressTimer, at: Date.now() + this.progressAfterMs },
        { op: 'setTimer', timer: doneTimer, at: Date.now() + this.progressAfterMs + 40 },
      ]);
    }
    if (command.command !== undefined && this.completeAfterMs) {
      return JSON.stringify([{ op: 'setTimer', timer: command.command, at: Date.now() + this.completeAfterMs }]);
    }
    if (command.command !== undefined && this.commandError) {
      return JSON.stringify([{ op: 'done', command: command.command, error: JSON.stringify(this.commandError) }]);
    }
    if (this.holdCommands && command.command !== undefined) return '[]';
    return command.command === undefined
      ? '[]'
      : JSON.stringify([
          { op: 'done', command: command.command, result: command.op === 'placeOrders' ? '["accepted-id"]' : '[]' },
        ]);
  };
  takeFrame = (_now: number, marketOnly: boolean) => {
    if (this.trap) throw new WebAssembly.RuntimeError('runtime panic');
    if (!this.pending || marketOnly) return undefined;
    this.pending = false;
    return new Uint8Array(consumerFrame(this.current));
  };
  snapshotFrame = () => {
    if (this.trap) throw new WebAssembly.RuntimeError('snapshot panic');
    return new Uint8Array(
      consumerFrame(
        this.started
          ? this.current
          : {
              markets: [],
              tickers: [],
              orders: [],
              positions: [],
              balance: { total: 0, free: 0, used: 0, upnl: 0 },
              loaded: { markets: false, tickers: false, orders: false, positions: false, balance: false },
            },
      ),
    );
  };
  wsOpen = () => '[]';
  wsMessage = this.wsOpen;
  wsClose = this.wsOpen;
  wsRouteExhausted = this.wsOpen;
  httpStart = this.wsOpen;
  httpChunk = this.wsOpen;
  httpEnd = this.wsOpen;
  httpFailed = this.wsOpen;
  timer = (id: number) =>
    JSON.stringify(this.scheduledActions.get(id) ?? [{ op: 'done', command: id, result: '["late-accepted"]' }]);
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
const createWorker = (options: Partial<ExchangeOptions> = {}, account = 'same-name') => {
  const messages: Array<{ op: string; data?: any }> = [];
  const worker = new ExchangeWorker(
    { postMessage: (message: any) => messages.push(message) } as unknown as Worker,
    account,
    'binance',
    { key: '', secret: '', engine: 'rust', publicDataOnly: true, ...options },
    {} as never,
    {} as never,
    'fixture',
  );
  workers.push(worker);
  return { worker, messages };
};
// Capture the actual registered callback so a stale invocation still exercises
// generation fencing even after dispose has removed emitter registrations.
const progressListener = (worker: ExchangeWorker) => {
  const emitter = worker.exchange!.emitter as unknown as {
    e: Record<string, Array<{ fn: (progress: CommandProgress) => void }>>;
  };
  return emitter.e['command-progress'].at(-1)!.fn;
};
const start = async (worker: ExchangeWorker) => {
  const promise = worker.start();
  await vi.advanceTimersByTimeAsync(250);
  return promise;
};
const panic = async (source: ControlledEngine) => {
  source.trap = true;
  await vi.advanceTimersByTimeAsync(250);
};
const placeOrder = (requestId: string, clientOrderId?: string) => ({
  op: 'callMethod' as const,
  data: {
    requestId,
    method: 'placeOrder',
    args: [
      {
        symbol: market.symbol,
        side: OrderSide.Buy,
        type: OrderType.Market,
        amount: 1,
        ...(clientOrderId && { clientOrderId }),
      },
    ],
  },
});
const replies = (messages: Array<{ op: string; data?: any }>, requestId: string) =>
  messages.filter((message) => message.op === 'callMethod:done' && message.data.requestId === requestId);

beforeEach(() => {
  // Web Locks callbacks run outside fake timers; one worker needs no cross-tab lock here.
  setTwapLockManager(null);
  vi.useFakeTimers();
  trapStartup = false;
  algosBuild = false;
  sources.length = 0;
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  setMainAcceptsFrames(true);
  setRustEngineTransport(() => transport);
  setWasmEngineLoader(async () => ({ WasmEngine: ControlledEngine }));
});
afterEach(() => {
  workers.splice(0).forEach((worker) => worker.dispose());
  setMainAcceptsFrames(false);
  vi.useRealTimers();
});

describe('Rust worker engine ownership', () => {
  it('reconstructs once with a fresh store, fences old callbacks and replays only live public demand', async () => {
    const { worker, messages } = createWorker();
    expect(await start(worker)).toBe(true);
    const original = worker.exchange as RustEngineExchange;
    const oldStore = worker.store;
    const previousUpdateIds = { ...oldStore.updateIds };
    const oldFrameCallback = original.onFrame!;
    worker.handlePayload({ op: 'ui:state', data: { activeSymbols: ['BTCUSDT'] } });
    worker.handlePayload({ op: 'subscribeOrderBook', data: { symbol: 'BTCUSDT', subId: 'keep' } });
    worker.handlePayload({ op: 'subscribeOHLCV', data: { symbol: 'BTCUSDT', interval: '1m', subId: 'remove' } });
    await panic(sources[0]);
    const boundary = messages.length;
    oldStore.update({ balance: { total: 999, free: 999, used: 0, upnl: 0 } });
    oldFrameCallback(new ArrayBuffer(1));
    original.emitter.emit('warning', 'stale generation');
    worker.handlePayload({ op: 'unsubscribeOHLCV', data: { symbol: 'BTCUSDT', interval: '1m', subId: 'remove' } });
    worker.handlePayload({
      op: 'callMethod',
      data: { requestId: 'during', method: 'setLeverage', args: ['BTCUSDT', 2] },
    });
    worker.handlePayload({ op: 'exec', data: { reqId: 'write', command: 'buy 1' } });
    await vi.advanceTimersByTimeAsync(1000);
    expect(sources).toHaveLength(2);
    expect(original.isDisposed).toBe(true);
    expect(worker.exchange).not.toBe(original);
    expect(worker.store).not.toBe(oldStore);
    expect(worker.store.balance.total).toBe(10);
    expect(worker.exchange?.getActiveTickerSymbols()).toContain('BTCUSDT');
    expect(messages.slice(boundary).some((m) => m.data === 'stale generation')).toBe(false);
    expect(messages.slice(boundary).some((m) => m.op === 'frame' && m.data.byteLength === 1)).toBe(false);
    expect(messages).toContainEqual(
      expect.objectContaining({
        op: 'callMethod:done',
        data: expect.objectContaining({ requestId: 'during', success: false }),
      }),
    );
    expect(messages).toContainEqual(
      expect.objectContaining({
        op: 'exec-result',
        data: [expect.objectContaining({ reqId: 'write', success: false })],
      }),
    );
    expect(sources[1].commands.filter((c) => c.op === 'subscribeOrderBook')).toHaveLength(1);
    expect(
      sources[1].commands.some((c) => c.op === 'subscribeCandles' || c.op === 'setLeverage' || c.op === 'placeOrder'),
    ).toBe(false);
    const reset = messages
      .slice(boundary)
      .find((m) => m.op === 'store' && m.data.loaded?.markets === false && Array.isArray(m.data.markets));
    expect(reset?.data).toMatchObject({ markets: [], tickers: [], orders: [], positions: [] });
    expect(reset?.data.updateIds.orders).toBeGreaterThan(previousUpdateIds.orders);
    expect(reset?.data.updateIds.positions).toBeGreaterThan(previousUpdateIds.positions);
    expect(messages.slice(boundary).some((m) => m.op === 'frame')).toBe(true);
    await panic(sources[1]);
    await vi.advanceTimersByTimeAsync(5000);
    expect(sources).toHaveLength(2);
    expect(messages.some((m) => m.op === 'warning' && m.data[0].includes('exhausted'))).toBe(true);
  });

  it.each(['dispose', 'deactivate'] as const)('%s cancels a queued recovery', async (op) => {
    const { worker } = createWorker();
    await start(worker);
    await panic(sources[0]);
    worker.handlePayload({ op });
    await vi.advanceTimersByTimeAsync(5000);
    expect(sources).toHaveLength(1);
  });

  it.each(['deactivate', 'panic'] as const)(
    'after %s, routine demand stays silent while a new algorithm start still fails visibly',
    async (stop) => {
      const { worker, messages } = createWorker();
      await start(worker);
      if (stop === 'panic') await panic(sources[0]);
      else worker.handlePayload({ op: 'deactivate' });
      const before = messages.length;
      worker.handlePayload({ op: 'ui:state', data: { activeSymbol: 'BTCUSDT' } } as never);
      worker.handlePayload({ op: 'store', data: {} } as never);
      worker.handlePayload({ op: 'merge-loaded', data: {} } as never);
      worker.handlePayload({ op: 'twap:stop', data: { id: 'gone' } } as never);
      expect(messages.slice(before).filter((message) => message.op === 'error')).toEqual([]);
      worker.handlePayload({ op: 'twap:start', data: {} } as never);
      expect(messages.slice(before).filter((message) => message.op === 'error')).toHaveLength(1);
    },
  );

  it('fails closed for a shared port instead of reusing it or falling back to direct feeds', async () => {
    let receive: ((event: { data: unknown }) => void) | undefined;
    const send = (data: unknown) => queueMicrotask(() => receive?.({ data }));
    const port = {
      addEventListener: (kind: string, listener: typeof receive) => {
        if (kind === 'message') receive = listener;
      },
      removeEventListener: () => {},
      start: () => {},
      close: vi.fn(),
      postMessage: (request: { op: string }) => {
        if (request.op === 'hello')
          send({
            op: 'ready',
            markets: [
              {
                id: market.id,
                symbol: market.symbol,
                contractSize: 1,
                isSpot: false,
                isPerpetual: false,
                isInverse: false,
                isExpiring: false,
              },
            ],
          });
        if (request.op === 'snapshot') send({ op: 'publicFeedUpdate', tickers: [{ symbol: market.symbol, last: 10 }] });
        if (request.op === 'ping') send({ op: 'pong' });
      },
    } as unknown as MessagePort;
    const { worker, messages } = createWorker({ publicFeed: { mode: 'shared', port } });
    expect(await start(worker)).toBe(true);
    expect(worker.store.balance.total).toBe(10);
    const boundary = messages.length;
    await panic(sources[0]);
    await vi.advanceTimersByTimeAsync(5000);
    expect(sources).toHaveLength(1);
    expect(messages.slice(boundary).some((message) => message.op === 'frame')).toBe(false);
    expect(
      messages
        .slice(boundary)
        .filter((message) => message.op === 'store')
        .every((message) => !('balance' in message.data)),
    ).toBe(true);
    expect(port.close).toHaveBeenCalledTimes(1);
    expect(worker.store.loaded).toEqual({
      markets: false,
      tickers: false,
      balance: false,
      orders: false,
      positions: false,
    });
    const invalidation = messages.filter((message) => message.op === 'store' && message.data.loaded).at(-1);
    expect(invalidation?.data).toEqual({
      loaded: { markets: false, tickers: false, balance: false, orders: false, positions: false },
      activeChasers: [],
      activeTWAPs: [],
    });
    expect(messages.some((m) => m.op === 'warning' && m.data[0].includes('fresh provider attachment'))).toBe(true);
    worker.handlePayload({ op: 'callMethod', data: { requestId: 'closed', method: 'setLeverage', args: [] } });
    expect(messages).toContainEqual(
      expect.objectContaining({
        op: 'callMethod:done',
        data: expect.objectContaining({ requestId: 'closed', success: false }),
      }),
    );
  });

  it('does not forward a late handler reply from the poisoned generation', async () => {
    const { worker, messages } = createWorker();
    await start(worker);
    sources[0].holdCommands = true;
    worker.handlePayload({ op: 'callMethod', data: { requestId: 'old', method: 'setLeverage', args: ['BTCUSDT', 2] } });
    await panic(sources[0]);
    await vi.advanceTimersByTimeAsync(1000);
    const replies = messages.filter((m) => m.op === 'callMethod:done' && m.data.requestId === 'old');
    expect(replies).toHaveLength(1);
    expect(replies[0].data).toMatchObject({
      success: false,
      result: { code: 'ENGINE_DEAD', outcome: 'unknown', message: expect.stringContaining('outcome is unknown') },
    });
  });

  it.each([
    ['subscribeOrderBook', 'unsubscribeOrderBook', 'subscribeOrderBook'],
    ['subscribeOHLCV', 'unsubscribeOHLCV', 'subscribeCandles'],
  ] as const)('honors subId-only %s removal before and during recovery', async (subscribe, unsubscribe, nativeOp) => {
    const { worker } = createWorker();
    await start(worker);
    const data = { symbol: market.symbol, interval: '1m' };
    worker.handlePayload({ op: subscribe, data: { ...data, subId: 'before' } });
    worker.handlePayload({ op: subscribe, data: { ...data, subId: 'during' } });
    worker.handlePayload({ op: unsubscribe, data: { subId: 'before' } });
    await panic(sources[0]);
    worker.handlePayload({ op: unsubscribe, data: { subId: 'during' } });
    await vi.advanceTimersByTimeAsync(1000);
    expect(sources).toHaveLength(2);
    expect(sources[1].commands.some((command) => command.op === nativeOp)).toBe(false);
  });

  it('does not announce recovered status when replaying safe demand traps the replacement', async () => {
    const { worker, messages } = createWorker();
    await start(worker);
    worker.handlePayload({ op: 'subscribeOrderBook', data: { symbol: market.symbol, subId: 'live' } });
    await panic(sources[0]);
    setWasmEngineLoader(async () => ({
      WasmEngine: class extends ControlledEngine {
        constructor() {
          super();
          this.trapSubscriptions = true;
        }
      },
    }));
    const boundary = messages.length;
    await vi.advanceTimersByTimeAsync(2000);
    expect(sources).toHaveLength(2);
    expect(worker.store.loaded.markets).toBe(false);
    expect(messages.slice(boundary).some((message) => message.op === 'ws-reconnected')).toBe(false);
    expect(
      messages.slice(boundary).some((message) => message.op === 'warning' && message.data[0].includes('exhausted')),
    ).toBe(true);
  });

  it('bounds a failed replacement startup to one attempt', async () => {
    const { worker, messages } = createWorker();
    await start(worker);
    trapStartup = true;
    await panic(sources[0]);
    await vi.advanceTimersByTimeAsync(5000);
    expect(sources).toHaveLength(2);
    expect(messages.some((message) => message.op === 'warning' && message.data[0].includes('recovery failed'))).toBe(
      true,
    );
  });

  it('cancels an in-flight replacement module lookup before constructing another account', async () => {
    const { worker } = createWorker();
    await start(worker);
    await panic(sources[0]);
    let finish!: (module: { WasmEngine: typeof ControlledEngine }) => void;
    setWasmEngineLoader(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    await vi.advanceTimersByTimeAsync(500);
    worker.dispose();
    finish({ WasmEngine: ControlledEngine });
    await vi.advanceTimersByTimeAsync(1000);
    expect(sources).toHaveLength(1);
  });

  it('preserves a native write acknowledgement that completed just before the engine trapped', async () => {
    const { worker, messages } = createWorker();
    await start(worker);
    worker.handlePayload(placeOrder('accepted', 'client-accepted'));
    const original = worker.exchange as RustEngineExchange;
    sources[0].trap = true;
    // The pre-snapshot flush contains the trap and disposes the host before the read.
    expect(original.snapshotFrame()).toBeUndefined();
    expect(original.isEngineDead).toBe(true);
    expect(original.isDisposed).toBe(true);
    await vi.advanceTimersByTimeAsync(1000);
    const accepted = replies(messages, 'accepted');
    expect(accepted).toHaveLength(1);
    expect(accepted[0].data).toMatchObject({ success: true, result: ['accepted-id'] });
    expect(sources[1].commands.some((command) => command.op === 'placeOrders')).toBe(false);
  });

  it('settles a held Rust exec once, as an unknown outcome, when the engine traps', async () => {
    algosBuild = true;
    const { worker, messages } = createWorker();
    await start(worker);
    expect((worker.exchange as RustEngineExchange).runsRustAlgos).toBe(true);
    worker.handlePayload({ op: 'exec', data: { reqId: 'pending', symbol: market.symbol, command: 'buy 1' } });
    expect(sources[0].algoOps).toEqual([['exec', { reqId: 'pending', symbol: market.symbol, command: 'buy 1' }]]);
    await panic(sources[0]);
    await vi.advanceTimersByTimeAsync(1000);
    const execReplies = messages.filter(
      (message) => message.op === 'exec-result' && message.data[0].reqId === 'pending',
    );
    expect(execReplies).toHaveLength(1);
    expect(execReplies[0].data[0]).toMatchObject({
      success: false,
      error: expect.stringContaining('outcome is unknown'),
    });
    expect(sources[1].algoOps).toEqual([]);
  });

  it('refuses exec visibly on an engine build without algos', async () => {
    const { worker, messages } = createWorker();
    await start(worker);
    worker.handlePayload({ op: 'exec', data: { reqId: 'no-algos', symbol: market.symbol, command: 'buy 1' } });
    await vi.advanceTimersByTimeAsync(0);
    const execReplies = messages.filter(
      (message) => message.op === 'exec-result' && message.data[0].reqId === 'no-algos',
    );
    expect(execReplies).toHaveLength(1);
    expect(execReplies[0].data[0]).toMatchObject({
      success: false,
      error: expect.stringContaining('engine build has no algos'),
    });
  });

  it('settles pending native read/write on disposal before fallback and fences late state', async () => {
    const { worker, messages } = createWorker();
    const sibling = createWorker({}, 'sibling');
    await start(worker);
    await start(sibling.worker);
    const original = worker.exchange as RustEngineExchange;
    const oldFrame = original.onFrame!;
    const oldStore = worker.store;
    sources[0].holdCommands = true;
    worker.handlePayload(placeOrder('write-dispose', 'caller-disposed'));
    worker.handlePayload({
      op: 'callMethod',
      data: { requestId: 'read-dispose', method: 'fetchOHLCV', args: [{ symbol: market.symbol, interval: '1m' }] },
    });
    worker.dispose();
    worker.dispose();
    const boundary = messages.length;
    oldFrame(new ArrayBuffer(1));
    oldStore.update({ balance: { total: 999, free: 999, used: 0, upnl: 0 } });
    await vi.advanceTimersByTimeAsync(0);
    const write = replies(messages, 'write-dispose');
    expect(write).toHaveLength(1);
    expect(write[0].data).toMatchObject({
      success: false,
      result: {
        code: 'ENGINE_DISPOSED',
        command: 'placeOrders',
        outcome: 'unknown',
        clientOrderId: 'caller-disposed',
        clientOrderIds: ['caller-disposed'],
      },
    });
    const read = messages.filter((m) => m.op === 'callMethod:done' && m.data.requestId === 'read-dispose');
    expect(read).toHaveLength(1);
    expect(read[0].data).toMatchObject({
      success: false,
      result: { code: 'ENGINE_DISPOSED', command: 'fetchCandles' },
    });
    expect(read[0].data.result).not.toHaveProperty('outcome');
    expect(messages.slice(boundary).some((m) => m.op === 'frame' || m.op === 'store')).toBe(false);
    expect(sibling.worker.exchange?.isDisposed).toBe(false);
    expect(sources[0].commands.filter((c) => c.op === 'placeOrders')).toHaveLength(1);
    expect(sources).toHaveLength(2);
  });

  it('keeps a known acknowledgement queued before repeated normal dispose', async () => {
    const { worker, messages } = createWorker();
    await start(worker);
    worker.handlePayload(placeOrder('known-dispose'));
    worker.dispose();
    worker.dispose();
    await vi.advanceTimersByTimeAsync(0);
    const known = replies(messages, 'known-dispose');
    expect(known).toHaveLength(1);
    expect(known[0].data).toMatchObject({ success: true, result: ['accepted-id'] });
  });

  it('retains native partial receipts queued before disposal without inventing unknown attribution', async () => {
    const { worker, messages } = createWorker();
    await start(worker);
    sources[0].commandError = {
      message: 'second leg refused',
      partialOrderIds: ['known-first'],
      partialOrderCount: 1,
      totalOrderCount: 2,
      failedOrderIndex: 1,
    };
    worker.handlePayload(placeOrder('partial-dispose'));
    worker.dispose();
    await vi.advanceTimersByTimeAsync(0);
    const partial = replies(messages, 'partial-dispose');
    expect(partial).toHaveLength(1);
    expect(partial[0].data).toMatchObject({
      success: false,
      result: {
        message: 'second leg refused',
        partialOrderIds: ['known-first'],
        partialOrderCount: 1,
        totalOrderCount: 2,
        failedOrderIndex: 1,
      },
    });
    expect(partial[0].data.result).not.toHaveProperty('outcome');
  });

  it('bounds a live native command deadline, ignores late completion, and never replays the write', async () => {
    const { worker, messages } = createWorker({ extra: { rustCommandTimeoutMs: 20 } });
    await start(worker);
    expect((worker.exchange as unknown as { commandTimeoutMs: number }).commandTimeoutMs).toBe(20);
    sources[0].completeAfterMs = 40;
    worker.handlePayload(placeOrder('deadline', 'caller-timeout'));
    await vi.advanceTimersByTimeAsync(25);
    const deadline = () => replies(messages, 'deadline');
    expect(deadline()).toHaveLength(1);
    expect(deadline()[0].data).toMatchObject({
      success: false,
      result: {
        code: 'COMMAND_TIMEOUT',
        command: 'placeOrders',
        timeoutMs: 20,
        outcome: 'unknown',
        clientOrderId: 'caller-timeout',
      },
    });
    await vi.advanceTimersByTimeAsync(100);
    expect(deadline()).toHaveLength(1);
    expect(worker.exchange?.isDisposed).toBe(false);
    expect(sources[0].commands.filter((c) => c.op === 'placeOrders')).toHaveLength(1);
    expect(sources).toHaveLength(1);
  });

  it.each(['setLeverage', 'fetchOHLCV'] as const)(
    'settles silent %s fallback once even when cleanup throws',
    async (method) => {
      const { worker, messages } = createWorker();
      await start(worker);
      const original = worker.exchange!;
      let finish!: (value: any) => void;
      vi.spyOn(original, method).mockImplementation(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }) as never,
      );
      worker.handlePayload({ op: 'callMethod', data: { requestId: 'silent', method, args: [] } });
      const oldFrame = (original as RustEngineExchange).onFrame!;
      const realDispose = original.dispose.bind(original);
      vi.spyOn(original, 'dispose').mockImplementation(() => {
        realDispose();
        throw new Error('cleanup failed');
      });
      expect(() => worker.dispose()).toThrow('cleanup failed');
      worker.dispose();
      oldFrame(new ArrayBuffer(1));
      await vi.advanceTimersByTimeAsync(0);
      finish([]);
      await vi.advanceTimersByTimeAsync(0);
      const replies = messages.filter((m) => m.op === 'callMethod:done' && m.data.requestId === 'silent');
      expect(replies).toHaveLength(1);
      expect(replies[0].data).toMatchObject({ success: false, result: { code: 'ENGINE_DISPOSED', command: method } });
      if (method === 'setLeverage') expect(replies[0].data.result.outcome).toBe('unknown');
      else expect(replies[0].data.result).not.toHaveProperty('outcome');
      expect(messages.some((m) => m.op === 'frame' && m.data.byteLength === 1)).toBe(false);
    },
  );

  it('routes native receipts after RPC timeout to their original account without changing the settled error', async () => {
    const { worker, messages } = createWorker({ extra: { rustCommandTimeoutMs: 20 } }, 'receipt-owner');
    const sibling = createWorker({}, 'other-owner');
    await start(worker);
    await start(sibling.worker);
    sources[0].progressAfterMs = 40;
    sources[0].receipts = [
      { symbol: market.symbol, kind: 'clientOrderId', identity: 'native-generated-child' },
      { symbol: market.symbol, kind: 'clientOrderIndex', identity: '9007199254740993' },
      { symbol: market.symbol, kind: 'orderCode', identity: '0000000017' },
    ];
    worker.handlePayload(placeOrder('late-identities', 'caller-parent'));
    await vi.advanceTimersByTimeAsync(25);
    const late = () => replies(messages, 'late-identities');
    expect(late()).toHaveLength(1);
    const snapshot = structuredClone(late()[0].data.result);
    expect(snapshot).toMatchObject({
      code: 'COMMAND_TIMEOUT',
      outcome: 'unknown',
      clientOrderId: 'caller-parent',
      clientOrderIds: ['caller-parent'],
      orderIdentities: [],
      actualClientOrderIds: [],
      receiptKey: expect.any(String),
      commandId: expect.any(Number),
    });
    expect(messages.filter((m) => m.op === 'command-progress')).toHaveLength(0);
    const positionsVersion = worker.store.updateIds.positions;
    await vi.advanceTimersByTimeAsync(20);
    const progress = messages.filter((m) => m.op === 'command-progress');
    expect(progress).toHaveLength(1);
    expect(progress[0].data).toMatchObject({
      account: 'receipt-owner',
      generation: expect.any(Number),
      receiptKey: snapshot.receiptKey,
      commandId: snapshot.commandId,
      receipts: sources[0].receipts,
      complete: false,
      late: true,
    });
    expect(sibling.messages.some((m) => m.op === 'command-progress')).toBe(false);
    expect(worker.store.updateIds.positions).toBe(positionsVersion);
    expect(worker.store.orders).toEqual([]);
    await vi.advanceTimersByTimeAsync(40);
    expect(late()).toHaveLength(1);
    expect(late()[0].data.result).toEqual(snapshot);
    expect(messages.filter((m) => m.op === 'command-progress').at(-1)?.data).toMatchObject({
      account: 'receipt-owner',
      receiptKey: snapshot.receiptKey,
      receipts: [],
      complete: true,
      late: true,
    });
    expect(sources[0].commands.filter((c) => c.op === 'placeOrders')).toHaveLength(1);
    const staleProgress = progressListener(worker);
    const priorEvents = messages.filter((m) => m.op === 'command-progress').length;
    worker.dispose();
    staleProgress({
      ...progress[0].data,
      receipts: [{ symbol: market.symbol, kind: 'clientOrderId', identity: 'closed-stream' }],
    });
    await vi.advanceTimersByTimeAsync(100);
    expect(messages.filter((m) => m.op === 'command-progress')).toHaveLength(priorEvents);
  });

  it('preserves actual pre-deadline receipts and fences the old host after reconstruction', async () => {
    const { worker, messages } = createWorker({ extra: { rustCommandTimeoutMs: 20 } });
    await start(worker);
    const staleProgress = progressListener(worker);
    sources[0].progressAfterMs = 0;
    sources[0].receipts = [{ symbol: market.symbol, kind: 'clientOrderId', identity: 'selected-native' }];
    worker.handlePayload(placeOrder('early-identities', 'caller-request'));
    await vi.advanceTimersByTimeAsync(25);
    const [reply] = replies(messages, 'early-identities');
    expect(reply?.data.result).toMatchObject({
      code: 'COMMAND_TIMEOUT',
      clientOrderIds: ['caller-request'],
      actualClientOrderIds: ['selected-native'],
      orderIdentities: sources[0].receipts,
    });
    const first = messages.find((m) => m.op === 'command-progress')!.data;
    expect(first).toMatchObject({ late: false, complete: false, account: 'same-name' });
    await panic(sources[0]);
    await vi.advanceTimersByTimeAsync(1000);
    const count = messages.filter((m) => m.op === 'command-progress').length;
    staleProgress(first);
    expect(messages.filter((m) => m.op === 'command-progress')).toHaveLength(count);
    worker.exchange!.emitter.emit('command-progress', {
      ...first,
      receiptKey: 'new-host:1',
      commandId: 1,
      account: 'spoofed-owner',
      generation: -1,
    });
    const newest = messages.filter((m) => m.op === 'command-progress').at(-1)!.data;
    expect(newest.account).toBe(first.account);
    expect(newest.generation).toBeGreaterThan(first.generation);
    expect(newest.receiptKey).toBe('new-host:1');
  });

  it('keeps SharedWorker attachments on the recovering owner', async () => {
    const context = () => {
      let receive!: (event: { data: unknown }) => Promise<void>;
      const messages: Array<{ op: string; data: any }> = [];
      const ctx = {
        postMessage: (message: any) => messages.push(message),
        addEventListener: (_kind: string, listener: typeof receive) => {
          receive = listener;
        },
      };
      startWorkerContext(ctx as unknown as Worker, true);
      return { messages, send: (data: unknown) => receive({ data }) };
    };
    const first = context();
    const second = context();
    const init = {
      op: 'init',
      data: {
        accountName: 'recovering-shared-owner',
        exchangeName: 'binance',
        options: { engine: 'rust', publicDataOnly: true },
        settings: {},
        metadata: {},
        sessionKey: 'fixture',
        store: {},
      },
    };
    const initial = first.send(init);
    await vi.advanceTimersByTimeAsync(250);
    await initial;
    const owner = exchangeWorkers['recovering-shared-owner'];
    workers.push(owner);
    await panic(sources[0]);
    await second.send(init);
    expect(exchangeWorkers['recovering-shared-owner']).toBe(owner);
    expect(sources).toHaveLength(1);
    expect(second.messages.some((message) => message.op === 'warning')).toBe(true);
    expect(second.messages).toContainEqual({ op: 'start-status', data: false });
    await vi.advanceTimersByTimeAsync(1000);
    expect(sources).toHaveLength(2);
    await first.send({ op: 'dispose' });
    await second.send({ op: 'dispose' });
  });

  it('does not reconstruct a startup panic and reports its startup failure', async () => {
    trapStartup = true;
    const { worker, messages } = createWorker();
    expect(await start(worker)).toBe(false);
    await vi.advanceTimersByTimeAsync(5000);
    expect(sources).toHaveLength(1);
    expect(messages.some((m) => m.op === 'log' && m.data.context.worker_error_phase === 'startup')).toBe(true);
  });

  it('leaves another account alive in the same module even when account names match', async () => {
    const first = createWorker();
    const second = createWorker();
    await start(first.worker);
    await start(second.worker);
    const unaffected = second.worker.exchange;
    await panic(sources[0]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(second.worker.exchange).toBe(unaffected);
    expect(unaffected?.isDisposed).toBe(false);
    expect(second.worker.store.loaded.markets).toBe(true);
    expect(sources).toHaveLength(3);
  });
});
