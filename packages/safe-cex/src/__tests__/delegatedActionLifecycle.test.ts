import type { DelegatedIntent, OutboxConnection, OutboxOptions } from '../delegatedOutbox';

import { describe, expect, it } from 'vitest';

import { DelegatedOutbox } from '../delegatedOutbox';

function harness() {
  const rows = new Map<string, DelegatedIntent>();
  const options: OutboxOptions = {
    owner: 'alice',
    accountName: 'main',
    venue: 'bybit',
    accountId: 'account-A',
    storage: {
      async load() {
        return [...rows.values()].map((row) => structuredClone(row));
      },
      async put(_, row) {
        rows.set(row.intentId, structuredClone(row));
      },
      async compact(_, checkpoint) {
        for (const id of checkpoint.retiredIntentIds) rows.delete(id);
      },
    },
    clock: { epoch: 'boot', now: () => 0, unix: () => 1700000000000 },
    random(bytes) {
      bytes.fill(1);
    },
    schedule() {
      return () => {};
    },
  };
  const connection: OutboxConnection = {
    owner: 'alice',
    accountName: 'main',
    accountId: 'account-A',
    venue: 'bybit',
    signingMode: 'delegated',
    ready: true,
    capabilities: {
      delegatedOrderFramesV1: true,
      delegatedOrderLedgerV1: true,
      delegatedOrderActionsV1: true,
      delegatedOrderActionLedgerV1: true,
    },
    send() {
      return true;
    },
  };
  return { options, connection };
}

describe('action receipt wire bounds', () => {
  it.each([false, true])('counts received UTF-8 bytes before JSON normalization, padded=%s', async (padded) => {
    const f = harness();
    const box = await DelegatedOutbox.open(f.options);
    const ticket = box.connect(f.connection);
    const id = await box.enqueueAction({ kind: 'cancel', order: { id: 'resting', symbol: 'BTCUSDT' } });
    const before = box.entries()[0];
    const compact = JSON.stringify({
      t: 'order.actionOutcome',
      d: {
        version: 1,
        accountName: 'main',
        intentId: id,
        seq: 1,
        outcomeSeq: 1,
        outcome: 'applied',
        result: { kind: 'cancel' },
        error: null,
      },
    });
    const wire = compact + (padded ? ' '.repeat(9000) : '');
    expect(JSON.stringify(JSON.parse(wire))).toBe(compact);
    expect(new TextEncoder().encode(wire).length > 8192).toBe(padded);
    await box.receive(ticket, wire);
    if (padded) {
      expect(box.entries()[0]).toEqual(before);
      // Ignoring the oversized receipt must not advance delivery sequencing.
      await box.receive(ticket, compact);
    }
    expect(box.entries()[0].state).toBe('applied');
    box.close();
  });

  it.each([false, true])('applies the smaller ACK wire bound, padded=%s', async (padded) => {
    const f = harness();
    const box = await DelegatedOutbox.open(f.options);
    const ticket = box.connect(f.connection);
    const id = await box.enqueueAction({ kind: 'cancelAll' });
    const before = box.entries()[0];
    const compact = JSON.stringify({
      t: 'order.actionAck',
      d: {
        version: 1,
        accountName: 'main',
        intentId: id,
        seq: 1,
        received: true,
      },
    });
    const wire = compact + (padded ? ' '.repeat(4096) : '');
    expect(JSON.stringify(JSON.parse(wire))).toBe(compact);
    expect(new TextEncoder().encode(wire).length).toBeLessThan(8192);
    expect(new TextEncoder().encode(wire).length > 4096).toBe(padded);
    await box.receive(ticket, wire);
    if (padded) {
      expect(box.entries()[0]).toEqual(before);
      // Ignoring the oversized receipt must not advance delivery sequencing.
      await box.receive(ticket, compact);
    }
    expect(box.entries()[0].state).toBe('acknowledged');
    box.close();
  });
});
