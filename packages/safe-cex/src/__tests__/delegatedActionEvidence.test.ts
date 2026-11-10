import type { DelegatedAction } from '../delegatedActions';
import type { DelegatedIntent, OutboxConnection, OutboxOptions } from '../delegatedOutbox';

import { describe, expect, it } from 'vitest';

import { delegatedActionFingerprint } from '../delegatedActions';
import { createDelegatedIntentId, DelegatedOutbox } from '../delegatedOutbox';

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
      bytes.fill(0);
      new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).setUint32(bytes.byteLength - 4, ++entropy);
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
    venue: 'bybit',
    signingMode: 'delegated',
    ready: true,
    capabilities: {
      delegatedOrderFramesV1: true,
      delegatedOrderLedgerV1: true,
      delegatedOrderActionsV1: true,
      delegatedOrderActionLedgerV1: true,
    },
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
const cancel: DelegatedAction = { kind: 'cancel', order: { id: 'resting', symbol: 'BTCUSDT' } };
function receipt(box: DelegatedOutbox, id: string, outcome: 'applied' | 'refused' | 'unknown', outcomeSeq = 1) {
  const row = box.entries().find((row) => row.intentId === id)!;
  return {
    t: 'order.actionOutcome',
    d: {
      version: 1,
      accountName: 'main',
      instanceId: 'registration',
      intentId: id,
      seq: row.seq,
      outcomeSeq,
      outcome,
      result: outcome === 'applied' ? { kind: row.action!.kind } : null,
      error:
        outcome === 'applied'
          ? null
          : { message: 'router proof', outcome: outcome === 'refused' ? 'notSent' : 'unknown' },
    },
  };
}
function key(n = 1) {
  return createDelegatedIntentId(1_700_000_000_000, (bytes) => {
    bytes.fill(0);
    new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).setUint32(bytes.byteLength - 4, n);
  });
}

describe('action writes share the handoff evidence model', () => {
  it.each([undefined, 'unknown'] as const)(
    'requires positive proof for a placement refusal sharing the action store (%s)',
    async (errorOutcome) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      const c = box.connect(f.connection());
      const id = await box.enqueue({ symbol: 'BTCUSDT', amount: 1 });
      await box.receive(c, {
        t: 'order.outcome',
        d: {
          accountName: 'main',
          instanceId: 'registration',
          intentId: id,
          seq: 1,
          outcomeSeq: 1,
          outcome: 'refused',
          clientOrderId: null,
          venueOrderId: null,
          error: { message: 'unproven', outcome: errorOutcome },
        },
      });
      expect(box.entries()[0].state).toBe('unknown');
      expect(box.entries()[0]).not.toHaveProperty('notSentEvidence');
      box.close();
    },
  );
  it.each(['refused', 'notSent'] as const)(
    'does not restore an unproven action %s claim as terminal',
    async (state) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      await box.enqueueAction(cancel);
      box.close();
      Object.assign([...f.rows.values()][0], { state });
      const restored = await DelegatedOutbox.open(f.options);
      await restored.flush(restored.connect(f.connection()));
      expect(restored.entries()[0].state).toBe('unknown');
      expect(f.writes).toHaveLength(0);
      restored.close();
    },
  );
  it('F1 preserves durable uncertainty after a failed reconnect without replay', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection());
    const id = await box.enqueueAction(cancel);
    expect(f.writes).toHaveLength(1);
    let reconnects = 0;
    await box.flush(
      box.connect({
        ...f.connection(),
        send() {
          reconnects++;
          return false;
        },
      }),
    );
    expect(reconnects).toBe(0);
    box.close();
    f.advance(10_001);
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries()[0]).toMatchObject({ intentId: id, state: 'unknown' });
    expect([...f.rows.values()][0]).not.toHaveProperty('noHandoffEvidence');
    expect(f.writes).toHaveLength(1);
    restored.close();
  });
  it.each(['deadline', 'disconnect', 'replace', 'cancel', 'close'] as const)(
    'never invents no-handoff proof after %s',
    async (event) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      const c = box.connect(f.connection());
      const id = await box.enqueueAction(cancel);
      if (event === 'deadline') f.advance(10_000);
      if (event === 'disconnect') box.disconnect(c);
      if (event === 'replace') box.connect(f.connection('replacement'));
      if (event === 'cancel') await box.cancel(id);
      if (event === 'close') box.close();
      expect(box.entries()[0]).toMatchObject({ state: 'unknown' });
      expect(box.entries()[0]).not.toHaveProperty('notSentEvidence');
      box.close();
      const restored = await DelegatedOutbox.open(f.options);
      await restored.flush(restored.connect(f.connection()));
      expect(restored.entries()[0].state).toBe('unknown');
      expect(f.writes.filter((wire) => JSON.parse(wire).t === 'order.action')).toHaveLength(1);
      restored.close();
    },
  );
  it.each(['ack', 'outcome'] as const)(
    'brands authoritative action %s refusals and restores their proof',
    async (kind) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      const c = box.connect(f.connection());
      const id = await box.enqueueAction(cancel);
      await box.receive(
        c,
        kind === 'ack'
          ? {
              t: 'order.actionAck',
              d: { version: 1, accountName: 'main', instanceId: 'registration', intentId: id, seq: 1, received: false },
            }
          : receipt(box, id, 'refused'),
      );
      expect(box.entries()[0]).toMatchObject({
        state: 'notSent',
        notSentEvidence: { kind: 'routerRefusal', intentId: id },
      });
      box.close();
      const restored = await DelegatedOutbox.open(f.options);
      expect(restored.entries()[0].state).toBe('notSent');
      await restored.flush(restored.connect(f.connection()));
      expect(f.writes).toHaveLength(1);
      restored.close();
    },
  );
  it('accepts an action ACK after physical handoff and keeps its deadline unknown', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    const id = await box.enqueueAction(cancel);
    await box.receive(c, {
      t: 'order.actionAck',
      d: { version: 1, accountName: 'main', instanceId: 'registration', intentId: id, seq: 1, received: true },
    });
    expect(box.entries()[0].state).toBe('acknowledged');
    f.advance(10_000);
    expect(box.entries()[0].state).toBe('unknown');
    box.close();
  });
  it('replays only a positively declined first action handoff with persisted proof', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect({ ...f.connection(), send: () => false });
    const id = await box.enqueueAction(cancel);
    expect([...f.rows.values()][0]).toMatchObject({
      state: 'queued',
      noHandoffEvidence: { source: 'adapter', handedOff: false },
    });
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(JSON.parse(f.writes[0]).d.intentId).toBe(id);
    expect(f.writes).toHaveLength(1);
    restored.close();
  });
  it.each(['declined', 'throw'] as const)(
    'preserves constructor evidence when the first %s send expires reentrantly',
    async (how) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      box.connect({
        ...f.connection(),
        send() {
          f.advance(10_000);
          if (how === 'throw') throw new Error('not handed off');
          return false;
        },
      });
      await box.enqueueAction(cancel);
      expect(box.entries()[0]).toMatchObject({
        state: 'notSent',
        cancelPending: false,
        notSentEvidence: { kind: 'noHandoff', source: 'adapter', handedOff: false },
      });
      expect(f.writes).toHaveLength(0);
      box.close();
    },
  );
  it.each(['owner', 'seq', 'fingerprint'] as const)(
    'rejects a restored action no-handoff proof with mismatched %s',
    async (field) => {
      const f = fixture();
      const box = await DelegatedOutbox.open(f.options);
      await box.enqueueAction(cancel);
      box.close();
      const saved = [...f.rows.values()][0];
      Object.assign(saved.noHandoffEvidence!, { [field]: field === 'seq' ? 99 : 'foreign' });
      const restored = await DelegatedOutbox.open(f.options);
      await restored.flush(restored.connect(f.connection()));
      expect(restored.entries()[0].state).toBe('unknown');
      expect(f.writes).toHaveLength(0);
      restored.close();
    },
  );
  it.each(Array.from({ length: 16 }, (_, mask) => mask))(
    'requires all four immutable capability flags: %s',
    async (mask) => {
      const f = fixture();
      const connection = f.connection();
      const flags = [
        'delegatedOrderFramesV1',
        'delegatedOrderLedgerV1',
        'delegatedOrderActionsV1',
        'delegatedOrderActionLedgerV1',
      ] as const;
      flags.forEach((flag, i) => (connection.capabilities[flag] = !!(mask & (1 << i))));
      const box = await DelegatedOutbox.open(f.options);
      const c = box.connect(connection);
      const id = await box.enqueueAction(cancel);
      flags.forEach((flag) => (connection.capabilities[flag] = true));
      await box.flush(c);
      expect(f.writes).toHaveLength(mask === 15 ? 1 : 0);
      await box.receive(c, receipt(box, id, 'applied'));
      expect(box.entries()[0].state).toBe(mask === 15 ? 'applied' : 'queued');
      box.close();
    },
  );
  it('indexes applied action history and recovers completed capacity', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    for (let n = 0; n < 40; n++) {
      const id = await box.enqueueAction(cancel);
      await box.receive(c, receipt(box, id, 'applied'));
    }
    expect(box.entries()).toHaveLength(32);
    expect(f.rows.size).toBe(32);
    box.close();
  });
});

describe('caller-owned action identity', () => {
  it('preserves caller UUID and canonical payload across retries and restore without downgrading handoff', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection());
    const id = key(71);
    expect(await box.enqueueAction(cancel, { intentId: id })).toBe(id);
    const first = box.entries()[0];
    expect(first.fingerprint).toBe(delegatedActionFingerprint('main', id, cancel, first.seq));
    expect(
      await box.enqueueAction({ order: { symbol: 'BTCUSDT', id: 'resting' }, kind: 'cancel' }, { intentId: id }),
    ).toBe(id);
    expect(box.entries()[0]).toEqual({ ...first, callerSuppliedIntentId: true });
    expect(box.entries()[0].state).toBe('sending');
    expect(f.writes).toHaveLength(1);
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    restored.connect(f.connection());
    expect(await restored.enqueueAction(cancel, { intentId: id })).toBe(id);
    expect(restored.entries()[0]).toMatchObject({ state: 'unknown', fingerprint: first.fingerprint, seq: first.seq });
    expect(restored.entries()[0]).not.toHaveProperty('noHandoffEvidence');
    expect(f.writes).toHaveLength(1);
    restored.close();
  });
  it('refuses a caller key bound to another action or placement family', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const id = key(72);
    await box.enqueueAction(cancel, { intentId: id });
    await expect(box.enqueueAction({ kind: 'nuke' }, { intentId: id })).rejects.toMatchObject({
      code: 'intentConflict',
    });
    await expect(box.enqueue({}, { intentId: id })).rejects.toMatchObject({ code: 'intentConflict' });
    box.close();
  });
  it('rejects invalid caller UUIDv7 keys before persistence', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    await expect(box.enqueueAction(cancel, { intentId: 'bad' })).rejects.toMatchObject({ code: 'invalidIntentId' });
    expect(f.rows.size).toBe(0);
    box.close();
  });
  it('keeps concurrent identical retries behind the original persistence', async () => {
    const f = fixture();
    let enter!: () => void;
    let release!: () => void;
    const entered = new Promise<void>((r) => (enter = r));
    const gate = new Promise<void>((r) => (release = r));
    const put = f.options.storage.put;
    let blocked = false;
    f.options.storage.put = async (scope, row) => {
      if (!blocked) {
        blocked = true;
        enter();
        await gate;
      }
      await put(scope, row);
    };
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection());
    const id = key(73);
    const first = box.enqueueAction(cancel, { intentId: id });
    await entered;
    let returned = false;
    const retry = box.enqueueAction(cancel, { intentId: id }).then((id) => {
      returned = true;
      return id;
    });
    await Promise.resolve();
    expect(returned).toBe(false);
    release();
    expect(await Promise.all([first, retry])).toEqual([id, id]);
    expect(f.writes).toHaveLength(1);
    expect(box.entries()[0].state).toBe('sending');
    box.close();
  });
  it('does not turn a failed initial put into a durable identity retry', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const id = key(74);
    f.options.storage.put = async () => {
      throw new Error('offline');
    };
    await expect(box.enqueueAction(cancel, { intentId: id })).rejects.toThrow('offline');
    await expect(box.enqueueAction(cancel, { intentId: id })).rejects.toMatchObject({ code: 'intentNotDurable' });
    expect(f.writes).toHaveLength(0);
    box.close();
  });
  it('retains completed caller reservations through history compaction and restore', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection());
    for (let n = 100; n < 140; n++) {
      const id = key(n);
      await box.enqueueAction(cancel, { intentId: id });
      await box.receive(c, receipt(box, id, 'applied'));
    }
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    restored.connect(f.connection());
    const id = key(100);
    expect(await restored.enqueueAction(cancel, { intentId: id })).toBe(id);
    await expect(restored.enqueueAction({ kind: 'nuke' }, { intentId: id })).rejects.toMatchObject({
      code: 'intentConflict',
    });
    expect(f.writes).toHaveLength(40);
    expect(restored.entries().find((row) => row.intentId === id)?.state).toBe('applied');
    restored.close();
  });
});
