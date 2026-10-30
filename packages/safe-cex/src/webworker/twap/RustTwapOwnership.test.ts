import type { Exchange } from '../../exchanges/base';
import type { TwapLockManager } from './twapLock';

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
const persisted = (...rows: ReturnType<typeof row>[]) =>
  Object.fromEntries(rows.map(({ id, ...rest }) => [id, rest]));

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

  it('refuses a stale row by the TS rules and rewrites the persisted rows', () => {
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
    ['an inconclusive lookup', { clientOrderId: 'c1' }, () => lookup.mockResolvedValue({ found: false, definitive: false })],
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
    expect(errors).toEqual([
      'TWAP BTCUSDT started without its cross-tab lock; a duplicated tab could also run it',
    ]);
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
