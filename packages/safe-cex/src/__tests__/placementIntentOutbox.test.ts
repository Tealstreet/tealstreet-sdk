import type { DelegatedIntent, OutboxOptions, OutboxSnapshot } from '../delegatedOutbox';

import { describe, expect, it } from 'vitest';

import { delegatedIntentTime } from '../delegatedOrderFrames';
import { createDelegatedIntentId, DelegatedOutbox } from '../delegatedOutbox';

const params = { symbol: 'BTCUSDT', type: 'market', side: 'buy', amount: 1000 };
function fixture(version: 1 | 2 = 2) {
  let sequence = 0;
  let floor = 0;
  let now = 1_764_000_000_000;
  let skew = 0; // Wall-clock error only: the monotonic clock never steps.
  let failCompaction = false;
  const rows = new Map<string, DelegatedIntent>();
  const options: OutboxOptions = {
    owner: 'owner-A',
    accountName: 'main',
    accountId: 'account-A',
    venue: 'bybit',
    clock: { epoch: 'boot-A', now: () => now - 1_764_000_000_000, unix: () => now + skew },
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
    /** A host upgrading its adapter to the version-2 checkpoint (floor starts at 0). */
    setVersion: (value: 1 | 2) => {
      version = value;
    },
    id: () => createDelegatedIntentId(now++ + skew, options.random),
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
    skew: (ms: number) => {
      skew = ms;
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
      for (let i = 0; i < 1060; i++) {
        await completed(box, f.id());
        f.advance(1_000);
      }
      // 32 completed rows, plus those still inside the 15 s key window.
      expect(box.entries().length).toBeLessThanOrEqual(32 + 16);
      expect(f.floor()).toBeGreaterThan(0);
    } finally {
      box.close();
    }
  });

  // CodeRabbit (AGT-106): completed rows inside the key window can fill the total cap. Once they
  // age out, an enqueue with no other compaction trigger must reclaim them instead of refusing.
  it('reclaims aged completed rows when a later enqueue finds the outbox full', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    try {
      for (let i = 0; i < 1056; i++) await completed(box, f.id());
      await expect(box.enqueue(params, { intentId: f.id() })).rejects.toThrow('Outbox is closed or full');
      f.advance(16_000);
      const id = f.id();
      expect(await box.enqueue(params, { intentId: id })).toBe(id);
      expect(box.entries().length).toBeLessThanOrEqual(32 + 1);
    } finally {
      box.close();
    }
  });

  it('refuses a forgotten caller UUID after restoring the durable checkpoint', async () => {
    const f = fixture();
    const first = f.id();
    let box = await DelegatedOutbox.open(f.options);
    await completed(box, first);
    for (let i = 0; i < 50; i++) {
      f.advance(1_000);
      await completed(box, f.id());
    }
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
    for (let i = 0; i < 50; i++) {
      f.advance(1_000);
      await completed(box, f.id());
    }
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
    f.advance(20_000); // Past the key window, so the next completion compacts.
    const before = [...f.rows.keys()];
    f.abort();
    await expect(completed(box, f.id())).rejects.toThrow('transaction aborted');
    expect(f.floor()).toBe(0);
    expect(before.every((id) => f.rows.has(id))).toBe(true);
    expect(box.entries()).toHaveLength(33);
    f.recover();
    box.close();
  });

  // Was "never advances ... past the current protocol clock", which admitted a 0xffffffffffff
  // id: such an id is now refused at ingress (see the key-window block below), and the floor
  // stays a full key window behind the clock rather than just behind it.
  it('never advances a durable retirement floor into the key window', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    try {
      for (let i = 0; i < 40; i++) await completed(box, f.id());
      expect(f.floor()).toBe(0);
      expect(box.entries()).toHaveLength(40);
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
    f.advance(16_000);
    // No durable floor: even past the key window the host cannot drop the reservation.
    expect(await box.retire(first)).toBe(false);
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
    f.skew(day);
    for (let i = 0; i < 40; i++) {
      await completed(box, f.id());
      f.advance(1_000);
    }
    expect(f.floor()).toBeGreaterThan(f.now());
    f.skew(0);
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
    f.skew(-60_000);
    try {
      const id = await box.enqueue(params);
      expect(box.entries().find((row) => row.intentId === id)?.state).toBe('queued');
    } finally {
      box.close();
    }
  });

  // bjfpyh: a floor ahead of the clock is what a corrected fast clock leaves behind. It only
  // gates caller ids (Unknown, fail-safe), so it must reopen; only a floor past UUIDv7 is corrupt.
  it('reopens after a fast clock is corrected and still sends its own ids', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    f.skew(86_400_000);
    for (let i = 0; i < 40; i++) {
      await completed(box, f.id());
      f.advance(1_000);
    }
    box.close();
    f.skew(0);
    expect(f.floor()).toBeGreaterThan(f.now());
    const reopened = await DelegatedOutbox.open(f.options);
    try {
      const id = await reopened.enqueue(params);
      expect(reopened.entries().find((row) => row.intentId === id)?.state).toBe('queued');
      await expect(reopened.enqueue(params, { intentId: f.legId(f.now(), 1) })).rejects.toMatchObject({
        code: 'intentRetired',
      });
    } finally {
      reopened.close();
    }
  });

  it('refuses a checkpoint whose floor is beyond the UUIDv7 time range as invalid', async () => {
    const f = fixture();
    f.setFloor(2 ** 48 + 1);
    await expect(DelegatedOutbox.open(f.options)).rejects.toThrow('Invalid outbox checkpoint');
    f.setFloor(2 ** 48);
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

// AGT-106 item 1: caller-keyed rows (the action intent as the outbox key) must not exhaust the
// 1,056-row total. Settled ones retire once past the key window; pending and Unknown ones never
// do for capacity, and the timers stay off here so nothing settles behind the test's back.
describe('caller-keyed row retention', () => {
  /** Records every frame handed to the socket, so a resend of any intent is visible. */
  function connection(f: ReturnType<typeof fixture>, frames: string[]) {
    const { owner, accountName, venue, accountId } = f.options;
    return {
      owner,
      accountName,
      venue,
      accountId,
      instanceId: 'registration',
      signingMode: 'delegated' as const,
      ready: true,
      capabilities: { delegatedOrderFramesV1: true, delegatedOrderLedgerV1: true },
      send: (frame: string) => {
        frames.push(frame);
        return true;
      },
    };
  }
  const submits = (frames: string[], id: string) =>
    frames.filter((frame) => {
      const { t, d } = JSON.parse(frame) as { t: string; d: { intentId: string } };
      return t === 'order.submit' && d.intentId === id;
    }).length;

  it('a total full of settled caller rows does not block trading after a relaunch', async () => {
    const f = fixture();
    let box = await DelegatedOutbox.open(f.options);
    for (let i = 0; i < 1056; i++) await completed(box, f.id());
    await expect(box.enqueue(params, { intentId: f.id() })).rejects.toThrow('Outbox is closed or full');
    box.close();
    expect(f.rows.size).toBe(1056);
    f.advance(16_000);
    box = await DelegatedOutbox.open(f.options);
    try {
      expect(box.entries()).toHaveLength(32);
      // Settled history holds none of the pending capacity: a full pending burst still fits.
      for (let i = 0; i < 1024; i++) await box.enqueue(params, { intentId: f.id() });
      await expect(box.enqueue(params)).rejects.toThrow('Outbox is closed or full');
      expect(box.entries().filter((row) => row.state === 'queued')).toHaveLength(1024);
    } finally {
      box.close();
    }
  });

  it('never evicts a pending or Unknown caller row to make room, and never resends it', async () => {
    const f = fixture();
    const box = await DelegatedOutbox.open(f.options);
    const frames: string[] = [];
    const live = box.connect(connection(f, frames));
    const unknown = f.id();
    await box.enqueue(params, { intentId: unknown });
    box.disconnect(live);
    const pending = f.id();
    await box.enqueue(params, { intentId: pending });
    try {
      for (let i = 0; i < 1100; i++) {
        await completed(box, f.id());
        f.advance(100);
      }
      expect(f.floor()).toBeGreaterThan(delegatedIntentTime(pending));
      expect(box.entries().length).toBeLessThanOrEqual(2 + 32 + 151);
      const kept = box.entries().filter((row) => row.intentId === unknown || row.intentId === pending);
      expect(kept.map((row) => [row.intentId, row.state, row.retentionExpired])).toEqual([
        [unknown, 'unknown', undefined],
        [pending, 'queued', undefined],
      ]);
      expect(f.rows.has(unknown) && f.rows.has(pending)).toBe(true);
      expect(await box.retire(pending)).toBe(false);
      // Below the floor, both still join their own rows instead of refusing or sending again.
      const again = box.connect(connection(f, frames));
      expect(await box.enqueue(params, { intentId: unknown })).toBe(unknown);
      expect(await box.enqueue(params, { intentId: pending })).toBe(pending);
      await box.flush(again);
      expect(submits(frames, unknown)).toBe(1);
    } finally {
      box.close();
    }
  });

  it('a relaunch keeps the key window, the floor and 24 h Unknown retention', async () => {
    const f = fixture();
    const states: [string, boolean | undefined][] = [];
    let box = await DelegatedOutbox.open(f.options);
    const frames: string[] = [];
    box.connect(connection(f, frames));
    const unknown = f.id();
    await box.enqueue(params, { intentId: unknown });
    box.close(); // Handed off: it restores Unknown.
    box = await DelegatedOutbox.open(f.options);
    const settled: string[] = [];
    for (let i = 0; i < 100; i++) {
      f.advance(1_000);
      settled.push(f.id());
      await completed(box, settled[i]);
    }
    box.close();

    box = await DelegatedOutbox.open(f.options);
    try {
      const ids = new Set(box.entries().map((row) => row.intentId));
      expect(box.entries().find((row) => row.intentId === unknown)?.state).toBe('unknown');
      await box.flush(box.connect(connection(f, frames)));
      expect(submits(frames, unknown)).toBe(1);
      // Rows inside the key window survive the reopen; only older settled rows retire.
      const inWindow = settled.filter((id) => delegatedIntentTime(id) >= f.now() - 15_000);
      expect(inWindow.length).toBeGreaterThan(0);
      expect(inWindow.every((id) => ids.has(id))).toBe(true);
      expect(ids.size).toBe(1 + Math.max(32, inWindow.length));
      expect(f.floor()).toBeGreaterThan(0);
      expect(f.floor()).toBeLessThanOrEqual(f.now() - 15_000);
      await expect(box.enqueue(params, { intentId: settled[0] })).rejects.toMatchObject({
        code: 'intentRetired',
        outcome: 'unknown',
      });
    } finally {
      box.close();
    }

    f.advance(24 * 60 * 60 * 1000);
    f.options.changed = (row) => {
      if (row.intentId === unknown) states.push([row.state, row.retentionExpired]);
    };
    box = await DelegatedOutbox.open(f.options);
    try {
      await completed(box, f.id()); // Drains the expiry write queued by the reopen.
      // Expired Unknown history is surfaced as such, never as not sent, then retires.
      expect(states).toContainEqual(['unknown', true]);
      expect(states.every(([state]) => state === 'unknown')).toBe(true);
      expect(f.rows.has(unknown)).toBe(false);
      await expect(box.enqueue(params, { intentId: unknown })).rejects.toMatchObject({
        code: 'intentRetired',
        outcome: 'unknown',
      });
      await box.flush(box.connect(connection(f, frames)));
      expect(submits(frames, unknown)).toBe(1);
    } finally {
      box.close();
    }
  });

  // NX retires Unknown rows it has reconciled (and algo actions answered "dispatched"); caller
  // keys must not make those calls no-ops, or 1,024 of them block every write for 24 h.
  it('the host can retire caller-keyed Unknown rows that fill the pending cap', async () => {
    const f = fixture();
    let box = await DelegatedOutbox.open(f.options);
    const frames: string[] = [];
    const live = box.connect(connection(f, frames));
    const unknowns: string[] = [];
    for (let i = 0; i < 1024; i++) {
      unknowns.push(f.id());
      await box.enqueue(params, { intentId: unknowns[i] });
    }
    box.disconnect(live);
    try {
      await expect(box.enqueue(params)).rejects.toThrow('Outbox is closed or full');
      // Inside the key window the box could still admit a replay, so the row stays.
      expect(await box.retire(unknowns[1023])).toBe(false);
      f.advance(16_000);
      for (const id of unknowns) expect(await box.retire(id)).toBe(true);
      expect(f.rows.size).toBe(0);
      expect(f.floor()).toBeGreaterThan(delegatedIntentTime(unknowns[1023]));
      const again = box.connect(connection(f, frames));
      const id = f.id();
      expect(await box.enqueue(params, { intentId: id })).toBe(id);
      expect(await box.enqueue(params)).toEqual(expect.any(String));
      await expect(box.enqueue(params, { intentId: unknowns[0] })).rejects.toMatchObject({
        code: 'intentRetired',
        outcome: 'unknown',
      });
      await box.flush(again);
    } finally {
      box.close();
    }
    box = await DelegatedOutbox.open(f.options);
    try {
      await expect(box.enqueue(params, { intentId: unknowns[0] })).rejects.toMatchObject({
        code: 'intentRetired',
        outcome: 'unknown',
      });
      await box.flush(box.connect(connection(f, frames)));
      expect(unknowns.every((id) => submits(frames, id) === 1)).toBe(true);
    } finally {
      box.close();
    }
  });

  // CodeRabbit (AGT-106): a cancel accepted while retirement awaits compaction must not be
  // published and then deleted with the row; it runs after retirement settles instead.
  async function overlappingCancel(f: ReturnType<typeof fixture>, abort = false) {
    const box = await DelegatedOutbox.open(f.options);
    const live = box.connect(connection(f, []));
    const id = f.id();
    await box.enqueue(params, { intentId: id });
    box.disconnect(live); // Handed off: Unknown, and a cancel stays pending.
    f.advance(16_000);
    const cancelPending: boolean[] = [];
    f.options.changed = (row) => {
      if (row.intentId === id) cancelPending.push(row.cancelPending);
    };
    const compact = f.options.storage.compact;
    let release: (() => void) | undefined;
    f.options.storage.compact = async (scope, checkpoint) => {
      await new Promise<void>((resolve) => (release = resolve));
      if (abort) throw new Error('transaction aborted');
      return compact(scope, checkpoint);
    };
    const retired = box.retire(id);
    while (!release) await new Promise((resolve) => setTimeout(resolve, 0));
    const canceled = box.cancel(id);
    f.options.storage.compact = compact;
    release();
    return { box, id, cancelPending, retired, canceled };
  }

  it('a cancel overlapping a retirement that lands finds no row to cancel', async () => {
    const f = fixture();
    const { box, id, cancelPending, retired, canceled } = await overlappingCancel(f);
    try {
      expect(await retired).toBe(true);
      await canceled;
      expect(cancelPending).not.toContain(true);
      expect(f.rows.has(id)).toBe(false);
      expect(box.entries().some((row) => row.intentId === id)).toBe(false);
    } finally {
      box.close();
    }
  });

  it('a cancel overlapping a failed retirement is still persisted', async () => {
    const f = fixture();
    const { box, id, retired, canceled } = await overlappingCancel(f, true);
    try {
      await expect(retired).rejects.toThrow('transaction aborted');
      await canceled;
      expect(f.rows.get(id)).toMatchObject({ state: 'unknown', cancelPending: true });
    } finally {
      box.close();
    }
  });

  it('upgrading legacy storage to the checkpoint frees caller rows that had filled it', async () => {
    const f = fixture(1);
    let box = await DelegatedOutbox.open(f.options);
    const first = f.id();
    await completed(box, first);
    for (let i = 1; i < 1056; i++) await completed(box, f.id());
    f.advance(16_000);
    await expect(box.enqueue(params, { intentId: f.id() })).rejects.toThrow('Outbox is closed or full');
    box.close();

    f.setVersion(2);
    box = await DelegatedOutbox.open(f.options);
    try {
      expect(box.entries()).toHaveLength(32);
      expect(f.floor()).toBeGreaterThan(delegatedIntentTime(first));
      const id = f.id();
      expect(await box.enqueue(params, { intentId: id })).toBe(id);
      await expect(box.enqueue(params, { intentId: first })).rejects.toMatchObject({ code: 'intentRetired' });
    } finally {
      box.close();
    }
  });
});
