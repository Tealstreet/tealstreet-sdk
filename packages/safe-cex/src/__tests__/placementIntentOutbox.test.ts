import type { DelegatedIntent, OutboxOptions, OutboxSnapshot } from '../delegatedOutbox';

import { describe, expect, it } from 'vitest';

import { createDelegatedIntentId, DelegatedOutbox } from '../delegatedOutbox';

const params = { symbol: 'BTCUSDT', type: 'market', side: 'buy', amount: 1000 };
function fixture(version: 1 | 2 = 2) {
  let sequence = 0;
  let floor = 0;
  let now = 1_764_000_000_000;
  let failCompaction = false;
  const rows = new Map<string, DelegatedIntent>();
  const options: OutboxOptions = {
    owner: 'owner-A',
    accountName: 'main',
    accountId: 'account-A',
    venue: 'bybit',
    clock: { epoch: 'boot-A', now: () => now - 1_764_000_000_000, unix: () => now },
    random: (bytes) => {
      bytes.fill(1);
    },
    schedule: () => () => {},
    storage: {
      async load(): Promise<OutboxSnapshot> {
        const intents = structuredClone([...rows.values()]);
        return version === 2
          ? { version, sequence, retiredIntentBefore: floor, intents }
          : { version, sequence, intents };
      },
      async put(_scope, row) {
        rows.set(row.intentId, structuredClone(row));
      },
      async compact(_scope, checkpoint) {
        if (failCompaction) throw new Error('transaction aborted');
        sequence = Math.max(sequence, checkpoint.sequence);
        if (checkpoint.version === 2) floor = Math.max(floor, checkpoint.retiredIntentBefore);
        for (const id of checkpoint.retiredIntentIds) rows.delete(id);
      },
    },
  };
  return {
    options,
    rows,
    floor: () => floor,
    setFloor: (value: number) => {
      floor = value;
    },
    id: () => createDelegatedIntentId(now++, options.random),
    /** Distinct UUIDs that share one timestamp, as `placementLegIntent` children do. */
    legId: (time: number, leg: number) =>
      createDelegatedIntentId(time, (bytes) => {
        bytes.fill(2);
        bytes[bytes.length - 1] = leg;
      }),
    now: () => now,
    advance: (ms: number) => {
      now += ms;
    },
    abort: () => {
      failCompaction = true;
    },
    recover: () => {
      failCompaction = false;
    },
  };
}
async function completed(box: DelegatedOutbox, id: string) {
  await box.enqueue(params, { intentId: id });
  await box.cancel(id); // No connection or handoff: a real queued cancellation.
}

describe('durable caller-intent retirement', () => {
  it('admits more than the lifetime capacity of distinct deliberate actions', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    try {
      for (let i = 0; i < 1060; i++) await completed(box, f.id());
      expect(box.entries().length).toBeLessThanOrEqual(32);
      expect(f.floor()).toBeGreaterThan(0);
    } finally {
      box.close();
    }
  });

  it('refuses a forgotten caller UUID after restoring the durable checkpoint', async () => {
    const f = fixture();
    const first = f.id();
    let box = await DelegatedOutbox.open(f.options);
    await completed(box, first);
    for (let i = 0; i < 33; i++) await completed(box, f.id());
    expect(f.rows.has(first)).toBe(false);
    box.close();
    box = await DelegatedOutbox.open(f.options);
    try {
      await expect(box.enqueue(params, { intentId: first })).rejects.toMatchObject({
        code: 'intentRetired',
        outcome: 'unknown',
      });
      expect(f.rows.has(first)).toBe(false);
    } finally {
      box.close();
    }
  });

  it('keeps an older unanswered intent joinable even below the retirement floor', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const pending = f.id();
    await box.enqueue(params, { intentId: pending });
    for (let i = 0; i < 34; i++) await completed(box, f.id());
    try {
      expect(f.floor()).toBeGreaterThan(0);
      expect(await box.enqueue(params, { intentId: pending })).toBe(pending);
      expect(box.entries().find((row) => row.intentId === pending)?.state).toBe('queued');
    } finally {
      box.close();
    }
  });

  it('an aborted compaction preserves both the old rows and rejection floor', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    for (let i = 0; i < 32; i++) await completed(box, f.id());
    const before = [...f.rows.keys()];
    f.abort();
    await expect(completed(box, f.id())).rejects.toThrow('transaction aborted');
    expect(f.floor()).toBe(0);
    expect(before.every((id) => f.rows.has(id))).toBe(true);
    expect(box.entries()).toHaveLength(33);
    f.recover();
    box.close();
  });

  it('never advances a durable retirement floor past the current protocol clock', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const future = createDelegatedIntentId(0xffffffffffff, f.options.random);
    await completed(box, future);
    try {
      for (let i = 0; i < 34; i++) await completed(box, f.id());
      expect(f.floor()).toBeLessThan(1_764_000_000_100);
      expect(f.rows.has(future)).toBe(true);
      expect(box.entries().length).toBeLessThanOrEqual(32);
    } finally {
      box.close();
    }
  });

  it('a conflict with restored Unknown work cannot claim the original was not sent', async () => {
    const f = fixture();
    const { owner, accountName, venue, accountId } = f.options;
    let writes = 0;
    let box = await DelegatedOutbox.open(f.options);
    box.connect({
      owner,
      accountName,
      venue,
      accountId,
      instanceId: 'registration',
      signingMode: 'delegated',
      ready: true,
      capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: true },
      send: () => {
        writes++;
        return true;
      },
    });
    const id = f.id();
    await box.enqueue(params, { intentId: id });
    expect(box.entries()[0].state).toBe('sending');
    box.close();
    box = await DelegatedOutbox.open(f.options);
    try {
      expect(box.entries()[0].state).toBe('unknown');
      await expect(box.enqueue({ ...params, amount: 2000 }, { intentId: id })).rejects.toMatchObject({
        code: 'intentConflict',
        outcome: 'unknown',
      });
      expect(writes).toBe(1);
    } finally {
      box.close();
    }
  });

  it('explicit retirement cannot move the durable floor past a rolled-back clock', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const id = await box.enqueue(params);
    await box.cancel(id);
    f.options.clock.unix = () => 1_763_999_999_999;
    try {
      expect(await box.retire(id)).toBe(false);
      expect(f.floor()).toBe(0);
      expect(f.rows.has(id)).toBe(true);
    } finally {
      box.close();
    }
  });

  it('legacy storage keeps permanent caller reservations instead of forgetting replay protection', async () => {
    const f = fixture(1);
    const box = await DelegatedOutbox.open(f.options);
    const first = f.id();
    await completed(box, first);
    for (let i = 0; i < 34; i++) await completed(box, f.id());
    expect(f.rows.has(first)).toBe(true);
    expect(await box.enqueue(params, { intentId: first })).toBe(first);
    expect(f.floor()).toBe(0);
    box.close();
  });

  it('refuses malformed floors before mutating persisted rows', async () => {
    const f = fixture();
    f.options.storage.load = async () => ({ version: 2, sequence: 0, retiredIntentBefore: NaN, intents: [] });
    await expect(DelegatedOutbox.open(f.options)).rejects.toThrow('Invalid outbox checkpoint');
    expect(f.rows.size).toBe(0);
  });
});

// AGT-106 review (rvJ F1/F2, rvK 2-5): the floor must never refuse an ID the box could still
// accept (key window: 10 s old, 5 s ahead), and a clock step must not lock out DELEGATED writes.
describe('retirement floor against the box key window', () => {
  it('admits every sequential leg of one parent action, past 33 completions', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const parent = f.now();
    try {
      for (let leg = 0; leg < 40; leg++) {
        await completed(box, f.legId(parent, leg));
        f.advance(50);
      }
      expect(f.floor()).toBe(0);
    } finally {
      box.close();
    }
  });

  it('keeps every row inside the key window and retires only older ones', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    try {
      for (let i = 0; i < 1060; i++) {
        await completed(box, f.id());
        f.advance(1_000);
      }
      expect(box.entries().length).toBeLessThanOrEqual(32 + 16);
      expect(f.floor()).toBeGreaterThan(0);
      expect(f.floor()).toBeLessThanOrEqual(f.now() - 15_000);
    } finally {
      box.close();
    }
  });

  it('still mints and sends its own ids after a fast clock is corrected', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const day = 86_400_000;
    f.advance(day);
    for (let i = 0; i < 40; i++) {
      await completed(box, f.id());
      f.advance(1_000);
    }
    expect(f.floor()).toBeGreaterThan(f.now() - day);
    f.advance(-day);
    try {
      const id = await box.enqueue(params);
      expect(box.entries().find((row) => row.intentId === id)?.state).toBe('queued');
    } finally {
      box.close();
    }
  });

  it('does not refuse an id it minted itself after the clock steps back', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    for (let i = 0; i < 40; i++) {
      await completed(box, f.id());
      f.advance(1_000);
    }
    f.advance(-60_000);
    try {
      const id = await box.enqueue(params);
      expect(box.entries().find((row) => row.intentId === id)?.state).toBe('queued');
    } finally {
      box.close();
    }
  });

  it('refuses a checkpoint whose floor is ahead of the clock as invalid', async () => {
    const f = fixture();
    f.setFloor(f.now() + 5_001);
    await expect(DelegatedOutbox.open(f.options)).rejects.toThrow('Invalid outbox checkpoint');
    f.setFloor(f.now() + 5_000);
    (await DelegatedOutbox.open(f.options)).close();
  });

  it('refuses a caller id more than 5 s ahead as invalid and not sent', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    try {
      await expect(
        box.enqueue(params, { intentId: createDelegatedIntentId(f.now() + 5_001, f.options.random) }),
      ).rejects.toMatchObject({ code: 'invalidIntentId' });
      expect(f.rows.size).toBe(0);
      const edge = createDelegatedIntentId(f.now() + 5_000, f.options.random);
      expect(await box.enqueue(params, { intentId: edge })).toBe(edge);
    } finally {
      box.close();
    }
  });
});

