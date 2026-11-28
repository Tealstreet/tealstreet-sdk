import type { Exchange } from '../../exchanges/base';
import type { TwapLockManager } from './twapLock';
import type { TwapOwner } from './TwapOwner';

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { OrderSide } from '../../types';
import { RustTwapOwnership } from './RustTwapOwnership';
import { setTwapLockManager, TWAP_RESTORE_LOCK_WAIT_MS, twapLockName } from './twapLock';
import { TwapStatus } from './TwapStatus';

/** Web Locks of one origin, shared by every tab's worker. */
const held = new Map<string, string>();
const locksFor = (worker: string): TwapLockManager => ({
  async request(name, _options, callback) {
    if (held.has(name)) return callback(null);
    held.set(name, worker);
    try {
      await callback({ name });
    } finally {
      if (held.get(name) === worker) held.delete(name);
    }
  },
});

const options = {
  accountName: 'acct',
  activeMarket: { symbol: 'BTCUSDT' },
  size: 1,
  duration: 10,
  ordersCount: 4,
  side: OrderSide.Buy,
  randomnessPercentage: 0,
};
const row = (id: string, state: Record<string, unknown> = {}) => ({
  id,
  options,
  state: {
    accountName: 'acct',
    activeMarket: { symbol: 'BTCUSDT' },
    size: 1,
    ordersSizes: [0.25, 0.25],
    side: OrderSide.Buy,
    status: TwapStatus.Running,
    ordersCount: 4,
    ordersExecuted: 1,
    nextOrderAt: Date.now() + 5_000,
    ...state,
  },
});
const persisted = (...rows: ReturnType<typeof row>[]) => Object.fromEntries(rows.map(({ id, ...rest }) => [id, rest]));

let errors: string[];
let sent: { op: string; data: any }[];
let posts: Record<string, unknown>[];
let lookup: ReturnType<typeof vi.fn>;
let ownership: RustTwapOwnership;

/** The algos adopt every restored row and persist it, as `TwapBook::restore` does. */
const algos = () => {
  const rows: Record<string, unknown> = {};
  return (op: string, data: any) => {
    sent.push({ op, data });
    if (op === 'twap:state') for (const { id, ...rest } of data) rows[id] = rest;
    if (op === 'twap:stop') delete rows[data];
    ownership.receive({ ...rows });
  };
};

beforeEach(() => {
  vi.useFakeTimers();
  held.clear();
  setTwapLockManager(locksFor('this-tab'));
  errors = [];
  sent = [];
  posts = [];
  lookup = vi.fn();
  const exchange = {
    emitter: { emit: (op: string, message: string) => op === 'error' && errors.push(message) },
    findOrderByClientOrderId: lookup,
  } as unknown as Exchange;
  ownership = new RustTwapOwnership(exchange, algos(), (rows) => posts.push(rows));
});
afterEach(() => {
  setTwapLockManager(undefined);
  vi.useRealTimers();
});

describe('RustTwapOwnership restore', () => {
  it('sends a row to the algos only once it holds the row lock, and keeps holding it', async () => {
    ownership.handle('twap:state', [row('t1')]);
    expect(sent).toEqual([]);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent).toEqual([{ op: 'twap:state', data: [row('t1')] }]);
    expect(held.get(twapLockName('acct', 't1'))).toBe('this-tab');
    expect(posts.at(-1)).toEqual(persisted(row('t1')));
  });

  it('refuses a row another tab runs, after the reload wait, and drops it from the persisted rows', async () => {
    held.set(twapLockName('acct', 't1'), 'other-tab');
    ownership.handle('twap:state', [row('t1')]);
    // Still waiting: the row stays persisted so a dying worker does not lose it.
    ownership.receive({});
    expect(posts.at(-1)).toEqual(persisted(row('t1')));
    await vi.advanceTimersByTimeAsync(TWAP_RESTORE_LOCK_WAIT_MS + 500);
    expect(sent).toEqual([]);
    expect(errors).toEqual(['TWAP BTCUSDT not restored: it is running in another tab']);
    expect(posts.at(-1)).toEqual({});
  });

  it('runs a row whose lock its own reloaded tab releases within the wait', async () => {
    held.set(twapLockName('acct', 't1'), 'previous-worker');
    ownership.handle('twap:state', [row('t1')]);
    await vi.advanceTimersByTimeAsync(1_000);
    held.delete(twapLockName('acct', 't1'));
    await vi.advanceTimersByTimeAsync(500);
    expect(sent.map((message) => message.op)).toEqual(['twap:state']);
    expect(errors).toEqual([]);
  });

  it('refuses a stale row by the restore rules and rewrites the persisted rows', () => {
    ownership.handle('twap:state', [row('done', { status: TwapStatus.Completed })]);
    expect(errors).toEqual(['TWAP BTCUSDT not restored: it already completed']);
    expect(posts).toEqual([{}]);
    expect(sent).toEqual([]);
  });

  it('counts an in-flight slice found by its client id as executed', async () => {
    lookup.mockResolvedValue({ found: true, order: { id: 'o1' } });
    ownership.handle('twap:state', [row('t1', { inFlight: { size: 0.25, clientOrderId: 'c1' } })]);
    await vi.advanceTimersByTimeAsync(0);
    expect(lookup).toHaveBeenCalledWith('BTCUSDT', 'c1');
    expect(sent[0].data[0].state).toEqual(row('t1', { ordersExecuted: 2 }).state);
    expect(errors).toEqual([]);
  });

  it('places a definitively missing in-flight slice again', async () => {
    lookup.mockResolvedValue({ found: false, definitive: true });
    ownership.handle('twap:state', [row('t1', { inFlight: { size: 0.25, clientOrderId: 'c1' } })]);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent[0].data[0].state).toEqual(row('t1', { ordersSizes: [0.25, 0.25, 0.25] }).state);
  });

  it.each([
    [
      'an inconclusive lookup',
      { clientOrderId: 'c1' },
      () => lookup.mockResolvedValue({ found: false, definitive: false }),
    ],
    ['a failed lookup', { clientOrderId: 'c1' }, () => lookup.mockRejectedValue(new Error('offline'))],
    ['no client id', {}, () => {}],
  ])('counts an unconfirmable in-flight slice after %s, with a warning', async (_case, identity, arrange) => {
    arrange();
    ownership.handle('twap:state', [row('t1', { inFlight: { size: 0.25, ...identity } })]);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent[0].data[0].state).toEqual(row('t1', { ordersExecuted: 2 }).state);
    expect(errors).toEqual([
      'TWAP BTCUSDT: a 0.25 slice was being placed when the page closed and could not be confirmed; it is not placed again. Check your orders.',
    ]);
  });

  it('completes a TWAP whose last slice was the one in flight without sending it', async () => {
    lookup.mockResolvedValue({ found: true, order: { id: 'o1' } });
    ownership.handle('twap:state', [row('t1', { ordersSizes: [], inFlight: { size: 0.25, clientOrderId: 'c1' } })]);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent).toEqual([]);
    expect(posts.at(-1)).toEqual({});
    expect(held.size).toBe(0);
  });

  it('restores a TWAP in its last interval so it can cancel its resting limit slices', async () => {
    const limitSlices = [{ orderIds: ['o1'], price: 100, size: 0.25 }];
    ownership.handle('twap:state', [row('t1', { ordersSizes: [], ordersExecuted: 4, limitSlices })]);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent[0].data[0].state.limitSlices).toEqual(limitSlices);
    expect(errors).toEqual([]);
  });

  it('adopts a found in-flight limit slice so the TWAP cancels it at its end', async () => {
    lookup.mockResolvedValue({ found: true, order: { id: 'o9', price: 100, type: 'limit' } });
    const limit = {
      ...row('t1', { ordersSizes: [], inFlight: { size: 0.25, clientOrderId: 'c1' } }),
      options: { ...options, useLimitOrders: true },
    };
    ownership.handle('twap:state', [limit]);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent[0].data[0].state.limitSlices).toEqual([{ orderIds: ['o9'], price: 100, size: 0.25 }]);
    expect(sent[0].data[0].state.ordersExecuted).toBe(2);
  });

  it('restores a row stopped while it waited, paused then stopped, when it holds limit slices', async () => {
    const limitSlices = [{ orderIds: ['o1'], price: 100, size: 0.25 }];
    let releaseLookup!: (value: unknown) => void;
    lookup.mockReturnValue(new Promise((resolve) => (releaseLookup = resolve)));
    ownership.handle('twap:state', [row('t1', { inFlight: { size: 0.25, clientOrderId: 'c1' }, limitSlices })]);
    await vi.advanceTimersByTimeAsync(0);
    ownership.handle('twap:stop', 't1');
    expect(posts.at(-1)).toHaveProperty('t1.stopped', true);
    releaseLookup({ found: false, definitive: true });
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.map(({ op }) => op)).toEqual(['twap:state', 'twap:stop']);
    expect(sent[0].data[0].state).toMatchObject({ status: TwapStatus.Paused, limitSlices });
    expect(sent[0].data[0]).not.toHaveProperty('stopped');
  });

  it('stops a restore placing its only limit slice: the found order is cancelled, not left', async () => {
    let releaseLookup!: (value: unknown) => void;
    lookup.mockReturnValue(new Promise((resolve) => (releaseLookup = resolve)));
    const limit = {
      ...row('t1', { ordersSizes: [], ordersExecuted: 3, inFlight: { size: 0.25, clientOrderId: 'c1', price: 99 } }),
      options: { ...options, useLimitOrders: true },
    };
    ownership.handle('twap:state', [limit]);
    await vi.advanceTimersByTimeAsync(0);
    ownership.handle('twap:stop', 't1');
    releaseLookup({ found: true, order: { id: 'o9', price: 0, type: 'limit' } });
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.map(({ op }) => op)).toEqual(['twap:state', 'twap:stop']);
    // The venue row carried no price: the slice's own is kept.
    expect(sent[0].data[0].state.limitSlices).toEqual([{ orderIds: ['o9'], price: 99, size: 0.25 }]);
  });

  it('drops a stopped restore whose in-flight slice was definitively never placed', async () => {
    let releaseLookup!: (value: unknown) => void;
    lookup.mockReturnValue(new Promise((resolve) => (releaseLookup = resolve)));
    const limit = {
      ...row('t1', { inFlight: { size: 0.25, clientOrderId: 'c1' } }),
      options: { ...options, useLimitOrders: true },
    };
    ownership.handle('twap:state', [limit]);
    await vi.advanceTimersByTimeAsync(0);
    ownership.handle('twap:stop', 't1');
    releaseLookup({ found: false, definitive: true });
    await vi.advanceTimersByTimeAsync(0);
    expect(sent).toEqual([]);
    expect(posts.at(-1)).toEqual({});
  });

  it('sends a persisted stopped row paused, then stops it, after a reload', async () => {
    const limitSlices = [{ orderIds: ['o1'], price: 100, size: 0.25 }];
    ownership.handle('twap:state', [{ ...row('t1', { status: TwapStatus.Paused, limitSlices }), stopped: true }]);
    await vi.advanceTimersByTimeAsync(0);
    expect(sent.map(({ op }) => op)).toEqual(['twap:state', 'twap:stop']);
  });

  it('names an order found during the lookup when the owner changed meanwhile', async () => {
    let owner = { ownerId: 'A', generation: 1 };
    let releaseLookup!: (value: unknown) => void;
    lookup.mockReturnValue(new Promise((resolve) => (releaseLookup = resolve)));
    const exchange = {
      emitter: { emit: (_op: string, message: string) => errors.push(message) },
      findOrderByClientOrderId: lookup,
    } as unknown as Exchange;
    const owned = new RustTwapOwnership(exchange, algos(), (rows) => posts.push(rows), () => owner);
    const limit = {
      ...row('t1', { inFlight: { size: 0.25, clientOrderId: 'c1' } }),
      ownerId: 'A',
      ownerGeneration: 1,
      options: { ...options, useLimitOrders: true },
    };
    owned.handle('twap:state', [limit]);
    await vi.advanceTimersByTimeAsync(0);
    owner = { ownerId: 'B', generation: 1 };
    releaseLookup({ found: true, order: { id: 'o9', price: 100, type: 'limit' } });
    await vi.advanceTimersByTimeAsync(0);
    expect(sent).toEqual([]);
    expect(errors).toEqual([
      'TWAP BTCUSDT wasn’t restored because the account connection changed. Its limit orders are still open: buy 0.25 at 100 (order o9).',
    ]);
  });

  it('names an in-flight limit slice of a row dropped for an owner change before its lookup', () => {
    const ownerless = new RustTwapOwnership(
      { emitter: { emit: (_op: string, message: string) => errors.push(message) } } as unknown as Exchange,
      algos(),
      (rows) => posts.push(rows),
      () => ({ ownerId: 'A', generation: 1 }),
    );
    const limit = {
      ...row('t1', { inFlight: { size: 0.25, clientOrderId: 'c1', price: 99 } }),
      options: { ...options, useLimitOrders: true },
    };
    ownerless.handle('twap:state', [limit]);
    expect(errors).toEqual([
      'TWAP BTCUSDT wasn’t restored because the account connection changed. A buy 0.25 at 99 limit slice (client id c1) was being placed and couldn’t be confirmed; check your open orders.',
    ]);
  });

  it('names the slices of a restore still waiting when the engine dies', async () => {
    const limitSlices = [{ orderIds: ['o1'], price: 100, size: 0.25 }];
    lookup.mockReturnValue(new Promise(() => {}));
    ownership.handle('twap:state', [row('t1', { inFlight: { size: 0.25, clientOrderId: 'c1' }, limitSlices })]);
    await vi.advanceTimersByTimeAsync(0);
    ownership.finish();
    expect(errors).toEqual([
      'TWAP BTCUSDT stopped before it was restored. Its limit orders are still open: buy 0.25 at 100 (order o1).',
    ]);
  });

  it('names the resting slices of a row it drops for an owner change', () => {
    const limitSlices = [{ orderIds: ['o1'], price: 100, size: 0.25 }];
    const ownerless = new RustTwapOwnership(
      { emitter: { emit: (_op: string, message: string) => errors.push(message) } } as unknown as Exchange,
      algos(),
      (rows) => posts.push(rows),
      () => ({ ownerId: 'A', generation: 1 }),
    );
    ownerless.handle('twap:state', [row('t1', { limitSlices })]);
    expect(errors).toEqual([
      'TWAP BTCUSDT wasn’t restored because the account connection changed. Its limit orders are still open: buy 0.25 at 100 (order o1).',
    ]);
  });

  it('names the resting limit slices of a row it refuses', () => {
    const limitSlices = [{ orderIds: ['o1'], price: 100, size: 0.25 }];
    ownership.handle('twap:state', [row('old', { nextOrderAt: Date.now() - 3_600_000, limitSlices })]);
    expect(errors).toEqual([
      'TWAP BTCUSDT not restored: its saved schedule has expired. Its limit orders are still open: buy 0.25 at 100 (order o1)',
    ]);
    expect(sent).toEqual([]);
  });

  it('applies stop and pause to a row still waiting for its lock', async () => {
    held.set(twapLockName('acct', 't1'), 'previous-worker');
    held.set(twapLockName('acct', 't2'), 'previous-worker');
    const t2 = row('t2');
    ownership.handle('twap:state', [row('t1'), t2]);
    ownership.handle('twap:stop', 't1');
    ownership.handle('twap:pause', 't2');
    ownership.handle('twap:resume', 't2');
    const paused = { ...t2, state: { ...t2.state, status: TwapStatus.Paused } };
    expect(posts.at(-1)).toEqual(persisted(paused));
    held.clear();
    await vi.advanceTimersByTimeAsync(500);
    expect(sent).toEqual([{ op: 'twap:state', data: [paused] }]);
    expect(held.has(twapLockName('acct', 't1'))).toBe(false);
  });
});

describe('RustTwapOwnership started TWAPs', () => {
  it('takes a started TWAP lock and releases it once the TWAP leaves the persisted rows', async () => {
    ownership.receive(persisted(row('fresh')));
    await vi.advanceTimersByTimeAsync(0);
    expect(held.get(twapLockName('acct', 'fresh'))).toBe('this-tab');
    expect(posts.at(-1)).toEqual(persisted(row('fresh')));
    ownership.receive({});
    await vi.advanceTimersByTimeAsync(0);
    expect(held.size).toBe(0);
  });

  it('warns when a started TWAP lock is held elsewhere, once', async () => {
    held.set(twapLockName('acct', 'fresh'), 'other-tab');
    ownership.receive(persisted(row('fresh')));
    await vi.advanceTimersByTimeAsync(0);
    ownership.receive(persisted(row('fresh', { ordersExecuted: 2 })));
    await vi.advanceTimersByTimeAsync(0);
    expect(errors).toEqual(['TWAP BTCUSDT started without its cross-tab lock; a duplicated tab could also run it']);
  });
});

describe('RustTwapOwnership when the engine dies', () => {
  it('posts the final state without its TWAPs or waiting restores, and releases every lock', async () => {
    ownership.handle('twap:state', [row('t1')]);
    await vi.advanceTimersByTimeAsync(0);
    held.set(twapLockName('acct', 't2'), 'previous-worker');
    ownership.handle('twap:state', [row('t2')]);
    ownership.finish();
    expect(posts.at(-1)).toEqual({});
    await vi.advanceTimersByTimeAsync(0);
    expect(held.get(twapLockName('acct', 't1'))).toBeUndefined();
    held.clear();
    await vi.advanceTimersByTimeAsync(500);
    expect(sent.filter((message) => message.data[0]?.id === 't2')).toEqual([]);
    expect(held.size).toBe(0);
    const count = posts.length;
    ownership.receive(persisted(row('late')));
    expect(posts).toHaveLength(count);
  });

  it('posts nothing when no TWAP was persisted', () => {
    ownership.finish();
    expect(posts).toEqual([]);
  });
});

describe.each(['acct-B', 'acct'])('replacement owner on account %s', (accountName) => {
  it.each([
    ['restore lock', 'dispose'],
    ['restore lock', 'finish'],
    ['slice lookup', 'dispose'],
    ['slice lookup', 'finish'],
    ['started lock', 'dispose'],
    ['started lock', 'finish'],
  ] as const)('drops A after %s resolves following %s while B remains live', async (phase, teardown) => {
    let current: TwapOwner = { ownerId: 'A', generation: 1 };
    const createOwner = (ownerId: string, account: string) => {
      const emit = vi.fn();
      const findOrderByClientOrderId = vi.fn().mockResolvedValue({ found: true, order: { id: `${ownerId}-order` } });
      const post = vi.fn();
      const send = vi.fn((op: string, data: any) => {
        if (op === 'twap:state') instance.receive(persisted(...data));
      });
      const instance = new RustTwapOwnership(
        { emitter: { emit }, findOrderByClientOrderId } as unknown as Exchange,
        send,
        post,
        () => current,
      );
      const restore = (id: string, inFlight = false) => ({
        ...row(id),
        ownerId,
        options: { ...options, accountName: account },
        state: {
          ...row(id).state,
          accountName: account,
          ...(inFlight ? { inFlight: { size: 0.25, clientOrderId: `${ownerId}-client` } } : {}),
        },
      });
      return { instance, send, post, emit, findOrderByClientOrderId, restore };
    };
    let grantLock!: () => void;
    const lockGate = new Promise<void>((resolve) => {
      grantLock = resolve;
    });
    let settleLookup!: (value: { found: false; definitive: true }) => void;
    const lookupGate = new Promise<{ found: false; definitive: true }>((resolve) => {
      settleLookup = resolve;
    });
    let lateLockGranted = false;
    const a = createOwner('A', 'acct');
    if (phase === 'slice lookup') a.findOrderByClientOrderId.mockReturnValue(lookupGate);
    else {
      setTwapLockManager({
        async request(name, lockOptions, callback) {
          await lockGate;
          return locksFor('A').request(name, lockOptions, (lock) => {
            lateLockGranted = lock !== null;
            return callback(lock);
          });
        },
      });
    }
    const aRow = a.restore('retained', phase === 'slice lookup');
    if (phase === 'started lock') a.instance.receive(persisted(aRow));
    else a.instance.handle('twap:state', [aRow]);
    await vi.advanceTimersByTimeAsync(0);
    expect(a.send).not.toHaveBeenCalled();
    if (phase === 'slice lookup') {
      expect(a.findOrderByClientOrderId).toHaveBeenCalledWith('BTCUSDT', 'A-client');
      expect(held.has(twapLockName('acct', 'retained'))).toBe(true);
    }

    a.instance[teardown]();
    current = { ownerId: 'B', generation: 2 };
    setTwapLockManager(locksFor('B'));
    const b = createOwner('B', accountName);
    try {
      b.instance.handle('twap:state', [b.restore('live-B', true)]);
      await vi.advanceTimersByTimeAsync(0);
      expect(b.findOrderByClientOrderId).toHaveBeenCalledWith('BTCUSDT', 'B-client');
      expect(b.send).toHaveBeenCalledTimes(1);
      expect(held.get(twapLockName(accountName, 'live-B'))).toBe('B');
      const aPosts = a.post.mock.calls.length;
      const bPosts = b.post.mock.calls.length;

      // Resolve the exact old callback/promise only after B's own restore works.
      grantLock();
      settleLookup({ found: false, definitive: true });
      await vi.advanceTimersByTimeAsync(TWAP_RESTORE_LOCK_WAIT_MS + 500);
      if (phase !== 'slice lookup') expect(lateLockGranted).toBe(true);
      expect(a.send).not.toHaveBeenCalled();
      expect(a.post).toHaveBeenCalledTimes(aPosts);
      expect(a.emit).not.toHaveBeenCalled();
      expect(b.send).toHaveBeenCalledTimes(1);
      expect(b.post).toHaveBeenCalledTimes(bPosts);
      expect(b.emit).not.toHaveBeenCalled();
      expect(held.has(twapLockName('acct', 'retained'))).toBe(false);
      expect(held.get(twapLockName(accountName, 'live-B'))).toBe('B');

      // Reuse the old row id too: A's late release must not strand the lock.
      b.instance.handle('twap:state', [b.restore('retained')]);
      await vi.advanceTimersByTimeAsync(0);
      expect(b.send).toHaveBeenCalledTimes(2);
      expect(b.send.mock.calls[1][1][0]).toMatchObject({ id: 'retained', ownerId: 'B' });
      expect(held.get(twapLockName(accountName, 'retained'))).toBe('B');
    } finally {
      a.instance.dispose();
      b.instance.dispose();
      await vi.advanceTimersByTimeAsync(0);
    }
    expect(held.size).toBe(0);
  });
});
