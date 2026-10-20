import type { Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, Market, StoreData } from '../types';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { consumerFrame } from '../__tests__/cutover/consumerFrame';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { OrderSide, OrderType } from '../types';
import { virtualClock } from '../utils/virtualClock';
import { registerCommandExecutor, resetCommandExecutor } from './commandExecutor';
import { ExchangeWorker } from './ExchangeWorker';
import { setMainAcceptsFrames } from './frameForwarding';
import startWorkerContext, { exchangeWorkers } from './store.webworker';
import { twapManager } from './twap/TwapManager';
import { TwapStatus } from './twap/TwapStatus';

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
class ControlledEngine implements WasmEngineLike {
  pending = true;
  started = false;
  trap = false;
  trapWrites = false;
  trapSubscriptions = false;
  holdCommands = false;
  commands: Array<Record<string, unknown>> = [];
  current: Partial<StoreData> = {
    markets: [market],
    tickers: [],
    orders: [],
    positions: [],
    balance: { total: 10, free: 10, used: 0, upnl: 0 },
    loaded: { markets: true, tickers: true, orders: true, positions: true, balance: true },
  };
  constructor() {
    sources.push(this);
  }
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
  setTimeout,
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
const start = async (worker: ExchangeWorker) => {
  const promise = worker.start();
  await vi.advanceTimersByTimeAsync(250);
  return promise;
};
const panic = async (source: ControlledEngine) => {
  source.trap = true;
  await vi.advanceTimersByTimeAsync(250);
};

beforeEach(() => {
  vi.useFakeTimers();
  trapStartup = false;
  sources.length = 0;
  vi.spyOn(virtualClock, 'start').mockResolvedValue();
  setMainAcceptsFrames(true);
  setRustEngineTransport(() => transport);
  setWasmEngineLoader(async () => ({ WasmEngine: ControlledEngine }));
});
afterEach(() => {
  workers.splice(0).forEach((worker) => worker.dispose());
  setMainAcceptsFrames(false);
  resetCommandExecutor();
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

  it.each(['twap:start', 'twap:state'] as const)(
    'stops %s when its first placement traps synchronously',
    async (op) => {
      const { worker } = createWorker();
      await start(worker);
      const added = vi.spyOn(twapManager, 'add');
      const options = {
        accountName: 'same-name',
        activeMarket: market,
        size: 2,
        duration: 1,
        ordersCount: 2,
        side: OrderSide.Buy,
        randomnessPercentage: 0,
        useLimitOrders: false,
        useChaserOrders: false,
      };
      sources[0].trapWrites = true;
      worker.handlePayload({
        op,
        data:
          op === 'twap:start'
            ? options
            : [
                {
                  id: 'restored',
                  options,
                  state: {
                    accountName: options.accountName,
                    activeMarket: market,
                    size: 2,
                    ordersSizes: [1, 1],
                    ordersCount: 2,
                    ordersExecuted: 0,
                    nextOrderAt: Date.now(),
                    side: OrderSide.Buy,
                    status: TwapStatus.Running,
                  },
                },
              ],
      });
      await vi.advanceTimersByTimeAsync(1000);
      const instance = added.mock.calls[0][0];
      expect(instance.state.status).toBe(TwapStatus.Completed);
      expect(twapManager.runningTwaps.has(instance.id)).toBe(false);
      expect(sources[0].commands.filter((command) => command.op === 'placeOrders')).toHaveLength(1);
      expect(sources).toHaveLength(2);
      expect(sources[1].commands.some((command) => command.op === 'placeOrders')).toBe(false);
      await vi.advanceTimersByTimeAsync(60_000);
      expect(sources[0].commands.filter((command) => command.op === 'placeOrders')).toHaveLength(1);
    },
  );

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
    registerCommandExecutor(async (exchange, _symbol, _command, reqId) => {
      const orderIds = await exchange.placeOrder({
        symbol: market.symbol,
        side: OrderSide.Buy,
        type: OrderType.Market,
        amount: 1,
        clientOrderId: 'client-accepted',
      });
      exchange.emitter.emit('exec-result', { reqId, handled: true, success: true, data: { orderIds } });
    });
    const { worker, messages } = createWorker();
    await start(worker);
    worker.handlePayload({ op: 'exec', data: { reqId: 'accepted', command: 'fixture' } });
    sources[0].trap = true;
    expect(() => (worker.exchange as RustEngineExchange).snapshotFrame()).toThrow();
    await vi.advanceTimersByTimeAsync(1000);
    const replies = messages.filter((message) => message.op === 'exec-result' && message.data[0].reqId === 'accepted');
    expect(replies).toHaveLength(1);
    expect(replies[0].data[0]).toMatchObject({ success: true, data: { orderIds: ['accepted-id'] } });
    expect(sources[1].commands.some((command) => command.op === 'placeOrders')).toBe(false);
  });

  it('settles a non-reporting executor once and rejects its stale success after reconstruction', async () => {
    let finish!: () => void;
    const pending = new Promise<void>((resolve) => {
      finish = resolve;
    });
    registerCommandExecutor(async (exchange, _symbol, _command, reqId) => {
      await pending;
      exchange.emitter.emit('exec-result', { reqId, handled: true, success: true });
    });
    const { worker, messages } = createWorker();
    await start(worker);
    worker.handlePayload({ op: 'exec', data: { reqId: 'pending', command: 'fixture' } });
    await panic(sources[0]);
    await vi.advanceTimersByTimeAsync(1000);
    finish();
    await vi.advanceTimersByTimeAsync(0);
    const replies = messages.filter((message) => message.op === 'exec-result' && message.data[0].reqId === 'pending');
    expect(replies).toHaveLength(1);
    expect(replies[0].data[0]).toMatchObject({ success: false, error: expect.stringContaining('outcome is unknown') });
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

  it("terminates only the failed worker\'s TWAPs and never restarts them on its new account", async () => {
    const first = createWorker();
    const second = createWorker();
    await start(first.worker);
    await start(second.worker);
    const payload = {
      op: 'twap:start' as const,
      data: {
        accountName: 'same-name',
        activeMarket: market,
        size: 2,
        duration: 1,
        ordersCount: 2,
        side: OrderSide.Buy,
        randomnessPercentage: 0,
        useLimitOrders: false,
        useChaserOrders: false,
      },
    };
    first.worker.handlePayload(payload);
    const firstTwap = [...twapManager.runningTwaps.values()][0];
    second.worker.handlePayload(payload);
    const secondTwap = [...twapManager.runningTwaps.values()].find((twap) => twap !== firstTwap)!;
    await vi.advanceTimersByTimeAsync(0);
    await panic(sources[0]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(firstTwap.state.status).toBe(TwapStatus.Completed);
    expect(secondTwap.state.status).toBe(TwapStatus.Running);
    expect(twapManager.runningTwaps.get(secondTwap.id)).toBe(secondTwap);
    expect(sources[2].commands.some((command) => command.op === 'placeOrders')).toBe(false);
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
