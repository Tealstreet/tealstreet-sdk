import type { DelegatedIntent, OutboxConnection, OutboxOptions } from '../delegatedOutbox';

import { describe, expect, it } from 'vitest';

import frames from '../../../../sdk-rs/hosts/server/tests/data/delegated-orders-v1.json';
import { createDelegatedIntentId, DelegatedOutbox } from '../delegatedOutbox';

function fixture(owner = 'alice', rows = new Map<string, DelegatedIntent>()) {
  let now = 0;
  let entropy = 0;
  const timers = new Set<{ at: number; run: () => void }>();
  const writes: string[] = [];
  const key = (scope: { owner: string; accountName: string }, id: string) =>
    JSON.stringify([scope.owner, scope.accountName, id]);
  const options: OutboxOptions = {
    owner,
    accountName: 'main',
    storage: {
      async load(scope) {
        return [...rows.values()]
          .filter((row) => row.owner === scope.owner && row.accountName === scope.accountName)
          .map((row) => structuredClone(row));
      },
      async put(scope, row) {
        rows.set(key(scope, row.intentId), structuredClone(row));
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
    venue: 'bybit',
    signingMode: 'delegated',
    ready: true,
    capabilities: { delegatedOrderFramesV1: true },
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
  it('requires the advertised capability, bounds receipts and checks per-account outcome sequencing', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    box.connect({ ...f.connection(), capabilities: { delegatedOrderFramesV1: false } });
    const id = await box.enqueue(params);
    expect(f.writes).toEqual([]);
    const c = box.connect(f.connection());
    await box.flush(c);
    await box.receive(c, response(id, 'order.ack', { received: true, padding: 'x'.repeat(4096) }));
    expect(box.entries()[0].state).toBe('queued');
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
  it('uses sortable unique UUIDv7 keys that fit the Bybit client id field', () => {
    const a = createDelegatedIntentId(1, (bytes) => bytes.fill(1));
    const b = createDelegatedIntentId(2, (bytes) => bytes.fill(1));
    expect(a).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(a < b).toBe(true);
  });
  it('persists before handoff, resends the same intent on reconnect, then stops at ack', async () => {
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
    expect(f.writes).toHaveLength(2);
    expect(JSON.parse(f.writes[1]).d).toMatchObject({ intentId: id, ageMs: 500, seq: 1 });
    await box.receive(next, response(id, 'order.ack', { received: true }));
    const third = box.connect(f.connection());
    await box.flush(third);
    expect(f.writes).toHaveLength(2);
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
    expect(box.entries()[0].state).toBe('queued');
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
      expect(restored.entries()[0].state).toBe(sent ? 'unknown' : 'notSent');
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
    await box.flush(box.connect(f.connection()));
    expect(f.writes).toHaveLength(1);
    box.close();
    f.options.storage.put = put;
    const restored = await DelegatedOutbox.open(f.options);
    await restored.flush(restored.connect(f.connection()));
    expect(restored.entries()[0].state).toBe('unknown');
    expect(f.writes).toHaveLength(1);
    restored.close();
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
    expect(box.entries()[0].state).toBe('queued');
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
