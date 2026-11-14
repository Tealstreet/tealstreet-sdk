import type { PlaceOrderOpts } from '../../types';
import type { StoreWorkerPayload } from '../StoreWorkerPayload';
import type { TwapOptions } from '../twap/TwapOptions';
import type { TwapOwner as Owner, TwapWorkerContext } from '../twap/TwapOwner';
import type { TwapState } from '../twap/TwapState';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { handlerArgs, rustAlgoExchange } from '../../__tests__/fixtures/rustAlgoExchange';
import nativeOwnerState from '../../__tests__/fixtures/twap-owner-state.json';
import { OrderSide } from '../../types';
import { TwapInstance } from '../twap/TwapInstance';
import { setTwapLockManager } from '../twap/twapLock';
import { TwapStatus } from '../twap/TwapStatus';
import { TwapHandler } from './TwapHandler';

const deferred = () => {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
type Restored = { id: string; ownerId: string; options: TwapOptions; state: TwapState };

const options: TwapOptions = {
  accountName: 'test-account',
  activeMarket: {
    id: 'BTCUSDT',
    symbol: 'BTCUSDT',
    base: 'BTC',
    quote: 'USDT',
    active: true,
    precision: { amount: 0.001, price: 0.01 },
    limits: { amount: { min: 0.001, max: 100 }, leverage: { min: 1, max: 100 } },
  },
  size: 0.3,
  duration: 0.01,
  ordersCount: 2,
  side: OrderSide.Buy,
  chaserDelay: 1000,
  reduceOnly: false,
  randomnessPercentage: 0,
  useLimitOrders: false,
  useChaserOrders: false,
};

const restored = (reconcile: boolean): Restored => ({
  id: 'retained-A',
  ownerId: 'A',
  options,
  state: {
    accountName: options.accountName,
    activeMarket: options.activeMarket,
    size: 0.3,
    side: OrderSide.Buy,
    status: TwapStatus.Running,
    ordersSizes: reconcile ? [0.15] : [0.15, 0.15],
    ordersCount: 2,
    ordersExecuted: 0,
    nextOrderAt: Date.now() + 100,
    ...(reconcile ? { inFlight: { size: 0.15, clientOrderId: 'pending-A' } } : {}),
  },
});

let handler: TwapHandler;
let executors: TwapInstance[];

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(1_700_000_000_000);
  executors = [];
});
afterEach(async () => {
  handler?.dispose();
  for (const executor of executors) await executor.terminate();
  setTwapLockManager(undefined);
  vi.useRealTimers();
});

const harness = (lookupOutcome: 'missing' | 'uncertain' | 'failed' = 'missing', withOwnerCheck = true) => {
  let owner: Owner | null = { ownerId: 'A', generation: 1 };
  const lockEntered = deferred(),
    lockGrant = deferred();
  const lookupEntered = deferred(),
    lookupReply = deferred();
  let held = false;
  setTwapLockManager({
    async request(_name, _options, callback) {
      lockEntered.resolve();
      await lockGrant.promise;
      held = true;
      try {
        await callback({});
      } finally {
        held = false;
      }
    },
  });
  const exchange = rustAlgoExchange();
  const slices: { amount: number; at: number }[] = [];
  exchange.placeOrder = vi.fn(async (order: PlaceOrderOpts) => {
    slices.push({ amount: order.amount, at: Date.now() });
    return ['slice'];
  });
  exchange.findOrderByClientOrderId = vi.fn(async () => {
    lookupEntered.resolve();
    await lookupReply.promise;
    if (lookupOutcome === 'failed') throw new Error('synthetic lookup failure');
    return { found: false, definitive: lookupOutcome === 'missing' } as const;
  });
  const postMessage = vi.fn();
  const context = {
    postMessage,
    ...(withOwnerCheck ? { getTwapOwner: () => owner } : {}),
  } as unknown as TwapWorkerContext;
  const adopted: Restored[] = [];
  exchange.algoOp.mockImplementation((op: string, data: unknown) => {
    if (op !== 'twap:state') return;
    for (const row of data as Restored[]) {
      adopted.push(structuredClone(row));
      exchange.listeners.get('twap:stateUpdate')?.({ [row.id]: row });
      const instance = new TwapInstance(exchange, context, row.options, () => {}, row.state, row.id);
      executors.push(instance);
      void instance.start();
    }
  });
  handler = new TwapHandler(...handlerArgs(exchange, context));
  return {
    exchange,
    adopted,
    slices,
    postMessage,
    lockEntered,
    lockGrant,
    lookupEntered,
    lookupReply,
    setOwner: (next: Owner | null) => {
      owner = next;
    },
    held: () => held,
  };
};

describe.each(['lock', 'reconciliation'] as const)('TWAP owner change during %s wait', (phase) => {
  it.each([
    ['B', { ownerId: 'B', generation: 2 }],
    ['disrupt:A', { ownerId: 'disrupt:A', generation: 2 }],
    ['logout', null],
    ['new A session', { ownerId: 'A', generation: 2 }],
  ] as const)('cancels an A restore after switching to %s, with zero slices', async (_name, next) => {
    const h = harness();
    handler.handle({ op: 'twap:state', data: [restored(phase === 'reconciliation')] } as StoreWorkerPayload);
    await h.lockEntered.promise;
    if (phase === 'reconciliation') {
      h.lockGrant.resolve();
      await h.lookupEntered.promise;
    }
    h.setOwner(next);
    h.lockGrant.resolve();
    h.lookupReply.resolve();
    await vi.advanceTimersByTimeAsync(1000);
    expect(h.slices).toEqual([]);
    expect(h.exchange.algoOp).not.toHaveBeenCalled();
    expect(h.held()).toBe(false);
    expect(h.postMessage).toHaveBeenLastCalledWith({ op: 'twap:stateUpdate', data: {} });
    expect(h.exchange.emitter.emit).toHaveBeenCalledWith('warning', 'TWAP BTCUSDT restore cancelled-for-owner-change');
  });

  it('keeps the same owner slice amounts and interval', async () => {
    const h = harness();
    handler.handle({ op: 'twap:state', data: [restored(phase === 'reconciliation')] } as StoreWorkerPayload);
    await h.lockEntered.promise;
    if (phase === 'reconciliation') {
      h.lockGrant.resolve();
      await h.lookupEntered.promise;
    }
    const releasedAt = Date.now();
    h.lockGrant.resolve();
    h.lookupReply.resolve();
    await vi.advanceTimersByTimeAsync(1000);
    expect(h.slices).toEqual([
      { amount: 0.15, at: releasedAt },
      { amount: 0.15, at: releasedAt + 300 },
    ]);
    expect(h.exchange.emitter.emit).not.toHaveBeenCalled();
  });
});

it('captures the owner before the engine-readiness queue, and drops a stale queued restore', async () => {
  const h = harness();
  const queued: (() => void)[] = [];
  h.exchange.routeAlgoOp.mockImplementation((_account, _op, run) => {
    queued.push(run);
  });
  handler.handle({ op: 'twap:state', data: [restored(false)] } as StoreWorkerPayload);
  h.setOwner({ ownerId: 'B', generation: 2 });
  queued[0]();
  await vi.advanceTimersByTimeAsync(1000);
  expect(h.slices).toEqual([]);
  expect(h.exchange.algoOp).not.toHaveBeenCalled();
  expect(h.exchange.findOrderByClientOrderId).not.toHaveBeenCalled();
  expect(h.exchange.emitter.emit).toHaveBeenCalledWith('warning', 'TWAP BTCUSDT restore cancelled-for-owner-change');
});

it.each([undefined, 0, 1, 7])('preserves persisted generation %s in waiting and dispatched rows without publishing the session snapshot', async (ownerGeneration) => {
  const h = harness();
  const row = { ...restored(false), ...(ownerGeneration !== undefined ? { ownerGeneration } : {}) };
  const expected = structuredClone(row);
  handler.handle({ op: 'twap:state', data: [row] } as StoreWorkerPayload);
  await h.lockEntered.promise;
  h.exchange.listeners.get('twap:stateUpdate')?.({});
  const { id, ...pending } = expected;
  expect(h.postMessage).toHaveBeenLastCalledWith({
    op: 'twap:stateUpdate',
    data: { [id]: pending },
  });
  h.setOwner({ ownerId: 'A', generation: 1 });
  h.lockGrant.resolve();
  await vi.advanceTimersByTimeAsync(0);
  expect(h.adopted).toEqual([expected]);
});

it('does not reuse a mutated live session snapshot', async () => {
  const h = harness();
  const live = { ownerId: 'A', generation: 1 };
  h.setOwner(live);
  handler.handle({ op: 'twap:state', data: [restored(false)] } as StoreWorkerPayload);
  await h.lockEntered.promise;
  live.generation++;
  h.lockGrant.resolve();
  await vi.advanceTimersByTimeAsync(1000);
  expect(h.slices).toEqual([]);
  expect(h.held()).toBe(false);
});

it('does not revive an old A restore after A switches to B and logs back in', async () => {
  const h = harness();
  handler.handle({ op: 'twap:state', data: [restored(false)] } as StoreWorkerPayload);
  await h.lockEntered.promise;
  h.setOwner({ ownerId: 'B', generation: 2 });
  h.setOwner({ ownerId: 'A', generation: 3 });
  h.lockGrant.resolve();
  await vi.advanceTimersByTimeAsync(1000);
  expect(h.slices).toEqual([]);
  expect(h.held()).toBe(false);
});

it.each(['uncertain', 'failed'] as const)(
  'suppresses a stale reconciliation %s warning after logout',
  async (outcome) => {
    const h = harness(outcome);
    handler.handle({ op: 'twap:state', data: [restored(true)] } as StoreWorkerPayload);
    h.lockGrant.resolve();
    await h.lookupEntered.promise;
    h.setOwner(null);
    h.lookupReply.resolve();
    await vi.advanceTimersByTimeAsync(1000);
    expect(h.slices).toEqual([]);
    expect(h.exchange.emitter.emit.mock.calls).toEqual([
      ['warning', 'TWAP BTCUSDT restore cancelled-for-owner-change'],
    ]);
  },
);

it('fails closed for an owned row when the host supplies no owner check', async () => {
  const h = harness('missing', false);
  handler.handle({ op: 'twap:state', data: [restored(false)] } as StoreWorkerPayload);
  h.lockGrant.resolve();
  await vi.advanceTimersByTimeAsync(1000);
  expect(h.slices).toEqual([]);
  expect(h.exchange.algoOp).not.toHaveBeenCalled();
  expect(h.exchange.emitter.emit).toHaveBeenCalledWith('warning', 'TWAP BTCUSDT restore cancelled-for-owner-change');
});

it.each(['A', 'B'])('refuses an untagged row under owner %s without inventing ownership', async (ownerId) => {
  const h = harness();
  h.setOwner({ ownerId, generation: 2 });
  const { ownerId: _oldOwner, ...untagged } = restored(false);
  handler.handle({ op: 'twap:state', data: [untagged] } as StoreWorkerPayload);
  h.lockGrant.resolve();
  await vi.advanceTimersByTimeAsync(1000);
  expect(h.slices).toEqual([]);
  expect(h.adopted).toEqual([]);
  expect(h.exchange.emitter.emit).toHaveBeenCalledWith(
    'warning',
    'TWAP BTCUSDT restore cancelled-for-owner-change: persisted ownerId is missing',
  );
});

it('keeps an A native persistence snapshot tagged through receive before B tries to restore it', async () => {
  const a = harness();
  // Rust's restored_owner_round_trip_matches_the_worker_fixture pins this exact serialize_state output.
  a.exchange.listeners.get('twap:stateUpdate')?.(structuredClone(nativeOwnerState));
  expect(a.postMessage).toHaveBeenLastCalledWith({ op: 'twap:stateUpdate', data: nativeOwnerState });
  handler.dispose();
  a.lockGrant.resolve();
  const b = harness();
  b.setOwner({ ownerId: 'B', generation: 2 });
  const rows = Object.entries(nativeOwnerState).map(([id, row]) => ({ id, ...row }));
  handler.handle({ op: 'twap:state', data: rows } as StoreWorkerPayload);
  b.lockGrant.resolve();
  await vi.advanceTimersByTimeAsync(1000);
  expect(b.slices).toEqual([]);
  expect(b.adopted).toEqual([]);
  expect(b.exchange.emitter.emit).toHaveBeenCalledWith('warning', 'TWAP BTCUSDT restore cancelled-for-owner-change');
});

it('restores the native tagged A snapshot for the same owner with unchanged slices', async () => {
  const h = harness();
  const rows = Object.entries(structuredClone(nativeOwnerState)).map(([id, row]) => ({ id, ...row }));
  handler.handle({ op: 'twap:state', data: rows } as StoreWorkerPayload);
  h.lockGrant.resolve();
  await vi.advanceTimersByTimeAsync(1000);
  expect(h.slices).toEqual([
    { amount: 0.15, at: 1700000000000 },
    { amount: 0.15, at: 1700000000300 },
  ]);
  expect(h.exchange.emitter.emit).not.toHaveBeenCalled();
});

it('keeps untagged restores only in a legacy single-user context without a provider', async () => {
  const h = harness('missing', false);
  const { ownerId: _owner, ...untagged } = restored(false);
  handler.handle({ op: 'twap:state', data: [untagged] } as StoreWorkerPayload);
  h.lockGrant.resolve();
  await vi.advanceTimersByTimeAsync(1000);
  expect(h.slices).toEqual([
    { amount: 0.15, at: 1700000000000 },
    { amount: 0.15, at: 1700000000300 },
  ]);
  expect(h.adopted[0]).not.toHaveProperty('ownerId');
});
