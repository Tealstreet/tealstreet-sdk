import type { ExchangeOptions, Order } from '../../types';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { decodeWorkerOrderIds, encodeWorkerOrderIds, replacementClientOrderIdOf, withReplacementClientOrderId } from '../../exchanges/clientOrderId';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { OrderSide, OrderStatus, OrderType } from '../../types';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const accounts: BaseExchange[] = [];
const amendmentOrder: Order = {
  id: 'original', symbol: 'BTCUSDT', status: OrderStatus.Open, type: OrderType.Limit,
  side: OrderSide.Buy, price: 100, amount: 1, filled: 0, remaining: 1, reduceOnly: false, lastUpdate: 0,
};

it.each([{ ids: [] }, { ids: ['accepted', 'accepted', ''] }])('keeps replacement IDs and client attribution through clone boundaries ($ids)', ({ ids }) => {
  const envelope = encodeWorkerOrderIds(withReplacementClientOrderId([...ids], 'replacement-client'));
  for (const copy of [structuredClone(envelope), JSON.parse(JSON.stringify(envelope))]) {
    const decoded = decodeWorkerOrderIds(copy);
    expect([...decoded]).toEqual(ids);
    expect(replacementClientOrderIdOf(decoded)).toBe('replacement-client');
    expect(decoded).not.toBe(copy.orderIds);
  }
  expect(envelope).toEqual({ orderIds: ids, clientOrderId: 'replacement-client' });
});

it.each([
  { result: undefined }, { result: null }, { result: {} }, { result: [] },
  { result: { orderIds: null } }, { result: { orderIds: 'accepted' } },
  { result: { orderIds: [null, 123] } }, { result: { orderIds: Array(1) } },
])('keeps a malformed amendment reply uncertain ($result)', ({ result }) => {
  expect(() => decodeWorkerOrderIds(result)).toThrow(/reconcile/i);
  try { decodeWorkerOrderIds(result); } catch (error) {
    expect(error).toMatchObject({ kind: 'internal', outcome: 'unknown', command: 'updateOrder' });
  }
});

it.each([{ orderIds: ['accepted'] }, { orderIds: [], clientOrderId: '' }])('preserves amendment replies without client attribution (%j)', (result) => {
  const decoded = decodeWorkerOrderIds(result);
  expect(decoded).toEqual(result.orderIds);
  expect(replacementClientOrderIdOf(decoded)).toBeUndefined();
});

it.each([
  { orderIds: ['accepted', 123, 'other'], clientOrderId: 'replacement-client' },
  { orderIds: ['accepted', 'other'], clientOrderId: 123 },
])('retains known partial IDs when an amendment reply is malformed (%j)', (result) => {
  try {
    decodeWorkerOrderIds(result);
    throw new Error('Expected malformed amendment reply to fail');
  } catch (error) {
    expect(error).toMatchObject({ outcome: 'unknown', partialOrderIds: ['accepted', 'other'] });
    if (typeof result.clientOrderId === 'string') expect(error).toHaveProperty('clientOrderId', result.clientOrderId);
    else expect(error).not.toHaveProperty('clientOrderId');
  }
});

it('rejects a malformed worker amendment success without replaying the write', async () => {
  const exchange = account(true, { publicDataOnly: true, workerMirror: true });
  const bridge = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', {
      requestId, success: true, result: { orderIds: ['accepted', 123], clientOrderId: 'replacement-client' },
    });
  });
  exchange.on('callMethod', bridge);
  await expect(exchange.updateOrder({ order: amendmentOrder, update: { price: 105 } })).rejects.toMatchObject({
    command: 'updateOrder', kind: 'internal', outcome: 'unknown',
    partialOrderIds: ['accepted'], clientOrderId: 'replacement-client',
  });
  expect(bridge).toHaveBeenCalledOnce();
});

it('preserves a valid worker amendment replacement ID after structured cloning', async () => {
  const exchange = account(true, { publicDataOnly: true, workerMirror: true });
  const bridge = vi.fn(({ requestId }: { requestId: string }) => {
    const result = structuredClone(encodeWorkerOrderIds(withReplacementClientOrderId([], 'replacement-client')));
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result });
  });
  exchange.on('callMethod', bridge);
  const result = await exchange.updateOrder({ order: amendmentOrder, update: { price: 105 } });
  expect([...result]).toEqual([]);
  expect(replacementClientOrderIdOf(result)).toBe('replacement-client');
  expect(bridge).toHaveBeenCalledOnce();
});

it.each(['unknown', 'rejected', 'notSent'])('preserves an amendment rejection outcome and partial IDs (%s)', async (outcome) => {
  const exchange = account(true, { publicDataOnly: true, workerMirror: true });
  vi.spyOn(exchange, 'emitAndLogError').mockImplementation(() => {});
  const bridge = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', {
      requestId, success: false,
      result: { message: 'Original worker failure', outcome, partialOrderIds: ['accepted'], clientOrderId: 'replacement-client' },
    });
  });
  exchange.on('callMethod', bridge);
  await expect(exchange.updateOrder({ order: amendmentOrder, update: { price: 105 } })).rejects.toMatchObject({
    message: 'Original worker failure', outcome, partialOrderIds: ['accepted'], clientOrderId: 'replacement-client',
  });
  expect(bridge).toHaveBeenCalledOnce();
});
const account = (rust = true, options: Partial<ExchangeOptions> = { publicDataOnly: true }): BaseExchange => {
  const config = { key: '', secret: '', ...options };
  const exchange = rust
    ? new RustEngineExchange('bybit', config, new DefaultStore())
    : new BaseExchange('bybit', config, new DefaultStore());
  accounts.push(exchange);
  return exchange;
};

afterEach(() => {
  accounts.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  vi.useRealTimers();
});

it('registers the response listener before dispatching a synchronous worker reply', async () => {
  const exchange = account();
  exchange.emitter.on('callMethod', ({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: ['acknowledged'] });
  });
  await expect(exchange.callMethodInWorker('fetchOrders')).resolves.toEqual(['acknowledged']);
});

it('reconstructs unknown outcomes and client attribution at the main-thread consumer', async () => {
  const exchange = account();
  exchange.emitter.on('callMethod', ({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', {
      requestId,
      success: false,
      result: {
        message: 'Reconcile before retrying',
        name: 'CommandInterruptedError',
        code: 'ENGINE_DISPOSED',
        command: 'setLeverage',
        outcome: 'unknown',
        clientOrderId: 'actual-client',
        clientOrderIds: ['actual-client', 'child-client'],
        cause: { credentials: 'must not cross the error boundary' },
      },
    });
  });
  const failure = await exchange.callMethodInWorker('setLeverage', 'BTCUSDT', 3).catch((error: unknown) => error);
  expect(failure).toMatchObject({
    message: 'Reconcile before retrying', code: 'ENGINE_DISPOSED', command: 'setLeverage', outcome: 'unknown',
    clientOrderId: 'actual-client', clientOrderIds: ['actual-client', 'child-client'],
  });
  expect(failure).not.toHaveProperty('cause');
});

it('retains known partial rejections without converting them to unknown outcomes', async () => {
  const exchange = account();
  exchange.emitter.on('callMethod', ({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', {
      requestId, success: false,
      result: {
        message: 'Second order rejected', code: 400,
        partialOrderIds: ['accepted-id'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1,
        clientOrderIds: ['valid', 123],
      },
    });
  });
  const failure = await exchange.callMethodInWorker('fetchOrders').catch((error: unknown) => error);
  expect(failure).toMatchObject({
    message: 'Second order rejected', code: 400,
    partialOrderIds: ['accepted-id'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1,
  });
  expect(failure).not.toHaveProperty('outcome');
  expect(failure).not.toHaveProperty('clientOrderIds');
});

it('keeps the TS worker waiting deadline at five seconds', async () => {
  vi.useFakeTimers();
  const exchange = account(false);
  const failure = exchange.callMethodInWorker('fetchOrders').catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(5000);
  await expect(failure).resolves.toMatchObject({ message: 'Timeout callMethodInWorker' });
  expect(vi.getTimerCount()).toBe(0);
});

it('waits for the Rust host deadline and reply delivery before declaring a worker unavailable', async () => {
  vi.useFakeTimers();
  const exchange = account();
  let settled = false;
  const failure = exchange.callMethodInWorker('setLeverage', 'BTCUSDT', 3).catch((error: unknown) => {
    settled = true;
    return error;
  });
  await vi.advanceTimersByTimeAsync(120_999);
  expect(settled).toBe(false);
  await vi.advanceTimersByTimeAsync(1);
  await expect(failure).resolves.toMatchObject({ code: 'COMMAND_TIMEOUT', command: 'setLeverage', outcome: 'unknown' });
  expect(vi.getTimerCount()).toBe(0);
});

it('uses the configured Rust deadline and keeps reads distinct from interrupted writes', async () => {
  vi.useFakeTimers();
  const exchange = account(true, { publicDataOnly: true, extra: { rustCommandTimeoutMs: 6000 } });
  let settled = false;
  const failure = exchange.callMethodInWorker('fetchOrders').catch((error: unknown) => {
    settled = true;
    return error;
  });
  await vi.advanceTimersByTimeAsync(6999);
  expect(settled).toBe(false);
  await vi.advanceTimersByTimeAsync(1);
  const error = await failure;
  expect(error).toMatchObject({ code: 'COMMAND_TIMEOUT', command: 'fetchOrders' });
  expect(error).not.toHaveProperty('outcome');
});

it('applies the configured deadline to the live SDK host while preserving caller IDs', async () => {
  vi.useFakeTimers();
  const requests: Array<{ op: string }> = [];
  const idle = () => '[]';
  const native: WasmEngineLike = {
    syncHostTime: () => true,
    capabilities: () => engineCapabilities('bybit'),
    start: idle, wsOpen: idle, wsMessage: idle, wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle, httpChunk: idle, httpEnd: idle, httpFailed: idle, timer: idle,
    command: (json) => { requests.push(JSON.parse(json)); return '[]'; },
    snapshotFrame: () => new Uint8Array(),
    // Public-only readiness needs decoded market and ticker sections as well as STATUS flags.
    takeFrame: () => new Uint8Array(consumerFrame({
      markets: [], tickers: [],
      loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
    })),
  };
  const construct = function () { return native; };
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const exchange = account(true, { publicDataOnly: true, extra: { rustCommandTimeoutMs: 25 } });
  const startup = exchange.start();
  await vi.advanceTimersByTimeAsync(250);
  await startup;
  const failure = exchange.placeOrders([{
    symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 1, price: 100,
    clientOrderId: 'caller-one',
  }]).catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(25);
  await expect(failure).resolves.toMatchObject({
    code: 'COMMAND_TIMEOUT', command: 'placeOrders', outcome: 'unknown', clientOrderId: 'caller-one',
  });
  expect(requests.filter(({ op }) => op === 'placeOrders')).toHaveLength(1);
  expect(exchange.isDisposed).toBe(false);
});

it('releases the listener and timer when dispatch itself throws', async () => {
  vi.useFakeTimers();
  const exchange = account();
  const off = vi.spyOn(exchange.emitter, 'off');
  exchange.emitter.on('callMethod', () => { throw new Error('Worker transport unavailable'); });
  await expect(exchange.callMethodInWorker('fetchOrders')).rejects.toThrow('Worker transport unavailable');
  expect(off).toHaveBeenCalledWith('callMethod:done', expect.any(Function));
  expect(vi.getTimerCount()).toBe(0);
});

it.each([0, -1, NaN, Infinity, 0.5, 2_147_482_648])('rejects an invalid Rust deadline %s', (rustCommandTimeoutMs) => {
  const store = new DefaultStore();
  const subscribe = vi.spyOn(store, 'subscribe');
  expect(() => new RustEngineExchange('bybit', { key: '', secret: '', extra: { rustCommandTimeoutMs } }, store))
    .toThrow('rustCommandTimeoutMs must be a positive integer');
  expect(subscribe).not.toHaveBeenCalled();
});

it('treats a lost catalogue-refresh worker reply as a read timeout', async () => {
  vi.useFakeTimers();
  const exchange = account(true, { publicDataOnly: true, extra: { rustCommandTimeoutMs: 25 } });
  const failure = exchange.callMethodInWorker('refreshMarkets').catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(1025);
  const error = await failure;
  expect(error).toMatchObject({ code: 'COMMAND_TIMEOUT', command: 'refreshMarkets' });
  expect(error).not.toHaveProperty('outcome');
});

it('labels an interrupted read separately from uncertain writes', async () => {
  const exchange = account(true, { publicDataOnly: true, workerMirror: true });
  const bridge = () => {};
  exchange.on('callMethod', bridge);
  const failure = exchange.callMethodInWorker('fetchOrders').catch((error: unknown) => error);
  exchange.off('callMethod', bridge);
  expect(await failure).toMatchObject({ code: 'WORKER_STOPPED', command: 'fetchOrders' });
  expect(await failure).not.toHaveProperty('outcome');
  expect(((await failure) as Error).message).not.toContain('interrupted writes');
});

it('dispatches a write unchanged when its ready worker bridge stays attached', async () => {
  const exchange = account(true, { publicDataOnly: true, workerMirror: true });
  const ownerA = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: 'owner-A' });
  });
  exchange.on('callMethod', ownerA);
  await expect(exchange.setLeverage('BTCUSDT', 3)).resolves.toBe('owner-A');
  expect(ownerA).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
    method: 'setLeverage', args: ['BTCUSDT', 3],
  }));
});

it.each(['exchange', 'emitter'] as const)(
  'drops a write when its ready worker bridge changes during the await through %s',
  async (subscriptions) => {
    const exchange = account(true, { publicDataOnly: true, workerMirror: true });
    (exchange as RustEngineExchange).setWorkerOwner(() => ({ ownerId: 'A', generation: 1 }));
    const ownerA = vi.fn();
    const ownerB = vi.fn(({ requestId }: { requestId: string }) => {
      exchange.emitter.emit('callMethod:done', { requestId, success: true, result: 'owner-B' });
    });
    exchange.on('callMethod', ownerA);
    const pending = exchange.setLeverage('BTCUSDT', 3).catch((error: unknown) => error);
    expect(ownerA).not.toHaveBeenCalled();

    // Replace the login on the retained mirror before the ready await resumes.
    const events = subscriptions === 'exchange' ? exchange : exchange.emitter;
    events.off('callMethod', ownerA);
    events.on('callMethod', ownerB);
    expect(await pending).toMatchObject({ command: 'setLeverage', outcome: 'notSent' });
    expect(ownerA).not.toHaveBeenCalled();
    expect(ownerB).not.toHaveBeenCalled();

    await expect(exchange.setLeverage('BTCUSDT', 7)).resolves.toBe('owner-B');
    expect(ownerB).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
      method: 'setLeverage', args: ['BTCUSDT', 7],
    }));
  },
);

it('drops a write when a ready bridge callback is reattached for another owner', async () => {
  const exchange = account(true, { publicDataOnly: true, workerMirror: true });
  (exchange as RustEngineExchange).setWorkerOwner(() => ({ ownerId: 'A', generation: 1 }));
  const ownerA = { writes: vi.fn() };
  const ownerB = { writes: vi.fn() };
  const bridge = function (this: typeof ownerA, { requestId }: { requestId: string }) {
    this.writes();
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: 'current-owner' });
  };
  exchange.emitter.on('callMethod', bridge, ownerA);
  const pending = exchange.setLeverage('BTCUSDT', 3).catch((error: unknown) => error);
  exchange.emitter.off('callMethod', bridge);
  exchange.emitter.on('callMethod', bridge, ownerB);
  expect(await pending).toMatchObject({ command: 'setLeverage', outcome: 'notSent' });
  expect(ownerA.writes).not.toHaveBeenCalled();
  expect(ownerB.writes).not.toHaveBeenCalled();

  await expect(exchange.setLeverage('BTCUSDT', 7)).resolves.toBe('current-owner');
  expect(ownerB.writes).toHaveBeenCalledOnce();
});

it.each(['never-wired', 'detached'] as const)('preserves master bootstrap without an owner hook (%s)', async (phase) => {
  vi.useFakeTimers();
  const exchange = account(true, { publicDataOnly: true, workerMirror: true });
  const previous = vi.fn();
  const attached = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: 'owner-A' });
  });
  if (phase === 'detached') {
    exchange.on('callMethod', previous);
    exchange.off('callMethod', previous);
  }
  const pending = exchange.setLeverage('BTCUSDT', 3).catch((error: unknown) => error);
  exchange.on('callMethod', attached);
  await vi.advanceTimersByTimeAsync(25);
  expect(await pending).toBe('owner-A');
  expect(previous).not.toHaveBeenCalled();
  expect(attached).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
    method: 'setLeverage', args: ['BTCUSDT', 3],
  }));
});

it.each([false, true])('preserves ready bridge dispatch compatibility (hooked=%s)', async (hooked) => {
  const exchange = account(true, { publicDataOnly: true, workerMirror: true }) as RustEngineExchange;
  let owner = { ownerId: 'A', generation: 1 };
  if (hooked) exchange.setWorkerOwner(() => owner);
  const first = vi.fn();
  const second = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: 'owner-B' });
  });
  exchange.on('callMethod', first);
  const pending = exchange.setLeverage('BTCUSDT', 3).catch((error: unknown) => error);
  owner = { ownerId: 'B', generation: 2 };
  exchange.off('callMethod', first);
  exchange.on('callMethod', second);
  if (hooked) {
    expect(await pending).toMatchObject({ command: 'setLeverage', code: 'WORKER_STOPPED', outcome: 'notSent' });
    expect(second).not.toHaveBeenCalled();
  } else {
    expect(await pending).toBe('owner-B');
    expect(second).toHaveBeenCalledOnce();
  }
  expect(first).not.toHaveBeenCalled();
});

it('refuses unwired calls when the installed owner hook has no current owner', async () => {
  const exchange = account(true, { publicDataOnly: true, workerMirror: true }) as RustEngineExchange;
  exchange.setWorkerOwner(() => undefined);
  await expect(exchange.setLeverage('BTCUSDT', 3)).rejects.toMatchObject({
    command: 'setLeverage', code: 'WORKER_STOPPED', outcome: 'notSent',
  });
});

it.each([
  { hooked: false, transition: 'same-owner' }, { hooked: false, transition: 'ready-switch' },
  { hooked: false, transition: 'unwired-switch' }, { hooked: true, transition: 'same-owner' },
  { hooked: true, transition: 'ready-switch' }, { hooked: true, transition: 'unwired-switch' },
])('fences capability-adopted keyed closed PnL (hooked=$hooked, $transition)', async ({ hooked, transition }) => {
  vi.useFakeTimers();
  const exchange = account(true, {
    key: 'fixture', secret: 'fixture', publicDataOnly: false, workerMirror: true,
  }) as RustEngineExchange;
  let owner = { ownerId: 'A', generation: 1 };
  if (hooked) exchange.setWorkerOwner(() => owner);
  const first = vi.fn(({ requestId, method }: { requestId: string; method: string }) => {
    exchange.emitter.emit('callMethod:done', {
      requestId, success: true,
      result: method === 'fetchEngineCapabilities' ? JSON.parse(engineCapabilities('bybit')) : ['owner-A-row'],
    });
  });
  const second = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: ['owner-B-row'] });
  });
  exchange.on('callMethod', first);
  await exchange.loadCapabilities();
  expect(first).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ method: 'fetchEngineCapabilities' }));
  first.mockClear();
  if (transition === 'unwired-switch') exchange.off('callMethod', first);
  const opts = { symbol: 'BTCUSDT', from: 1, to: 2, accountName: 'fixture-account' };
  const pending = exchange.fetchClosedPnl(opts).catch((error: unknown) => error);
  expect(first).not.toHaveBeenCalled();
  if (transition !== 'same-owner') {
    owner = { ownerId: 'B', generation: 2 };
    if (transition === 'ready-switch') exchange.off('callMethod', first);
    exchange.on('callMethod', second);
  }
  await vi.advanceTimersByTimeAsync(25);
  if (hooked && transition !== 'same-owner') {
    const failure = await pending;
    expect(failure).toMatchObject({ code: 'WORKER_STOPPED', command: 'fetchClosedPnl' });
    expect(failure).not.toHaveProperty('outcome');
    expect(first).not.toHaveBeenCalled();
    expect(second).not.toHaveBeenCalled();
    await expect(exchange.fetchClosedPnl(opts)).resolves.toEqual(['owner-B-row']);
    expect(second).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ method: 'fetchClosedPnl', args: [opts] }));
  } else {
    expect(await pending).toEqual([transition === 'same-owner' ? 'owner-A-row' : 'owner-B-row']);
    expect(transition === 'same-owner' ? first : second).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ method: 'fetchClosedPnl', args: [opts] }),
    );
    expect(transition === 'same-owner' ? second : first).not.toHaveBeenCalled();
  }
});

it.each(['same-owner', 'other-owner', 'new-generation', 'logout', 'new-provider'] as const)(
  'checks a bound owner while the initial bridge attaches (%s)', async (transition) => {
    vi.useFakeTimers();
    const exchange = account(true, { publicDataOnly: true, workerMirror: true }) as RustEngineExchange;
    let owner: { ownerId: string; generation: number } | undefined = { ownerId: 'A', generation: 1 };
    exchange.setWorkerOwner(() => owner);
    const pending = exchange.setLeverage('BTCUSDT', 3).catch((error: unknown) => error);
    if (transition === 'other-owner') owner!.ownerId = 'B';
    if (transition === 'new-generation') owner!.generation = 2;
    if (transition === 'logout') owner = undefined;
    if (transition === 'new-provider') exchange.setWorkerOwner(() => owner);
    const current = vi.fn(({ requestId }: { requestId: string }) => {
      exchange.emitter.emit('callMethod:done', { requestId, success: true, result: 'current-owner' });
    });
    exchange.on('callMethod', current);
    await vi.advanceTimersByTimeAsync(25);
    if (transition === 'same-owner') {
      expect(await pending).toBe('current-owner');
      expect(current).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
        method: 'setLeverage', args: ['BTCUSDT', 3],
      }));
    } else {
      expect(await pending).toMatchObject({ command: 'setLeverage', outcome: 'notSent' });
      expect(current).not.toHaveBeenCalled();
      if (transition !== 'logout') {
        await expect(exchange.setLeverage('BTCUSDT', 7)).resolves.toBe('current-owner');
        expect(current).toHaveBeenCalledOnce();
      }
    }
  },
);

it('keeps a bound owner through a detached bridge with unchanged bootstrap identity', async () => {
  vi.useFakeTimers();
  const exchange = account(true, { publicDataOnly: true, workerMirror: true }) as RustEngineExchange;
  const first = vi.fn();
  exchange.setWorkerOwner(() => ({ ownerId: 'A', generation: 1 }));
  exchange.on('callMethod', first);
  exchange.off('callMethod', first);
  const pending = exchange.setLeverage('BTCUSDT', 3);
  const reattached = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: 'owner-A' });
  });
  exchange.on('callMethod', reattached);
  await vi.advanceTimersByTimeAsync(25);
  await expect(pending).resolves.toBe('owner-A');
  expect(first).not.toHaveBeenCalled();
  expect(reattached).toHaveBeenCalledOnce();
});

it('drops a retired bound call at the next poll before the replacement bridge exists', async () => {
  vi.useFakeTimers();
  const exchange = account(true, { publicDataOnly: true, workerMirror: true }) as RustEngineExchange;
  let owner = { ownerId: 'A', generation: 1 };
  exchange.setWorkerOwner(() => owner);
  const settled = vi.fn();
  const pending = exchange.setLeverage('BTCUSDT', 3).then(settled, settled);
  owner = { ownerId: 'B', generation: 2 };
  await vi.advanceTimersByTimeAsync(25);
  expect(settled).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
    command: 'setLeverage', outcome: 'notSent',
  }));
  await pending;
  const current = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', { requestId, success: true, result: 'owner-B' });
  });
  exchange.on('callMethod', current);
  await expect(exchange.setLeverage('BTCUSDT', 7)).resolves.toBe('owner-B');
  expect(current).toHaveBeenCalledOnce();
});

it.each([
  { keyed: false, hooked: false }, { keyed: true, hooked: false },
  { keyed: false, hooked: true }, { keyed: true, hooked: true },
])('keeps placement startup compatibility (keyed=$keyed, hooked=$hooked)', async ({ keyed, hooked }) => {
  for (const switchOwner of [false, true]) {
    let rejectStartup!: (error: Error) => void;
    setWasmEngineLoader(() => new Promise<WasmEngineModule>((_resolve, reject) => { rejectStartup = reject; }));
    const exchange = account(true, {
      key: keyed ? 'fixture' : '', secret: keyed ? 'fixture' : '',
      publicDataOnly: !keyed, workerMirror: true,
    });
    if (hooked) (exchange as RustEngineExchange).setWorkerOwner(() => ({ ownerId: 'A', generation: 1 }));
    const ownerA = vi.fn(({ requestId }: { requestId: string }) => {
      exchange.emitter.emit('callMethod:done', { requestId, success: true, result: ['owner-A-order'] });
    });
    const ownerB = vi.fn(({ requestId }: { requestId: string }) => {
      exchange.emitter.emit('callMethod:done', { requestId, success: true, result: ['owner-B-order'] });
    });
    exchange.on('callMethod', ownerA);
    const pending = exchange.placeOrders([{
      symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 1, price: 100,
      clientOrderId: 'admitted-A',
    }]).catch((error: unknown) => error);
    expect(ownerA).not.toHaveBeenCalled();
    if (switchOwner) {
      exchange.off('callMethod', ownerA);
      exchange.on('callMethod', ownerB);
    }
    if (keyed) rejectStartup(new Error('controlled page-engine fallback'));
    if (switchOwner && hooked) {
      expect(await pending).toMatchObject({ command: 'placeOrders', outcome: 'notSent', clientOrderIds: ['admitted-A'] });
      expect(ownerA).not.toHaveBeenCalled();
      expect(ownerB).not.toHaveBeenCalled();
      await expect(exchange.placeOrders([{
        symbol: 'BTCUSDT', side: OrderSide.Buy, type: OrderType.Limit, amount: 2, price: 100,
      }])).resolves.toEqual(['owner-B-order']);
      expect(ownerB).toHaveBeenCalledOnce();
    } else {
      expect(await pending).toEqual([switchOwner ? 'owner-B-order' : 'owner-A-order']);
      expect(switchOwner ? ownerB : ownerA).toHaveBeenCalledOnce();
      expect(switchOwner ? ownerA : ownerB).not.toHaveBeenCalled();
    }
    exchange.dispose();
  }
});

it.each([false, true])('keeps source order refresh compatibility (hooked=%s)', async (hooked) => {
  for (const switchOwner of [false, true]) {
    const exchange = account(true, { publicDataOnly: true, workerMirror: true }) as RustEngineExchange;
    if (hooked) exchange.setWorkerOwner(() => ({ ownerId: 'A', generation: 1 }));
    const ownerA = vi.fn(({ requestId }: { requestId: string }) => {
      exchange.emitter.emit('callMethod:done', { requestId, success: true, result: undefined });
    });
    const ownerB = vi.fn(({ requestId }: { requestId: string }) => {
      exchange.emitter.emit('callMethod:done', { requestId, success: true, result: undefined });
    });
    exchange.on('callMethod', ownerA);
    const args = [['BTCUSDT'], [{ id: 'synthetic-A-order', symbol: 'BTCUSDT' }]] as const;
    const pending = exchange.refreshAccountOrders([...args[0]], [...args[1]])
      .then(() => 'dispatched', () => 'dropped');
    if (switchOwner) {
      exchange.off('callMethod', ownerA);
      exchange.on('callMethod', ownerB);
    }
    if (switchOwner && hooked) {
      expect(await pending).toBe('dropped');
      expect(ownerA).not.toHaveBeenCalled();
      expect(ownerB).not.toHaveBeenCalled();
    } else {
      expect(await pending).toBe('dispatched');
      expect(switchOwner ? ownerB : ownerA).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
        method: 'refreshAccountOrders', args,
      }));
      expect(switchOwner ? ownerA : ownerB).not.toHaveBeenCalled();
    }
  }
});

it('retries a worker candle read once on the replacement bridge and ignores the old reply', async () => {
  vi.useFakeTimers();
  // Lighter explicitly fetches candles in the worker.
  const lighter = new RustEngineExchange(
    'lighter',
    { key: '', secret: '', publicDataOnly: true, workerMirror: true },
    new DefaultStore(),
  );
  accounts.push(lighter);
  const first = vi.fn();
  lighter.on('callMethod', first);
  const pending = lighter.callMethodInWorker('fetchOHLCV', { symbol: 'BTCUSDT', interval: '15m' });
  void pending.catch(() => {});
  await vi.advanceTimersByTimeAsync(0);
  expect(first).toHaveBeenCalledOnce();
  lighter.off('callMethod', first);
  await vi.advanceTimersByTimeAsync(100);
  const second = vi.fn(({ requestId }: { requestId: string }) => {
    lighter.emitter.emit('callMethod:done', {
      requestId: first.mock.calls[0][0].requestId,
      success: true,
      result: ['stale'],
    });
    lighter.emitter.emit('callMethod:done', { requestId, success: true, result: [] });
  });
  lighter.on('callMethod', second);
  await vi.advanceTimersByTimeAsync(100);
  await expect(pending).resolves.toEqual([]);
  expect(second).toHaveBeenCalledOnce();
});

it('bounds the replacement bridge wait and never replays an interrupted write', async () => {
  vi.useFakeTimers();
  const exchange = new RustEngineExchange(
    'lighter',
    { key: '', secret: '', publicDataOnly: true, workerMirror: true },
    new DefaultStore(),
  );
  accounts.push(exchange);
  const first = vi.fn();
  exchange.on('callMethod', first);
  const candle = exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '15m' }).catch((error: unknown) => error);
  const write = exchange.setLeverage('BTCUSDT', 3).catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(0);
  exchange.off('callMethod', first);
  await vi.advanceTimersByTimeAsync(30_100);
  expect(await candle).toMatchObject({ message: 'lighter worker bridge was not wired' });
  expect(await write).toMatchObject({ code: 'COMMAND_TIMEOUT', outcome: 'unknown' });
  const replacement = vi.fn();
  exchange.on('callMethod', replacement);
  await vi.advanceTimersByTimeAsync(100);
  expect(replacement).not.toHaveBeenCalled();
});

it('does not retry a second interrupted worker candle read', async () => {
  vi.useFakeTimers();
  const exchange = new RustEngineExchange(
    'lighter',
    { key: '', secret: '', publicDataOnly: true, workerMirror: true },
    new DefaultStore(),
  );
  accounts.push(exchange);
  const first = vi.fn();
  exchange.on('callMethod', first);
  const pending = exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '15m' }).catch((error: unknown) => error);
  await vi.advanceTimersByTimeAsync(0);
  exchange.off('callMethod', first);
  const second = vi.fn();
  exchange.on('callMethod', second);
  await vi.advanceTimersByTimeAsync(100);
  expect(second).toHaveBeenCalledOnce();
  exchange.off('callMethod', second);
  await expect(pending).resolves.toMatchObject({ code: 'WORKER_STOPPED' });
});

it('does not retry a worker candle refusal', async () => {
  const exchange = new RustEngineExchange(
    'lighter',
    { key: '', secret: '', publicDataOnly: true, workerMirror: true },
    new DefaultStore(),
  );
  accounts.push(exchange);
  const bridge = vi.fn(({ requestId }: { requestId: string }) => {
    exchange.emitter.emit('callMethod:done', {
      requestId,
      success: false,
      result: { message: 'invalid interval', kind: 'invalidInput' },
    });
  });
  exchange.on('callMethod', bridge);
  await expect(exchange.fetchOHLCV({ symbol: 'BTCUSDT', interval: '15m' })).rejects.toMatchObject({
    message: 'invalid interval',
    kind: 'invalidInput',
  });
  expect(bridge).toHaveBeenCalledOnce();
});
