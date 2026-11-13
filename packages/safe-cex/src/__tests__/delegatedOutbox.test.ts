import type { DelegatedIntent, OutboxCompaction, OutboxConnection, OutboxOptions } from '../delegatedOutbox';
import type { OutboxOptions as LegacyOutboxOptions } from './fixtures/delegatedOutbox.7232375420';

import { describe, expect, it, vi } from 'vitest';

import frames from '../../../../sdk-rs/hosts/server/tests/data/delegated-orders-v1.json';
import routerReplies from '../../../../sdk-rs/hosts/server/tests/data/delegated-router-replies-v1.json';
import { createDelegatedIntentId, DelegatedOutbox, markDelegatedNotSent } from '../delegatedOutbox';
import { DelegatedOutbox as LegacyOutbox } from './fixtures/delegatedOutbox.7232375420';

const checkpoints = new WeakMap<Map<string, DelegatedIntent>, Map<string, number>>();
function fixture(owner = 'alice', rows = new Map<string, DelegatedIntent>()) {
  let now = 0;
  let entropy = 0;
  const timers = new Set<{ at: number; run: () => void }>();
  const writes: string[] = [];
  const key = (scope: { owner: string; accountName: string }, id: string) =>
    JSON.stringify([scope.owner, scope.accountName, id]);
  const sequences = checkpoints.get(rows) ?? new Map<string, number>();
  checkpoints.set(rows, sequences);
  const scopeKey = (scope: { owner: string; accountName: string }) => JSON.stringify([scope.owner, scope.accountName]);
  const options: OutboxOptions = {
    owner,
    accountName: 'main',
    venue: 'bybit',
    accountId: 'account-A',
    storage: {
      async load(scope) {
        const intents = [...rows.values()]
          .filter((row) => row.owner === scope.owner && row.accountName === scope.accountName)
          .map((row) => structuredClone(row));
        return { version: 1, sequence: sequences.get(scopeKey(scope)) ?? 0, intents };
      },
      async put(scope, row) {
        rows.set(key(scope, row.intentId), structuredClone(row));
      },
      async compact(scope, checkpoint) {
        for (const id of checkpoint.retiredIntentIds) rows.delete(key(scope, id));
        sequences.set(scopeKey(scope), Math.max(sequences.get(scopeKey(scope)) ?? 0, checkpoint.sequence));
      },
    },
    clock: { epoch: 'boot-A', now: () => now, unix: () => 1_700_000_000_000 + now },
    random(bytes) {
      bytes.fill(++entropy);
    },
    schedule(run, delay) {
      const timer = { at: now + delay, run };
      timers.add(timer);
      return () => {
        timers.delete(timer);
      };
    },
  };
  const connection = (instanceId = 'registration'): OutboxConnection => ({
    owner,
    accountName: 'main',
    accountId: 'account-A',
    venue: 'bybit',
    signingMode: 'delegated',
    ready: true,
    capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: true },
    instanceId,
    send(frame) {
      writes.push(frame);
      return true;
    },
  });
  return {
    options,
    rows,
    writes,
    connection,
    advance(ms: number) {
      now += ms;
      for (const timer of [...timers])
        if (timer.at <= now) {
          timers.delete(timer);
          timer.run();
        }
    },
  };
}
const params = { symbol: 'BTCUSDT', side: 'buy', type: 'limit', amount: 0.001, price: 50_000 };
function response(id: string, op: string, data: Record<string, unknown>) {
  return {
    t: op,
    d: { accountName: 'main', instanceId: 'registration', intentId: id, seq: 1, outcomeSeq: 1, ...data },
  };
}

describe('DELEGATED host-neutral outbox', () => {
  describe('caller-supplied intent identity', () => {
    const intentId = '018bcfe5-6800-7000-8000-000000000001';

    it.each([
      { source: 'caller', retry: false },
      { source: 'caller', retry: true },
      { source: 'SDK', retry: false },
      { source: 'SDK', retry: true },
    ])(
      'retry then restore: "never replay them automatically or interpret failure as proof that no order exists"; prevent "a double placement manufactured inside the SDK" ($source key, retry=$retry)',
      async ({ source, retry }) => {
        // Both quoted invariants are from packages/safe-cex/CLAUDE.md.
        const f = fixture();
        const box = await DelegatedOutbox.open(f.options);
        box.connect(f.connection());
        const id = await box.enqueue(params, source === 'caller' ? { intentId } : undefined);
        const beforeRetry = [...f.rows.values()][0];
        expect(beforeRetry.state).toBe('sending');
        expect(f.writes).toHaveLength(1);
        if (retry) {
          f.advance(100);
          expect(await box.enqueue(params, { intentId: id })).toBe(id);
          expect(f.writes).toHaveLength(1);
        }
        const afterRetry = [...f.rows.values()][0];
        box.close();
        const restored = await DelegatedOutbox.open(f.options);
        await restored.flush(restored.connect(f.connection('restored')));
        expect(f.writes.filter((frame) => JSON.parse(frame).t === 'order.submit')).toHaveLength(1);
        expect(afterRetry).toEqual(retry ? { ...beforeRetry, callerSuppliedIntentId: true } : beforeRetry);
        expect(restored.entries()[0]).toMatchObject({
          intentId: id,
          state: 'unknown',
          fingerprint: beforeRetry.fingerprint,
          seq: beforeRetry.seq,
        });
        restored.close();
      },
    );

    it.each([false, true])(
      'retry then restore preserves a committed handoff marker when its connection retires before dispatch (retry=%s)',
      async (retry) => {
        const f = fixture();
        const box = await DelegatedOutbox.open(f.options);
        const connection = box.connect(f.connection());
        const put = f.options.storage.put;
        f.options.storage.put = async (scope, row) => {
          await put(scope, row);
          if (row.state === 'sending') box.disconnect(connection);
        };
        const id = await box.enqueue(params);
        f.options.storage.put = put;
        const beforeRetry = [...f.rows.values()][0];
        expect(beforeRetry.state).toBe('sending');
        expect(box.entries()[0].state).toBe('queued');
        expect(f.writes).toEqual([]);
        if (retry) expect(await box.enqueue(params, { intentId: id })).toBe(id);
        const afterRetry = [...f.rows.values()][0];
        box.close();
        const restored = await DelegatedOutbox.open(f.options);
        await restored.flush(restored.connect(f.connection('restored')));
        expect(f.writes).toEqual([]);
        expect(afterRetry).toEqual(retry ? { ...beforeRetry, callerSuppliedIntentId: true } : beforeRetry);
        expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
        restored.close();
      },
    );

    it.each([false, true])(
      'retry then restore preserves uncertainty when receipt persistence failed (retry=%s)',
      async (retry) => {
        const f = fixture();
        const box = await DelegatedOutbox.open(f.options);
        const connection = box.connect(f.connection());
        const id = await box.enqueue(params, { intentId });
        const beforeRetry = [...f.rows.values()][0];
        const put = f.options.storage.put;
        f.options.storage.put = async () => {
          throw new Error('receipt storage unavailable');
        };
        await expect(
          box.receive(
            connection,
            response(id, 'order.outcome', {
              outcome: 'placed',
              clientOrderId: id,
              venueOrderId: 'synthetic-native-order',
            }),
          ),
        ).rejects.toThrow('receipt storage unavailable');
        expect(box.entries()[0].state).toBe('placed');
        f.options.storage.put = put;
        if (retry) expect(await box.enqueue(params, { intentId: id })).toBe(id);
        const afterRetry = [...f.rows.values()][0];
        box.close();
        const restored = await DelegatedOutbox.open(f.options);
        await restored.flush(restored.connect(f.connection('restored')));
        expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
        expect(afterRetry).toEqual(beforeRetry);
        expect(f.writes).toHaveLength(1);
        restored.close();
      },
    );

    it.each(['queued', 'expired', 'routerRefusal'] as const)(
      'SDK-key adoption preserves the committed evidence gate (%s)',
      async (kind) => {
        const f = fixture();
        const box = await DelegatedOutbox.open(f.options);
        const id = await box.enqueue(params);
        if (kind === 'expired') {
          f.advance(10_000);
          await box.flush(box.connect(f.connection()));
        } else if (kind === 'routerRefusal') {
          const connection = box.connect(f.connection());
          await box.flush(connection);
          await box.receive(connection, response(id, 'order.ack', { received: false }));
        }
        const committed = [...f.rows.values()][0];
        expect(committed.state).toBe(kind === 'queued' ? 'queued' : 'notSent');
        expect(kind === 'queued' ? committed.noHandoffEvidence : committed.notSentEvidence).toMatchObject({
          intentId: id,
          seq: 1,
          fingerprint: committed.fingerprint,
          kind: kind === 'routerRefusal' ? 'routerRefusal' : 'noHandoff',
        });
        expect(await box.enqueue(params, { intentId: id })).toBe(id);
        expect([...f.rows.values()][0]).toEqual({ ...committed, callerSuppliedIntentId: true });
        box.close();
        const restored = await DelegatedOutbox.open(f.options);
        await restored.flush(restored.connect(f.connection('restored')));
        expect(restored.entries()[0].state).toBe(kind === 'queued' ? 'sending' : 'notSent');
        expect(f.writes).toHaveLength(kind === 'expired' ? 0 : 1);
        restored.close();
      },
    );

    it.each([
      '',
      'not-a-uuid',
      '018bcfe5-6800-4000-8000-000000000001',
      '018bcfe5-6800-7000-c000-000000000001',
      null,
      7,
    ])('refuses invalid key %s before storage or send', async (invalid) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection());
      await expect(box.enqueue(params, { intentId: invalid as string })).rejects.toMatchObject({
        name: 'DelegatedIntentError',
        code: 'invalidIntentId',
      });
      expect(box.entries()).toEqual([]);
      expect(f.rows.size).toBe(0);
      expect(f.writes).toEqual([]);
      box.close();
    });

    it('dedupes canonical params without another sequence, timer, or send', async () => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection());
      expect(await box.enqueue(params, { intentId })).toBe(intentId);
      const original = box.entries()[0];
      f.advance(100);
      expect(
        await box.enqueue(
          { price: 50_000, amount: 0.001, type: 'limit', side: 'buy', symbol: 'BTCUSDT' },
          { intentId },
        ),
      ).toBe(intentId);
      expect(box.entries()).toEqual([original]);
      expect(f.writes).toHaveLength(1);
      const next = await box.enqueue(params);
      expect(box.entries().find((row) => row.intentId === next)?.seq).toBe(2);
      box.close();
    });

    it('refuses changed params for the same key, including after restore', async () => {
      const f = fixture();
      let box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection());
      await box.enqueue(params, { intentId });
      for (const restored of [false, true]) {
        if (restored) {
          box.close();
          box = await DelegatedOutbox.open(f.options);
          box.connect(f.connection());
        }
        await expect(box.enqueue({ ...params, amount: 2 }, { intentId })).rejects.toMatchObject({
          name: 'DelegatedIntentError',
          code: 'intentConflict',
        });
        expect(await box.enqueue(params, { intentId })).toBe(intentId);
        expect(box.entries()).toHaveLength(1);
        expect(f.writes).toHaveLength(1);
      }
      box.close();
    });

    it('waits for original persistence when a duplicate arrives during the deferral', async () => {
      const f = fixture();
      const put = f.options.storage.put;
      let release!: () => void;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      f.options.storage.put = async (...args) => {
        await gate;
        await put(...args);
      };
      const box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection());
      const first = box.enqueue(params, { intentId });
      let settled = false;
      const duplicate = box.enqueue(params, { intentId }).then((id) => {
        settled = true;
        return id;
      });
      await Promise.resolve();
      expect(settled).toBe(false);
      expect(f.writes).toEqual([]);
      release();
      expect(await Promise.all([first, duplicate])).toEqual([intentId, intentId]);
      expect(f.writes).toHaveLength(1);
      expect(box.entries()).toHaveLength(1);
      box.close();
    });

    it('does not report a duplicate durable when original persistence fails', async () => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      f.options.storage.put = async () => {
        throw new Error('storage unavailable');
      };
      const first = box.enqueue(params, { intentId });
      const duplicate = box.enqueue(params, { intentId });
      await expect(first).rejects.toThrow('storage unavailable');
      await expect(duplicate).rejects.toMatchObject({ name: 'DelegatedIntentError', code: 'intentNotDurable' });
      expect(f.writes).toEqual([]);
      box.close();
    });

    it('keeps completed caller identity through compaction and restore', async () => {
      const f = fixture();
      let box = await DelegatedOutbox.open(f.options);
      await box.cancel(await box.enqueue(params, { intentId }));
      for (let i = 0; i < 40; i++) {
        await box.cancel(await box.enqueue(params));
        f.advance(1);
      }
      box.close();
      box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection());
      expect(await box.enqueue(params, { intentId })).toBe(intentId);
      await expect(box.enqueue({ ...params, price: 1 }, { intentId })).rejects.toMatchObject({
        code: 'intentConflict',
      });
      expect(f.writes).toEqual([]);
      expect(box.entries().find((row) => row.intentId === intentId)?.state).toBe('canceled');
      box.close();
    });

    it('requires a durable reservation when adopting an SDK-minted key, including after a failed put', async () => {
      const f = fixture();
      let box = await DelegatedOutbox.open(f.options);
      const id = await box.enqueue(params);
      await box.cancel(id);
      const put = f.options.storage.put;
      f.options.storage.put = async () => {
        throw new Error('reservation unavailable');
      };
      await expect(box.enqueue(params, { intentId: id })).rejects.toThrow('reservation unavailable');
      await expect(box.enqueue(params, { intentId: id })).rejects.toThrow('reservation unavailable');
      f.options.storage.put = put;
      expect(await box.enqueue(params, { intentId: id })).toBe(id);
      for (let i = 0; i < 40; i++) {
        await box.cancel(await box.enqueue(params));
        f.advance(1);
      }
      box.close();
      box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection());
      expect(await box.enqueue(params, { intentId: id })).toBe(id);
      expect(f.writes).toEqual([]);
      expect(box.entries().find((row) => row.intentId === id)?.callerSuppliedIntentId).toBe(true);
      box.close();
    });

    it('dedupes at capacity and refuses new keys without forgetting completed caller identities', async () => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      for (let i = 0; i < 1056; i++) {
        const id = `018bcfe5-6800-7000-8000-${i.toString(16).padStart(12, '0')}`;
        await box.cancel(await box.enqueue(params, { intentId: id }));
      }
      expect(await box.enqueue(params, { intentId })).toBe(intentId);
      await expect(box.enqueue(params, { intentId: '018bcfe5-6800-7000-8000-ffffffffffff' })).rejects.toThrow('full');
      expect(box.entries()).toHaveLength(1056);
      expect(f.rows.size).toBe(1056);
      expect(f.writes).toEqual([]);
      box.close();
    });

    it('keeps SDK-key adoption scoped when the owner switches during reservation persistence', async () => {
      const a = fixture();
      const alice = await DelegatedOutbox.open(a.options);
      const id = await alice.enqueue(params);
      const put = a.options.storage.put;
      let release!: () => void;
      let started!: () => void;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      const entered = new Promise<void>((resolve) => {
        started = resolve;
      });
      a.options.storage.put = async (scope, row) => {
        started();
        await gate;
        await put(scope, row);
      };
      const adoption = alice.enqueue(params, { intentId: id });
      await entered;
      alice.close();
      const b = fixture('bob', a.rows);
      const bob = await DelegatedOutbox.open(b.options);
      bob.connect(b.connection());
      expect(await bob.enqueue(params, { intentId: id })).toBe(id);
      release();
      expect(await adoption).toBe(id);
      expect(a.writes).toEqual([]);
      expect(b.writes).toHaveLength(1);
      const saved = [...a.rows.values()];
      expect(saved).toHaveLength(2);
      expect(saved.find((row) => row.owner === 'alice')).toMatchObject({
        intentId: id,
        state: 'queued',
        callerSuppliedIntentId: true,
      });
      expect(saved.find((row) => row.owner === 'bob')).toMatchObject({ intentId: id, state: 'sending' });
      bob.close();
    });

    it('cannot reattribute a duplicate when the owner switches during original persistence', async () => {
      const a = fixture();
      const put = a.options.storage.put;
      let release!: () => void;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      a.options.storage.put = async (...args) => {
        await gate;
        await put(...args);
      };
      const alice = await DelegatedOutbox.open(a.options);
      alice.connect(a.connection());
      const first = alice.enqueue(params, { intentId });
      const duplicate = expect(alice.enqueue(params, { intentId })).rejects.toThrow('closed');
      await Promise.resolve();
      alice.close();
      const b = fixture('bob', a.rows);
      const bob = await DelegatedOutbox.open(b.options);
      bob.connect(b.connection());
      expect(await bob.enqueue(params, { intentId })).toBe(intentId);
      release();
      await first;
      await duplicate;
      expect(a.writes).toEqual([]);
      expect(b.writes).toHaveLength(1);
      expect([...a.rows.values()].map((row) => row.owner).sort()).toEqual(['alice', 'bob']);
      bob.close();
    });
  });

  describe('completed index subset', () => {
    type IndexedOutbox = {
      intents: Map<string, DelegatedIntent>;
      completedIntents: Map<string, DelegatedIntent>;
      pendingCount(): number;
      compact(): Promise<void>;
    };
    const index = (box: DelegatedOutbox) => box as unknown as IndexedOutbox;
    function expectSubset(box: DelegatedOutbox): void {
      const { intents, completedIntents } = index(box);
      for (const [id, row] of completedIntents) expect(intents.get(id)).toBe(row);
    }

    it('keeps the same-object subset after cancel, compact and restore', async () => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      for (let n = 0; n < 35; n++) {
        await box.cancel(await box.enqueue(params));
        expectSubset(box);
      }
      expect(index(box).intents.size).toBe(32);
      expect(index(box).completedIntents.size).toBe(32);
      box.close();
      const restored = await DelegatedOutbox.open(f.options);
      expectSubset(restored);
      expect(index(restored).completedIntents.size).toBe(32);
      await restored.cancel(await restored.enqueue(params));
      expectSubset(restored);
      expect(index(restored).intents.size).toBe(32);
      expect(f.writes).toHaveLength(0);
      restored.close();
    });

    it.each(['removed', 'replaced'] as const)(
      'never undercounts pending work when a completed row was %s without updating the index',
      async (fault) => {
        const f = fixture();
        const box = await DelegatedOutbox.open(f.options);
        await box.enqueue(params);
        const completed = await box.enqueue(params);
        await box.cancel(completed);
        const internal = index(box);
        const row = internal.intents.get(completed)!;
        if (fault === 'removed') internal.intents.delete(completed);
        else internal.intents.set(completed, { ...row, state: 'queued' });
        // Fault injection bypasses the production removal discipline intentionally.
        expect(internal.pendingCount()).toBe(fault === 'removed' ? 1 : 2);
        expect(internal.completedIntents.has(completed)).toBe(false);
        expectSubset(box);
        box.close();
      },
    );

    it('prunes an orphan before compaction so comparisons stay finite and retirement stays live', async () => {
      const f = fixture();
      const compact = f.options.storage.compact;
      f.options.storage.compact = async () => {
        throw new Error('hold retirement');
      };
      const box = await DelegatedOutbox.open(f.options);
      const ids: string[] = [];
      for (let n = 0; n < 33; n++) {
        const id = await box.enqueue(params);
        ids.push(id);
        if (n === 32) await expect(box.cancel(id)).rejects.toThrow('hold retirement');
        else await box.cancel(id);
      }
      const internal = index(box);
      const first = internal.intents.get(ids[0])!;
      internal.completedIntents.set('orphan', { ...first, intentId: 'orphan' });
      const retired: string[][] = [];
      f.options.storage.compact = async (scope, checkpoint) => {
        retired.push([...checkpoint.retiredIntentIds]);
        await compact(scope, checkpoint);
      };
      const comparisons: number[] = [];
      const sort = Array.prototype.sort;
      const spy = vi.spyOn(Array.prototype, 'sort').mockImplementation(function <T>(
        this: T[],
        compare?: (a: T, b: T) => number,
      ) {
        return sort.call(
          this,
          compare &&
            ((a: T, b: T) => {
              const result = compare(a, b);
              comparisons.push(result);
              return result;
            }),
        );
      });
      // compact sorts synchronously before its storage await; restore the global
      // method before yielding to other work in the test runner.
      const compacted = (() => {
        try {
          return internal.compact();
        } finally {
          spy.mockRestore();
        }
      })();
      await compacted;
      expect(comparisons.length).toBeGreaterThan(0);
      expect(comparisons.every(Number.isFinite)).toBe(true);
      expect(retired).toEqual([[ids[0]]]);
      expect(internal.completedIntents.has('orphan')).toBe(false);
      expectSubset(box);
      expect(internal.intents.size).toBe(32);
      expect(f.rows.size).toBe(32);
      box.close();
    });
  });

  it('does not replay a legacy queued handoff after a declined reconnect in the same epoch', async () => {
    const f = fixture();
    // Exercise the historical writer rather than manufacturing an ambiguous label.
    const old = await LegacyOutbox.open(f.options as unknown as LegacyOutboxOptions);
    old.connect({ ...f.connection(), venue: 'bybit' });
    const id = await old.enqueue(params);
    expect(f.writes).toHaveLength(1);
    const declined = old.connect({ ...f.connection(), venue: 'bybit', send: () => false });
    await old.flush(declined);
    const saved = [...f.rows.values()][0];
    expect(saved).toMatchObject({ intentId: id, state: 'queued', clockEpoch: 'boot-A', createdAt: 0 });
    expect(saved).not.toHaveProperty('noHandoffEvidence');
    expect(saved).not.toHaveProperty('notSentEvidence');
    old.close();
    const before = structuredClone([...f.rows.values()]);
    await expect(DelegatedOutbox.open(f.options)).rejects.toThrow('Persisted outbox scope differs');
    expect(f.writes).toHaveLength(1);
    expect([...f.rows.values()]).toEqual(before);
  });
  it('restores a proven never-sent queued row and sends it exactly once', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const id = await box.enqueue(params);
    expect([...f.rows.values()][0]).toMatchObject({
      state: 'queued',
      noHandoffEvidence: { kind: 'noHandoff', source: 'local', attempted: false, intentId: id, seq: 1 },
    });
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'queued' });
    const c = restored.connect(f.connection());
    await restored.flush(c);
    await restored.flush(c);
    expect(f.writes).toHaveLength(1);
    expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'sending' });
    expect([...f.rows.values()][0]).not.toHaveProperty('noHandoffEvidence');
    restored.close();
  });
  it('refuses an unbound legacy queued row without rewriting its unresolved history', async () => {
    const f = fixture();
    const box = await LegacyOutbox.open(f.options as unknown as LegacyOutboxOptions);
    const id = await box.enqueue(params);
    expect([...f.rows.values()][0]).toMatchObject({ intentId: id, state: 'queued', clockEpoch: 'boot-A' });
    expect([...f.rows.values()][0]).not.toHaveProperty('noHandoffEvidence');
    box.close();
    const before = structuredClone([...f.rows.values()]);
    await expect(DelegatedOutbox.open(f.options)).rejects.toThrow('Persisted outbox scope differs');
    expect(f.writes).toHaveLength(0);
    expect([...f.rows.values()]).toEqual(before);
  });
  it.each(['notSent', 'canceled'] as const)(
    'rollout restores unresolved legacy %s cancellations conservatively',
    async (state) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection());
      const id = await box.enqueue(params);
      const row = box.entries()[0];
      box.close();
      // Legacy JSON intentionally lies outside the current branded state boundary.
      await f.options.storage.put(f.options, JSON.parse(JSON.stringify({ ...row, state, cancelPending: true })));
      f.advance(10_000);
      const restored = await DelegatedOutbox.open(f.options);
      expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'unknown', cancelPending: true });
      await restored.flush(restored.connect(f.connection('replacement')));
      expect(f.writes.map((text) => JSON.parse(text).t)).toEqual(['order.submit', 'order.cancelQueued']);
      restored.close();
    },
  );

  it.each([undefined, false])('rollout requires the router ledger capability (%s)', async (ledger) => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect({
      ...f.connection(),
      capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: ledger },
    });
    await box.enqueue(params);
    await box.flush(c);
    expect(f.writes).toEqual([]);
    box.close();
  });

  it('rollout rechecks both capabilities after a connection replacement', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const first = box.connect({
      ...f.connection(),
      capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: true },
    });
    const id = await box.enqueue(params);
    expect(f.writes).toHaveLength(1);
    box.disconnect(first);
    f.advance(500);
    const replacement = box.connect({
      ...f.connection('replacement'),
      capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: false },
    });
    await box.flush(replacement);
    expect(f.writes).toHaveLength(1);
    expect(box.entries()[0].intentId).toBe(id);
    box.close();
  });

  it('rollout keeps a physically handed-off intent unresolved at its deadline and after reload', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect({
      ...f.connection(),
      capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: true },
    });
    const id = await box.enqueue(params);
    expect(f.writes).toHaveLength(1);
    f.advance(10_000);
    await box.flush(c);
    expect(box.entries()[0].state).toBe('unknown');
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    const replacement = restored.connect({
      ...f.connection('replacement'),
      capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: true },
    });
    await restored.flush(replacement);
    expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
    const submits = f.writes.map((text) => JSON.parse(text)).filter((frame) => frame.t === 'order.submit');
    expect(submits).toHaveLength(1);
    expect(submits[0].d.intentId).toBe(id);
    restored.close();
  });

  it('restores a legacy notSent row without evidence as unknown', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const id = await box.enqueue(params);
    box.close();
    const [key, saved] = [...f.rows][0];
    // Simulate JSON written by an old build, outside the current type boundary.
    f.rows.set(
      key,
      JSON.parse(JSON.stringify({ ...saved, state: 'notSent', error: { message: 'unproven', outcome: 'notSent' } })),
    );
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
    expect(restored.entries()[0].error?.outcome).not.toBe('notSent');
    expect(f.writes).toEqual([]);
    restored.close();
  });
  it.each(['local', 'router'] as const)('validates and restores scoped %s no-forwarding evidence', async (kind) => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    let c = kind === 'router' ? box.connect(f.connection()) : undefined;
    const id = await box.enqueue(params);
    if (c) await box.receive(c, response(id, 'order.ack', { received: false }));
    else f.advance(10_000);
    c ??= box.connect(f.connection());
    await box.flush(c);
    expect(box.entries()[0]).toMatchObject({
      state: 'notSent',
      notSentEvidence: { kind: kind === 'local' ? 'noHandoff' : 'routerRefusal', intentId: id, seq: 1 },
    });
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries()[0].state).toBe('notSent');
    expect(f.writes).toHaveLength(kind === 'local' ? 0 : 1);
    restored.close();
  });
  it.each(['owner', 'accountName', 'intentId', 'seq', 'fingerprint', 'kind', 'received'] as const)(
    'restores unknown when persisted no-forwarding evidence has an invalid %s',
    async (field) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      const c = box.connect(f.connection());
      const id = await box.enqueue(params);
      await box.receive(c, response(id, 'order.ack', { received: false }));
      box.close();
      const [key, saved] = [...f.rows][0];
      const wire = JSON.parse(JSON.stringify(saved));
      wire.notSentEvidence[field] = field === 'seq' ? 2 : field === 'received' ? true : 'foreign';
      f.rows.set(key, wire);
      const restored = await DelegatedOutbox.open(f.options);
      await restored.flush(restored.connect(f.connection()));
      expect(restored.entries()[0].state).toBe('unknown');
      expect(restored.entries()[0].notSentEvidence).toBeUndefined();
      expect(f.writes).toHaveLength(1);
      restored.close();
    },
  );
  it.each(['sending', 'unknown'] as const)(
    'refuses a local no-handoff claim for an already %s intent',
    async (state) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection());
      await box.enqueue(params);
      if (state === 'unknown') f.advance(10_000);
      const row = box.entries()[0];
      expect(markDelegatedNotSent(row, { kind: 'noHandoff', source: 'adapter', handedOff: false })).toBe(false);
      expect(row.state).toBe('unknown');
      expect(row.notSentEvidence).toBeUndefined();
      box.close();
    },
  );
  it('rejects a live unbranded notSent claim even with copied JSON evidence', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const id = await box.enqueue(params);
    const row = box.entries()[0];
    expect(markDelegatedNotSent(row, { kind: 'noHandoff', source: 'local', attempted: false })).toBe(true);
    // Deliberately bypass the compiler to test the runtime boundary, not restore.
    const internal = box as unknown as { intents: Map<string, DelegatedIntent> };
    internal.intents.set(id, JSON.parse(JSON.stringify(row)));
    expect(box.entries()[0].state).toBe('unknown');
    box.close();
  });
  it('marks physical handoff in flight and turns its deadline into unknown', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const id = await box.enqueue(params);
    expect(box.entries()[0]).toMatchObject({ intentId: id, state: 'sending' });
    f.advance(10_000);
    expect(box.entries()[0].state).toBe('unknown');
    await box.flush(c);
    expect([...f.rows.values()][0].state).toBe('unknown');
    expect(f.writes).toHaveLength(1);
    box.close();
  });
  it('keeps a handoff unknown through disconnect, deadline and reconnect without a replay', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    await box.enqueue(params);
    box.disconnect(c);
    expect(box.entries()[0].state).toBe('unknown');
    f.advance(10_000);
    await box.flush(box.connect(f.connection()));
    expect(box.entries()[0].state).toBe('unknown');
    expect(f.writes).toHaveLength(1);
    box.close();
  });
  it.each([false, true])(
    'makes direct close unknown immediately after handoff (acknowledged=%s)',
    async (acknowledged) => {
      const f = fixture();
      const changes: DelegatedIntent[] = [];
      f.options.changed = (row) => changes.push(row);
      const box = await DelegatedOutbox.open(f.options);
      const c = box.connect(f.connection());
      const id = await box.enqueue(params);
      if (acknowledged) await box.receive(c, response(id, 'order.ack', { received: true }));
      expect(box.entries()[0].state).toBe(acknowledged ? 'acknowledged' : 'sending');
      const beforeClose = changes.length;
      box.close();
      expect(box.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
      f.advance(10_000);
      await box.receive(c, response(id, 'order.ack', { received: false, error: 'late refusal' }));
      await box.flush(c);
      expect(box.entries()[0].state).toBe('unknown');
      expect(changes).toHaveLength(beforeClose);
      expect(f.writes).toHaveLength(1);
      const restored = await DelegatedOutbox.open(f.options);
      await restored.flush(restored.connect(f.connection()));
      expect(restored.entries()[0].state).toBe('unknown');
      expect(f.writes).toHaveLength(1);
      restored.close();
    },
  );
  it.each([false, true])('fences a throwing send as unknown (physical handoff: %s)', async (handedOff) => {
    const f = fixture();
    const changes: DelegatedIntent[] = [];
    f.options.changed = (row) => changes.push(row);
    const box = await DelegatedOutbox.open(f.options);
    let attempts = 0;
    const connection = box.connect({
      ...f.connection(),
      send: (frame) => {
        attempts++;
        if (handedOff) f.writes.push(frame);
        throw new Error('connection lost');
      },
    });
    const id = await box.enqueue(params);
    expect(box.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
    expect(changes.at(-1)?.state).toBe('unknown');
    expect([...f.rows.values()][0]).toMatchObject({ state: 'unknown' });
    expect([...f.rows.values()][0]).not.toHaveProperty('noHandoffEvidence');
    expect([...f.rows.values()][0]).not.toHaveProperty('notSentEvidence');
    await box.flush(connection);
    f.advance(60_000);
    await box.flush(box.connect(f.connection()));
    expect(box.entries()[0].state).toBe('unknown');
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries()[0].state).toBe('unknown');
    expect(attempts).toBe(1);
    expect(f.writes).toHaveLength(handedOff ? 1 : 0);
    restored.close();
  });
  it.each(['closed', 'declined'] as const)('expires a never-forwarded %s send as notSent', async (kind) => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect({
      ...f.connection(),
      send: () => {
        return false;
      },
    });
    if (kind === 'closed') box.disconnect(c);
    await box.enqueue(params);
    f.advance(10_000);
    expect(box.entries()[0].state).toBe('notSent');
    await box.flush(box.connect(f.connection()));
    expect(f.writes).toEqual([]);
    box.close();
  });
  it('preserves no-handoff proof only for an explicit false return', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect({ ...f.connection(), send: () => false });
    const id = await box.enqueue(params);
    expect([...f.rows.values()][0]).toHaveProperty('noHandoffEvidence.source', 'adapter');
    f.advance(10_000);
    expect(box.entries()[0]).toMatchObject({ intentId: id, state: 'notSent' });
    await box.flush(box.connect(f.connection()));
    expect([...f.rows.values()][0]).not.toHaveProperty('noHandoffEvidence');
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'notSent' });
    expect(f.writes).toEqual([]);
    restored.close();
  });
  it.each([undefined, null, 0, ''])(
    'fences a non-boolean return (%s) as unknown after a physical write',
    async (result) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      let attempts = 0;
      const connection = box.connect({
        ...f.connection(),
        send: (frame) => {
          attempts++;
          f.writes.push(frame);
          // Exercise an invalid JS adapter return without widening the TS contract.
          return result as unknown as boolean;
        },
      });
      const id = await box.enqueue(params);
      expect(box.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
      expect([...f.rows.values()][0]).toMatchObject({ state: 'unknown' });
      expect([...f.rows.values()][0]).not.toHaveProperty('noHandoffEvidence');
      expect([...f.rows.values()][0]).not.toHaveProperty('notSentEvidence');
      await box.flush(connection);
      box.close();
      // Reload before the original deadline in the same epoch: time cannot fence a replay for us.
      const restored = await DelegatedOutbox.open(f.options);
      await restored.flush(restored.connect(f.connection()));
      expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
      f.advance(60_000);
      await restored.flush(restored.connect(f.connection()));
      expect(restored.entries()[0].state).toBe('unknown');
      expect(attempts).toBe(1);
      expect(f.writes).toHaveLength(1);
      restored.close();
    },
  );
  it('expires unknown retention explicitly after 24 hours and recovers capacity durably', async () => {
    const f = fixture();
    const expired: DelegatedIntent[] = [];
    f.options.changed = (row) => {
      if ('retentionExpired' in row && row.retentionExpired) expired.push(row);
    };
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    let first = '';
    for (let seq = 1; seq <= 1024; seq++) {
      const id = await box.enqueue(params);
      if (!first) first = id;
      await box.receive(
        c,
        response(id, 'order.outcome', { seq, outcomeSeq: seq, outcome: 'unknown', clientOrderId: id }),
      );
      f.advance(1);
    }
    await expect(box.enqueue(params)).rejects.toThrow('full');
    f.advance(86_400_000 - 1024 - 1);
    expect(box.entries().find((row) => row.intentId === first)).toMatchObject({ state: 'unknown' });
    expect(expired).toHaveLength(0);
    f.advance(1025);
    await box.flush(c);
    expect(expired).toHaveLength(1024);
    expect(expired.every((row) => row.state === 'unknown')).toBe(true);
    expect(box.entries()).toHaveLength(32);
    expect(f.rows.size).toBe(32);
    await box.enqueue(params);
    expect(JSON.parse(f.writes.at(-1)!).d.seq).toBe(1025);
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries().filter((row) => row.state === 'unknown')).toHaveLength(33);
    expect(f.writes).toHaveLength(1025);
    restored.close();
  });

  it.each(['declined', 'throw'] as const)('preserves a reentrant cancellation when handoff is %s', async (kind) => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    let cancellation: Promise<void> | undefined;
    box.connect({
      ...f.connection(),
      send: (text) => {
        if (JSON.parse(text).t !== 'order.submit') return false;
        cancellation = box.cancel(JSON.parse(text).d.intentId);
        if (kind === 'throw') throw new Error('connection lost');
        return false;
      },
    });
    await box.enqueue(params);
    await cancellation;
    expect(box.entries()[0]).toMatchObject({
      state: kind === 'throw' ? 'unknown' : 'canceled',
      cancelPending: kind === 'throw',
    });
    await box.flush(box.connect(f.connection()));
    expect(f.writes.map((frame) => JSON.parse(frame).t)).toEqual(kind === 'throw' ? ['order.cancelQueued'] : []);
    box.close();
  });
  it('keeps delayed unknown-expiry storage and timers with Alice across a Bob login', async () => {
    const a = fixture('alice');
    const aliceChanges: DelegatedIntent[] = [];
    a.options.changed = (row) => aliceChanges.push(row);
    const alice = await DelegatedOutbox.open(a.options);
    const c = alice.connect(a.connection());
    const aId = await alice.enqueue(params);
    await alice.receive(c, response(aId, 'order.outcome', { outcome: 'unknown', clientOrderId: aId }));
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const waiting = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const put = a.options.storage.put;
    a.options.storage.put = async (scope, row) => {
      if (row.retentionExpired) {
        entered();
        await held;
      }
      await put(scope, row);
    };
    a.advance(86_400_000);
    await waiting;
    alice.close();
    const changesAtLogout = aliceChanges.length;
    const b = fixture('bob', a.rows);
    const bob = await DelegatedOutbox.open(b.options);
    const bc = bob.connect(b.connection());
    const bId = await bob.enqueue(params);
    release();
    await alice.flush(c);
    a.advance(86_400_000);
    await bob.flush(bc);
    expect(aliceChanges).toHaveLength(changesAtLogout);
    expect(a.writes).toHaveLength(1);
    expect(b.writes).toHaveLength(1);
    expect(bob.entries()[0]).toMatchObject({ owner: 'bob', intentId: bId, state: 'sending' });
    expect([...a.rows.values()].filter((row) => row.owner === 'bob')).toEqual(bob.entries());
    bob.close();
    const restored = await DelegatedOutbox.open(a.options);
    await restored.flush(restored.connect(a.connection()));
    expect(restored.entries()[0]).toMatchObject({ owner: 'alice', state: 'unknown', retentionExpired: true });
    expect(a.writes).toHaveLength(1);
    restored.close();
  });
  it('returns the durable physically-sent intent even when unrelated retirement fails', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const compact = f.options.storage.compact;
    try {
      f.options.storage.compact = async () => {
        throw new Error('synthetic retirement unavailable');
      };
      for (let n = 0; n < 32; n++) {
        await box.cancel(await box.enqueue(params));
        f.advance(1);
      }
      const last = await box.enqueue(params);
      await expect(box.cancel(last)).rejects.toThrow('retirement');
      f.advance(1);
      expect(box.entries()).toHaveLength(33);
      const connection = box.connect(f.connection());
      const result = await box.enqueue(params).then(
        (id) => ({ id, error: undefined as string | undefined }),
        (error) => ({ id: undefined, error: String(error) }),
      );
      expect(f.writes).toHaveLength(1);
      const sent = JSON.parse(f.writes[0]);
      expect([...f.rows.values()].some((row) => row.intentId === sent.d.intentId)).toBe(true);
      expect(result.error, 'an accepted handoff must return its original intent ID').toBeUndefined();
      expect(result.id).toBe(sent.d.intentId);
      expect(box.entries()).toHaveLength(34);
      expect(f.rows.size).toBe(34);
      f.options.storage.compact = compact;
      await box.flush(connection);
      expect(box.entries()).toHaveLength(33);
      expect(f.rows.size).toBe(33);
      expect(f.writes).toHaveLength(1);
      expect(box.entries().find((row) => row.intentId === result.id)?.state).toBe('sending');
    } finally {
      box.close();
    }
  });
  it('distinguishes a declined handoff from a successful handoff on cancellation', async () => {
    for (const handedOff of [false, true]) {
      const f = fixture();
      const attempts: string[] = [];
      const box = await DelegatedOutbox.open(f.options);
      box.connect({
        ...f.connection(),
        send: (frame) => {
          attempts.push(JSON.parse(frame).t);
          return handedOff;
        },
      });
      const id = await box.enqueue(params);
      await box.cancel(id);
      expect(box.entries()[0]).toMatchObject({ cancelPending: handedOff, state: handedOff ? 'unknown' : 'canceled' });
      expect(attempts).toEqual(handedOff ? ['order.submit', 'order.cancelQueued'] : ['order.submit']);
      await box.flush(box.connect(f.connection()));
      expect(f.writes.map((frame) => JSON.parse(frame).t)).toEqual(handedOff ? ['order.cancelQueued'] : []);
      box.close();
    }
  });
  it.each([false, true])(
    'preserves synchronous cancellation during a declined handoff (prior handoff: %s)',
    async (priorHandoff) => {
      const f = fixture();
      const changes: DelegatedIntent[] = [];
      f.options.changed = (row) => {
        changes.push(row);
      };
      let box = await DelegatedOutbox.open(f.options);
      let id: string | undefined;
      let cancellation: Promise<void> | undefined;
      if (priorHandoff) {
        box.connect(f.connection());
        id = await box.enqueue(params);
        // Capture cancellation before replacement; handed-off work is never resubmitted.
        cancellation = box.cancel(id);
      }
      const declined = box.connect({
        ...f.connection(),
        send(frame) {
          const message = JSON.parse(frame);
          if (message.t === 'order.submit') cancellation = box.cancel(message.d.intentId);
          return false;
        },
      });
      if (priorHandoff) await box.flush(declined);
      else id = await box.enqueue(params);
      expect(cancellation).toBeDefined();
      await cancellation;
      const afterCancel = box.entries()[0];
      const persistedAfterCancel = [...f.rows.values()][0];
      await box.flush(box.connect(f.connection('replacement')));
      expect(f.writes.map((frame) => JSON.parse(frame).t)).toEqual(
        priorHandoff ? ['order.submit', 'order.cancelQueued'] : [],
      );
      const expected = { intentId: id, state: priorHandoff ? 'unknown' : 'canceled', cancelPending: priorHandoff };
      expect(afterCancel).toMatchObject(expected);
      expect(persistedAfterCancel).toMatchObject(expected);
      expect(changes.at(-1)).toMatchObject(expected);
      f.advance(10_000);
      box.close();
      box = await DelegatedOutbox.open(f.options);
      await box.flush(box.connect(f.connection('restored')));
      expect(box.entries()[0]).toMatchObject(expected);
      expect(f.writes.filter((frame) => JSON.parse(frame).t === 'order.submit')).toHaveLength(priorHandoff ? 1 : 0);
      box.close();
    },
  );
  it.each([false, true])('preserves expiry during a declined handoff (prior handoff: %s)', async (priorHandoff) => {
    const f = fixture();
    const changes: DelegatedIntent[] = [];
    f.options.changed = (row) => {
      changes.push(row);
    };
    const box = await DelegatedOutbox.open(f.options);
    let id: string | undefined;
    if (priorHandoff) {
      box.connect(f.connection());
      id = await box.enqueue(params);
    }
    const declined = box.connect({
      ...f.connection(),
      send(frame) {
        if (JSON.parse(frame).t === 'order.submit') f.advance(10_000);
        return false;
      },
    });
    if (priorHandoff) {
      await box.flush(declined);
      // No second submit may call the adapter; advance the existing handoff's deadline.
      f.advance(10_000);
    } else id = await box.enqueue(params);
    const afterDecline = box.entries()[0];
    await box.flush(box.connect(f.connection('replacement')));
    expect(afterDecline).toMatchObject({
      intentId: id,
      state: priorHandoff ? 'unknown' : 'notSent',
      cancelPending: false,
    });
    expect(changes.at(-1)).toMatchObject({ state: priorHandoff ? 'unknown' : 'notSent', cancelPending: false });
    expect(f.writes.map((frame) => JSON.parse(frame).t)).toEqual(priorHandoff ? ['order.submit'] : []);
    box.close();
  });
  it('requires atomic compaction and upgrades legacy completed records on restore', async () => {
    const f = fixture();
    const compact = f.options.storage.compact;
    f.options.storage = { ...f.options.storage, compact: undefined } as unknown as OutboxOptions['storage'];
    await expect(DelegatedOutbox.open(f.options)).rejects.toThrow('atomic compaction');
    f.options.storage.compact = compact;
    const legacy: DelegatedIntent[] = [];
    const put = f.options.storage.put;
    f.options.storage.put = async (scope, row) => {
      if (row.state === 'canceled') legacy.push(structuredClone(row));
      await put(scope, row);
    };
    const box = await DelegatedOutbox.open(f.options);
    for (let i = 0; i < 34; i++) {
      f.advance(1);
      await box.cancel(await box.enqueue(params));
    }
    box.close();
    f.options.storage.put = put;
    f.options.storage.load = async () => legacy;
    const restored = await DelegatedOutbox.open(f.options);
    expect(restored.entries()).toHaveLength(32);
    expect(f.rows.size).toBe(32);
    restored.connect(f.connection());
    await restored.enqueue(params);
    expect(JSON.parse(f.writes[0]).d.seq).toBe(35);
    restored.close();
  });
  it('recovers failed initial persistence as terminal work before compaction', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const put = f.options.storage.put;
    f.options.storage.put = async () => {
      throw new Error('synthetic unavailable storage');
    };
    await expect(box.enqueue(params)).rejects.toThrow('storage');
    f.options.storage.put = put;
    await box.flush(box.connect(f.connection()));
    expect([...f.rows.values()].map((row) => row.state)).toEqual(['notSent']);
    expect(f.writes).toEqual([]);
    box.close();
  });
  it('bounds pending work and recovers a slot after a never-sent cancellation', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    let first = '';
    for (let i = 0; i < 1024; i++) {
      f.advance(1);
      const id = await box.enqueue(params);
      if (i === 0) first = id;
    }
    await expect(box.enqueue(params)).rejects.toThrow('full');
    await box.cancel(first);
    f.advance(1);
    await box.enqueue(params);
    expect(box.entries().filter((row) => row.state === 'queued')).toHaveLength(1024);
    expect(f.writes).toEqual([]);
    box.close();
  });
  it('restores an empty checkpoint without resetting the durable sequence', async () => {
    const f = fixture();
    f.options.storage.load = async () => ({ version: 1, sequence: 4096, intents: [] });
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection());
    await box.enqueue(params);
    expect(JSON.parse(f.writes[0]).d.seq).toBe(4097);
    box.close();
  });
  it('keeps retirement pending after storage failure and retries it on flush', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const compact = f.options.storage.compact;
    f.options.storage.compact = async () => {
      throw new Error('synthetic unavailable compaction');
    };
    for (let i = 0; i < 32; i++) {
      f.advance(1);
      await box.cancel(await box.enqueue(params));
    }
    f.advance(1);
    const id = await box.enqueue(params);
    await expect(box.cancel(id)).rejects.toThrow('compaction');
    expect(f.rows.size).toBe(33);
    f.options.storage.compact = compact;
    await box.flush(box.connect(f.connection()));
    expect(f.rows.size).toBe(32);
    expect(box.entries()).toHaveLength(32);
    expect(f.writes).toEqual([]);
    box.close();
  });
  it('keeps a delayed A storage checkpoint scoped while B takes over', async () => {
    const a = fixture('alice');
    const alice = await DelegatedOutbox.open(a.options);
    for (let i = 0; i < 32; i++) {
      a.advance(1);
      await alice.cancel(await alice.enqueue(params));
    }
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const waiting = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const compact = a.options.storage.compact;
    a.options.storage.compact = async (scope, snapshot: OutboxCompaction) => {
      entered();
      await held;
      await compact(scope, snapshot);
    };
    a.advance(1);
    const finishing = alice.cancel(await alice.enqueue(params));
    await waiting;
    alice.close();
    const b = fixture('bob', a.rows);
    const bob = await DelegatedOutbox.open(b.options);
    bob.connect(b.connection());
    const bId = await bob.enqueue(params);
    release();
    await finishing;
    expect(a.writes).toEqual([]);
    expect(b.writes).toHaveLength(1);
    expect(bob.entries()[0]).toMatchObject({ owner: 'bob', intentId: bId, state: 'sending' });
    expect([...a.rows.values()].filter((row) => row.owner === 'bob')).toHaveLength(1);
    bob.close();
  });
  it('preserves newer same-scope records and checkpoint progress during delayed retirement', async () => {
    const a = fixture();
    const first = await DelegatedOutbox.open(a.options);
    for (let i = 0; i < 32; i++) {
      a.advance(1);
      await first.cancel(await first.enqueue(params));
    }
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const waiting = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const compact = a.options.storage.compact;
    a.options.storage.compact = async (scope, checkpoint) => {
      entered();
      await held;
      await compact(scope, checkpoint);
    };
    a.advance(1);
    const retiring = first.cancel(await first.enqueue(params));
    await waiting;
    first.close();
    const b = fixture('alice', a.rows);
    const next = await DelegatedOutbox.open(b.options);
    const c = next.connect(b.connection());
    const id = await next.enqueue(params);
    await next.receive(
      c,
      response(id, 'order.outcome', {
        seq: 34,
        outcomeSeq: 1,
        outcome: 'placed',
        clientOrderId: id,
        venueOrderId: 'native-next',
      }),
    );
    release();
    await retiring;
    expect([...a.rows.values()].find((row) => row.intentId === id)?.state).toBe('placed');
    next.close();
    const restored = await DelegatedOutbox.open(b.options);
    restored.connect(b.connection());
    await restored.enqueue(params);
    expect(JSON.parse(b.writes.at(-1)!).d.seq).toBe(35);
    restored.close();
  });
  it('recovers capacity after completed orders and restores sequence progress', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    for (let seq = 1; seq <= 1025; seq++) {
      const id = await box.enqueue(params);
      await box.receive(
        c,
        response(id, 'order.outcome', {
          seq,
          outcomeSeq: seq,
          outcome: 'placed',
          clientOrderId: id,
          venueOrderId: `native-${seq}`,
        }),
      );
      f.advance(1);
    }
    expect(box.entries().length).toBeLessThanOrEqual(32);
    expect(f.rows.size).toBeLessThanOrEqual(32);
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    restored.connect(f.connection());
    await restored.enqueue(params);
    expect(JSON.parse(f.writes.at(-1)!).d.seq).toBe(1026);
    restored.close();
  });
  it('recovers capacity after never-sent cancellations without replaying retained work', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const uncertain = await box.enqueue(params);
    await box.receive(c, response(uncertain, 'order.outcome', { outcome: 'unknown', clientOrderId: uncertain }));
    f.advance(1);
    const acknowledged = await box.enqueue(params);
    await box.receive(c, response(acknowledged, 'order.ack', { seq: 2, received: true }));
    box.disconnect(c);
    for (let i = 0; i < 1025; i++) {
      f.advance(1);
      await box.cancel(await box.enqueue(params));
    }
    expect(f.rows.size).toBeLessThanOrEqual(34);
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(f.writes).toHaveLength(2);
    expect(restored.entries().find((row) => row.intentId === uncertain)?.state).toBe('unknown');
    expect(restored.entries().find((row) => row.intentId === acknowledged)?.state).toBe('unknown');
    restored.close();
  });
  it('pins all four frozen frame shapes against the shared native fixture', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const id = await box.enqueue(params);
    expect(id).toBe(frames.key);
    expect(JSON.parse(f.writes[0])).toEqual(frames.submit);
    await box.cancel(id);
    expect(JSON.parse(f.writes[1])).toEqual(frames.cancelQueued);
    await box.receive(c, frames.ack);
    expect(box.entries()[0].state).toBe('acknowledged');
    await box.receive(c, frames.outcome);
    expect(box.entries()[0].state).toBe('placed');
    box.close();
  });
  it('requires the advertised capability, bounds receipts and checks per-intent outcome sequencing', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect({ ...f.connection(), capabilities: { delegatedOrderFramesV1: false } });
    const id = await box.enqueue(params);
    expect(f.writes).toEqual([]);
    const c = box.connect(f.connection());
    await box.flush(c);
    await box.receive(c, response(id, 'order.ack', { received: true, padding: 'x'.repeat(4096) }));
    expect(box.entries()[0].state).toBe('sending');
    await box.receive(c, response(id, 'order.outcome', { outcome: 'unknown', clientOrderId: id, outcomeSeq: 2 }));
    await box.receive(
      c,
      response(id, 'order.outcome', {
        outcome: 'placed',
        clientOrderId: id,
        venueOrderId: 'native-one',
        outcomeSeq: 1,
      }),
    );
    expect(box.entries()[0].state).toBe('unknown');
    await box.flush(box.connect(f.connection()));
    expect(f.writes).toHaveLength(1);
    box.close();
  });
  it.each([
    [false, false],
    [false, true],
    [true, false],
    [true, true],
  ])('gates writes and receipts on box frames=%s AND router ledger=%s', async (frames, ledger) => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const advertised = {
      ...f.connection(),
      capabilities: { delegatedOrderFramesV1: frames, delegatedOrderLedgerV1: ledger },
    };
    const c = box.connect(advertised);
    const id = await box.enqueue(params);
    await box.flush(c);
    expect(f.writes).toHaveLength(frames && ledger ? 1 : 0);
    await box.receive(
      c,
      response(id, 'order.outcome', { outcome: 'placed', clientOrderId: id, venueOrderId: 'router-order' }),
    );
    expect(box.entries()[0].state).toBe(frames && ledger ? 'placed' : 'queued');
    box.close();
  });
  it("accepts a router replay without comparing another intent's outcome sequence", async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const replayed = await box.enqueue(params);
    const live = await box.enqueue(params);
    // Hosts may deliver an independent live result before a deferred replay.
    await box.receive(
      c,
      response(live, 'order.outcome', {
        seq: 2,
        outcomeSeq: 42,
        outcome: 'placed',
        clientOrderId: live,
        venueOrderId: 'live-order',
      }),
    );
    await box.receive(
      c,
      response(replayed, 'order.outcome', {
        outcomeSeq: 41,
        outcome: 'placed',
        clientOrderId: replayed,
        venueOrderId: 'replayed-order',
      }),
    );
    expect(box.entries().map((row) => [row.intentId, row.state, row.venueOrderId])).toEqual([
      [replayed, 'placed', 'replayed-order'],
      [live, 'placed', 'live-order'],
    ]);
    // A stale result for the same intent is still discarded.
    await box.receive(
      c,
      response(replayed, 'order.outcome', {
        outcomeSeq: 40,
        outcome: 'unknown',
        clientOrderId: replayed,
      }),
    );
    expect(box.entries()[0].state).toBe('placed');
    box.close();
  });
  it('requires an explicit router capability and captures both advertisements immutably', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const advertised = { ...f.connection(), capabilities: { delegatedOrderFramesV1: true } };
    const c = box.connect(advertised);
    // A caller mutating discovery after connect cannot enable that old ticket.
    Object.assign(advertised.capabilities, { delegatedOrderLedgerV1: true });
    const id = await box.enqueue(params);
    await box.flush(c);
    expect(f.writes).toEqual([]);
    const next = box.connect(f.connection());
    await box.flush(next);
    expect(f.writes).toHaveLength(1);
    expect(JSON.parse(f.writes[0]).d.intentId).toBe(id);
    box.close();
  });
  it('gates deferred queue cancellation on both capabilities too', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection());
    const id = await box.enqueue(params);
    // Capture the cancellation while the handed-off intent is still sending;
    // replacement then retires that captured ticket before cancellation dispatch.
    const cancellation = box.cancel(id);
    const disabled = box.connect({
      ...f.connection(),
      capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: false },
    });
    await cancellation;
    await box.flush(disabled);
    expect(f.writes.map((text) => JSON.parse(text).t)).toEqual(['order.submit']);
    await box.flush(box.connect(f.connection()));
    expect(f.writes.map((text) => JSON.parse(text).t)).toEqual(['order.submit', 'order.cancelQueued']);
    box.close();
  });
  it('accepts renumbered outcomes on a replacement ticket while discarding old socket deliveries', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const old = box.connect(f.connection());
    const id = await box.enqueue(params);
    await box.receive(
      old,
      response(id, 'order.outcome', {
        outcomeSeq: 900,
        outcome: 'unknown',
        clientOrderId: id,
      }),
    );
    // A new physical socket may reuse the registration label, but not its ticket.
    const current = box.connect(f.connection());
    await box.receive(
      old,
      response(id, 'order.outcome', {
        outcomeSeq: 901,
        outcome: 'placed',
        clientOrderId: id,
        venueOrderId: 'old-socket',
      }),
    );
    expect(box.entries()[0].state).toBe('unknown');
    await box.receive(
      current,
      response(id, 'order.outcome', {
        outcomeSeq: 1,
        outcome: 'placed',
        clientOrderId: id,
        venueOrderId: 'replayed-order',
      }),
    );
    await box.flush(current);
    expect(box.entries()[0]).toMatchObject({ state: 'placed', venueOrderId: 'replayed-order' });
    expect(f.writes).toHaveLength(1);
    box.close();
  });
  it('isolates replay sequencing when the owner switches during receipt persistence', async () => {
    const a = fixture();
    const alice = await DelegatedOutbox.open(a.options);
    const old = alice.connect(a.connection());
    const aId = await alice.enqueue(params);
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const waiting = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const put = a.options.storage.put;
    a.options.storage.put = async (scope, row) => {
      if (row.state === 'unknown') {
        entered();
        await held;
      }
      await put(scope, row);
    };
    const receipt = alice.receive(
      old,
      response(aId, 'order.outcome', {
        outcomeSeq: 900,
        outcome: 'unknown',
        clientOrderId: aId,
      }),
    );
    await waiting;
    alice.close();
    const b = fixture('bob', a.rows);
    const bob = await DelegatedOutbox.open(b.options);
    const current = bob.connect(b.connection());
    const bId = await bob.enqueue(params);
    // Deliberately collide account/intent labels; the captured owner is distinct.
    expect(bId).toBe(aId);
    await bob.receive(
      current,
      response(bId, 'order.outcome', {
        outcomeSeq: 1,
        outcome: 'unknown',
        clientOrderId: bId,
      }),
    );
    const stale = alice.receive(
      old,
      response(aId, 'order.outcome', {
        outcomeSeq: 901,
        outcome: 'placed',
        clientOrderId: aId,
        venueOrderId: 'stale',
      }),
    );
    release();
    await Promise.all([receipt, stale]);
    expect(bob.entries()).toMatchObject([{ owner: 'bob', intentId: bId, state: 'unknown' }]);
    expect(b.writes).toHaveLength(1);
    expect(a.writes).toHaveLength(1);
    expect([...a.rows.values()].filter((row) => row.owner === 'alice')).toMatchObject([{ state: 'unknown' }]);
    bob.close();
  });
  it.each(routerReplies.boxReceipts)('pins the box receipt predicate: $name', async (test) => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    await box.enqueue(params);
    const beforeReceipt = box.entries()[0];
    expect(beforeReceipt.state).toBe('sending');
    await expect(box.receive(c, test.frame)).resolves.toBeUndefined();
    // The frozen fixture's queued state denotes an ignored receipt.
    if (test.outboxState === 'queued') expect(box.entries()).toEqual([beforeReceipt]);
    else expect(box.entries()[0].state).toBe(test.outboxState);
    box.close();
  });
  it('does not send before socket admission even with both capabilities advertised', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const pending = box.connect({ ...f.connection(), ready: false });
    const id = await box.enqueue(params);
    await box.flush(pending);
    await box.receive(pending, response(id, 'order.ack', { received: true }));
    expect(f.writes).toEqual([]);
    expect(box.entries()[0].state).toBe('queued');
    await box.flush(box.connect(f.connection()));
    expect(f.writes).toHaveLength(1);
    box.close();
  });
  it.each(routerReplies.cases)('accepts frozen router replies: $name', async (test) => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const id = await box.enqueue(params);
    expect(id).toBe(frames.key);
    if (test.request === 'cancelQueued') {
      await box.cancel(id);
      expect(JSON.parse(f.writes[1])).toEqual(frames.cancelQueued);
    }
    for (const reply of test.replies) {
      if (reply.t === 'order.ack' || reply.t === 'order.outcome') {
        const reference = reply.t === 'order.ack' ? frames.ack : frames.outcome;
        expect(Object.keys(reply.d).sort()).toEqual(Object.keys(reference.d).sort());
        expect(reply.d).toMatchObject({ accountName: 'main', instanceId: 'registration', intentId: id, seq: 1 });
        expect(new TextEncoder().encode(JSON.stringify(reply)).length).toBeLessThanOrEqual(
          reply.t === 'order.ack' ? 4096 : 8192,
        );
      } else {
        expect(reply).toEqual({
          t: 'msg',
          d: { op: 'error', accountName: 'main', data: ['not sent: invalid order binding'] },
        });
      }
      await box.receive(c, JSON.stringify(reply));
    }
    // An ordinary error cannot prove that a handed-off cancellation succeeded.
    expect(box.entries()[0]).toMatchObject({
      state: test.state === 'canceled' ? 'unknown' : test.state,
      cancelPending: test.cancelPending,
    });
    await box.flush(box.connect(f.connection()));
    expect(f.writes.filter((text) => JSON.parse(text).t === 'order.submit')).toHaveLength(1);
    expect(f.writes.filter((text) => JSON.parse(text).t === 'order.cancelQueued')).toHaveLength(
      test.request === 'cancelQueued' ? (test.cancelPending ? 2 : 1) : 0,
    );
    box.close();
  });
  it('accepts independent renumbered results after both handoffs time out unknown', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const replayed = await box.enqueue(params);
    const live = await box.enqueue(params);
    f.advance(10_000);
    expect(box.entries().map((row) => row.state)).toEqual(['unknown', 'unknown']);
    await box.receive(
      c,
      response(live, 'order.outcome', {
        seq: 2,
        outcomeSeq: 42,
        outcome: 'placed',
        clientOrderId: live,
        venueOrderId: 'live-after-deadline',
      }),
    );
    await box.receive(
      c,
      response(replayed, 'order.outcome', {
        outcomeSeq: 41,
        outcome: 'placed',
        clientOrderId: replayed,
        venueOrderId: 'replay-after-deadline',
      }),
    );
    expect(box.entries().map((row) => row.state)).toEqual(['placed', 'placed']);
    await box.flush(box.connect(f.connection()));
    expect(f.writes).toHaveLength(2);
    box.close();
  });
  it('keeps handed-off work unknown when a replacement loses ledger capability', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection());
    const id = await box.enqueue(params);
    const disabled = box.connect({
      ...f.connection(),
      capabilities: {
        delegatedOrderFramesV1: true,
        delegatedOrderLedgerV1: false,
      },
    });
    expect(box.entries()[0].state).toBe('unknown');
    await box.receive(disabled, response(id, 'order.ack', { received: false }));
    f.advance(10_000);
    await box.flush(disabled);
    expect(box.entries()[0].state).toBe('unknown');
    const current = box.connect(f.connection());
    await box.flush(current);
    expect(f.writes).toHaveLength(1);
    await box.receive(
      current,
      response(id, 'order.outcome', {
        outcomeSeq: 1,
        outcome: 'placed',
        clientOrderId: id,
        venueOrderId: 'restored-ledger',
      }),
    );
    expect(box.entries()[0].state).toBe('placed');
    box.close();
  });
  it('uses sortable unique UUIDv7 keys that fit the Bybit client id field', () => {
    const a = createDelegatedIntentId(1, (bytes) => bytes.fill(1));
    const b = createDelegatedIntentId(2, (bytes) => bytes.fill(1));
    expect(a).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(a < b).toBe(true);
  });
  it('persists before handoff, stays unknown on reconnect, and accepts a late receipt', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const id = await box.enqueue(params);
    expect(f.rows.size).toBe(1);
    expect(f.writes).toHaveLength(1);
    const first = JSON.parse(f.writes[0]);
    expect(first.d).toMatchObject({
      accountName: 'main',
      instanceId: 'registration',
      intentId: id,
      seq: 1,
      ageMs: 0,
    });
    box.disconnect(c);
    f.advance(500);
    const next = box.connect(f.connection());
    await box.flush(next);
    expect(f.writes).toHaveLength(1);
    expect(box.entries()[0].state).toBe('unknown');
    await box.receive(next, response(id, 'order.ack', { received: true }));
    const third = box.connect(f.connection());
    await box.flush(third);
    expect(f.writes).toHaveLength(1);
    box.close();
  });
  it('caps every queued order at ten monotonic seconds and only sends queue cancellation afterward', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const id = await box.enqueue(params);
    f.advance(10_000);
    expect(box.entries()[0].state).toBe('notSent');
    const c = box.connect(f.connection());
    await box.flush(c);
    expect(f.writes.every((text) => JSON.parse(text).t !== 'order.submit')).toBe(true);
    expect(box.entries()[0].intentId).toBe(id);
    box.close();
  });
  it('never resends unknown outcomes, including after reload', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const id = await box.enqueue(params);
    await box.receive(c, response(id, 'order.outcome', { outcome: 'unknown', clientOrderId: id }));
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries()[0].state).toBe('unknown');
    expect(f.writes).toHaveLength(1);
    restored.close();
  });
  it('drops A handoff work when login changes during persistence', async () => {
    const a = fixture('alice');
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const waiting = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const put = a.options.storage.put;
    a.options.storage.put = async (scope, row) => {
      entered();
      await held;
      await put(scope, row);
    };
    const alice = await DelegatedOutbox.open(a.options);
    alice.connect(a.connection());
    const queued = alice.enqueue(params);
    await waiting;
    alice.close();
    const b = fixture('bob', a.rows);
    const bob = await DelegatedOutbox.open(b.options);
    bob.connect(b.connection());
    const bId = await bob.enqueue(params);
    release();
    await queued;
    expect(a.writes).toEqual([]);
    expect(b.writes).toHaveLength(1);
    expect(JSON.parse(b.writes[0]).d.intentId).toBe(bId);
    expect(bob.entries().every((row) => row.owner === 'bob')).toBe(true);
    bob.close();
  });
  it('fences the old registration during the final write-ahead wait', async () => {
    const f = fixture();
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const waiting = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const put = f.options.storage.put;
    let hold = true;
    f.options.storage.put = async (scope, row) => {
      if (row.state === 'sending' && hold) {
        hold = false;
        entered();
        await held;
      }
      await put(scope, row);
    };
    const box = await DelegatedOutbox.open(f.options);
    const stale = box.connect(f.connection('old'));
    const queued = box.enqueue(params);
    await waiting;
    const current = box.connect(f.connection('new'));
    const flush = box.flush(current);
    release();
    const id = await queued;
    await flush;
    expect(f.writes).toHaveLength(1);
    expect(JSON.parse(f.writes[0]).d.instanceId).toBe('new');
    await box.receive(stale, response(id, 'order.ack', { received: true }));
    expect(box.entries()[0].state).toBe('sending');
    box.close();
  });
  it('expires synchronously while the physical-handoff persistence is delayed', async () => {
    const f = fixture();
    let release!: () => void;
    let entered!: () => void;
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const waiting = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const put = f.options.storage.put;
    f.options.storage.put = async (scope, row) => {
      if (row.state === 'sending') {
        entered();
        await held;
      }
      await put(scope, row);
    };
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const queued = box.enqueue(params);
    await waiting;
    f.advance(10_000);
    expect(box.entries()[0].state).toBe('notSent');
    release();
    await queued;
    await box.flush(c);
    expect(f.writes).toEqual([]);
    box.close();
  });
  it('does not infer elapsed time from a new clock epoch or retry a restored physical handoff', async () => {
    for (const sent of [false, true]) {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      if (sent) box.connect(f.connection());
      await box.enqueue(params);
      box.close();
      f.options.clock.epoch = 'new-boot';
      const restored = await DelegatedOutbox.open(f.options);
      await restored.flush(restored.connect(f.connection()));
      // A restored queued label has no live proof that an old build never sent.
      expect(restored.entries()[0].state).toBe('unknown');
      expect(f.writes).toHaveLength(sent ? 1 : 0);
      restored.close();
    }
  });
  it('persists unknown before any reconnect even when saving the result fails', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const id = await box.enqueue(params);
    const put = f.options.storage.put;
    f.options.storage.put = async (scope, row) => {
      if (row.state === 'unknown') throw new Error('synthetic unavailable storage');
      await put(scope, row);
    };
    await expect(
      box.receive(c, response(id, 'order.outcome', { outcome: 'unknown', clientOrderId: id })),
    ).rejects.toThrow('storage');
    await expect(box.flush(box.connect(f.connection()))).rejects.toThrow('storage');
    expect(f.writes).toHaveLength(1);
    box.close();
    f.options.storage.put = put;
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries()[0].state).toBe('unknown');
    expect(f.writes).toHaveLength(1);
    restored.close();
  });
  it('retains the same queued intent when preparing handoff cannot be persisted', async () => {
    const f = fixture();
    const put = f.options.storage.put;
    f.options.storage.put = async (scope, row) => {
      if (row.state === 'sending') throw new Error('synthetic unavailable storage');
      await put(scope, row);
    };
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const id = await box.enqueue(params);
    expect(box.entries()).toHaveLength(1);
    expect(box.entries()[0]).toMatchObject({ intentId: id, state: 'queued' });
    expect(f.writes).toEqual([]);
    f.options.storage.put = put;
    await box.flush(c);
    expect(f.writes).toHaveLength(1);
    expect(JSON.parse(f.writes[0]).d.intentId).toBe(id);
    box.close();
  });
  it('resends only queue cancellation after canceling an unacknowledged submit', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection());
    const id = await box.enqueue(params);
    await box.cancel(id);
    await box.flush(box.connect(f.connection()));
    expect(f.writes.map((text) => JSON.parse(text).t)).toEqual([
      'order.submit',
      'order.cancelQueued',
      'order.cancelQueued',
    ]);
    box.close();
  });
  it('rejects wrong owners and ignores stale or foreign reply registrations', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    expect(() => box.connect({ ...f.connection(), owner: 'bob' })).toThrow('scope');
    const c = box.connect(f.connection());
    const id = await box.enqueue(params);
    const frame = response(id, 'order.ack', { received: true });
    frame.d.instanceId = 'stale';
    await box.receive(c, frame);
    frame.d.instanceId = 'registration';
    frame.d.accountName = 'another';
    await box.receive(c, frame);
    expect(box.entries()[0].state).toBe('sending');
    box.close();
  });
  it('does not hand off while pending, after storage failure, or through HTTP', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect({ ...f.connection(), ready: false });
    await box.enqueue(params);
    await box.flush(c);
    expect(f.writes).toEqual([]);
    f.options.storage.put = async () => {
      throw new Error('synthetic storage failure');
    };
    await expect(box.enqueue(params)).rejects.toThrow('storage');
    expect(box.entries()[1].state).toBe('notSent');
    expect(f.writes).toEqual([]);
    box.close();
  });
});
