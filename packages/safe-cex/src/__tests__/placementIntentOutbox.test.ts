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
    owner: 'owner-A', accountName: 'main', accountId: 'account-A', venue: 'bybit',
    clock: { epoch: 'boot-A', now: () => now - 1_764_000_000_000, unix: () => now },
    random: (bytes) => { bytes.fill(1); },
    schedule: () => () => {},
    storage: {
      async load(): Promise<OutboxSnapshot> {
        const intents = structuredClone([...rows.values()]);
        return version === 2 ? { version, sequence, retiredIntentBefore: floor, intents } : { version, sequence, intents };
      },
      async put(_scope, row) { rows.set(row.intentId, structuredClone(row)); },
      async compact(_scope, checkpoint) {
        if (failCompaction) throw new Error('transaction aborted');
        sequence = Math.max(sequence, checkpoint.sequence);
        if (checkpoint.version === 2) floor = Math.max(floor, checkpoint.retiredIntentBefore);
        for (const id of checkpoint.retiredIntentIds) rows.delete(id);
      },
    },
  };
  return { options, rows, floor: () => floor,
    id: () => createDelegatedIntentId(now++, options.random),
    abort: () => { failCompaction = true; },
    recover: () => { failCompaction = false; },
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
    } finally { box.close(); }
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
      await expect(box.enqueue(params, { intentId: first })).rejects.toMatchObject({ code: 'intentRetired', outcome: 'unknown' });
      expect(f.rows.has(first)).toBe(false);
    } finally { box.close(); }
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
    } finally { box.close(); }
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
