import type { DelegatedIntent, OutboxConnection, OutboxOptions } from '../delegatedOutbox';
import type { ExchangeName } from '../types';

import { setImmediate } from 'node:timers/promises';

import { describe, expect, it } from 'vitest';

import { DelegatedOutbox } from '../delegatedOutbox';

const params = { symbol: 'BTCUSDT', side: 'buy', type: 'limit', amount: 0.001, price: 50_000 };

function fixture(venue: ExchangeName = 'bybit', accountId = 'account-A', rows: DelegatedIntent[] = []) {
  const writes: string[] = [];
  let entropy = 0;
  const options = {
    owner: 'alice',
    accountName: 'main',
    venue,
    accountId,
    clock: { epoch: 'fixture-boot', now: () => 0, unix: () => 1_700_000_000_000 },
    random: (bytes: Uint8Array) => {
      bytes.fill(++entropy);
    },
    schedule: () => () => undefined,
    storage: {
      load: async () => structuredClone(rows),
      put: async (_scope: unknown, row: DelegatedIntent) => {
        const index = rows.findIndex((saved) => saved.intentId === row.intentId);
        if (index < 0) rows.push(structuredClone(row));
        else rows[index] = structuredClone(row);
      },
      compact: async () => undefined,
    },
  } satisfies OutboxOptions;
  const connection = {
    owner: options.owner,
    accountName: options.accountName,
    venue,
    accountId,
    signingMode: 'delegated',
    ready: true,
    instanceId: 'registration-A',
    capabilities: {
      delegatedOrderFramesV1: true,
      delegatedOrderLedgerV1: true,
      delegatedOrderActionsV1: true,
      delegatedOrderActionLedgerV1: true,
      delegatedVenueV1: venue,
      delegatedActivationScopeV1: { place: true, close: 'reduceOnly' },
    },
    send: (frame: string) => {
      writes.push(frame);
      return true;
    },
  } satisfies OutboxConnection;
  return { options, connection, rows, writes };
}

describe('DELEGATED immutable account binding', () => {
  it('rechecks each restored record after a storage deferral', async () => {
    const original = fixture();
    const box = await DelegatedOutbox.open(original.options);
    await box.enqueue(params);
    await box.enqueue(params);
    box.close();
    const loaded = structuredClone(original.rows);
    const persisted: DelegatedIntent[] = [];
    original.options.storage.load = async () => loaded;
    original.options.storage.put = async (_scope, row) => {
      persisted.push(structuredClone(row));
      loaded[1].venue = 'binance';
      loaded[1].accountId = 'account-B';
    };
    await expect(DelegatedOutbox.open(original.options)).rejects.toThrow();
    expect(persisted).toHaveLength(1);
    expect(persisted[0]).toMatchObject({ venue: 'bybit', accountId: 'account-A' });
  });
  for (const useActivation of [false, true]) {
    for (const replacement of [
      { venue: 'binance', accountId: 'account-B' },
      { venue: 'bybit', accountId: 'account-B' },
    ] satisfies Array<{ venue: ExchangeName; accountId: string }>) {
      it(`refuses live ${replacement.venue} / ${replacement.accountId} replacement (activate=${useActivation})`, async () => {
        const original = fixture();
        const box = await DelegatedOutbox.open(original.options);
        const declined = { ...original.connection, send: () => false };
        const ticket = useActivation ? box.activate(declined, { DELEGATED: true }) : box.connect(declined);
        await box.enqueue(params);
        expect(box.entries()[0].state).toBe('queued');
        const other = fixture(replacement.venue, replacement.accountId);
        expect(() =>
          useActivation ? box.activate(other.connection, { DELEGATED: true }) : box.connect(other.connection),
        ).toThrow();
        await box.flush(ticket);
        expect(other.writes).toEqual([]);
        box.close();
      });

      it(`refuses persisted ${replacement.venue} / ${replacement.accountId} replacement (activate=${useActivation})`, async () => {
        const original = fixture();
        const box = await DelegatedOutbox.open(original.options);
        box.connect({ ...original.connection, send: () => false });
        await box.enqueue(params);
        box.close();
        const before = structuredClone(original.rows);
        const other = fixture(replacement.venue, replacement.accountId, original.rows);
        await expect(DelegatedOutbox.open(other.options)).rejects.toThrow();
        expect(original.rows).toEqual(before);
        expect(other.writes).toEqual([]);
      });
    }
  }

  it('preserves complete order frames on same-account reconnect and restore', async () => {
    const control = fixture();
    const controlBox = await DelegatedOutbox.open(control.options);
    controlBox.connect(control.connection);
    await controlBox.enqueue(params);
    const original = fixture();
    const box = await DelegatedOutbox.open(original.options);
    box.connect({ ...original.connection, send: () => false });
    await box.enqueue(params);
    box.close();
    const restored = await DelegatedOutbox.open(original.options);
    const ticket = restored.connect(original.connection);
    await restored.flush(ticket);
    expect(original.writes).toEqual(control.writes);
    expect(original.writes).toHaveLength(1);
    restored.close();
    controlBox.close();
  });

  it('does not adopt unbound legacy records into the current account', async () => {
    const original = fixture();
    const box = await DelegatedOutbox.open(original.options);
    await box.enqueue(params);
    box.close();
    const legacy = JSON.parse(JSON.stringify(original.rows));
    delete legacy[0].venue;
    delete legacy[0].accountId;
    const other = fixture('binance', 'account-B', legacy);
    await expect(DelegatedOutbox.open(other.options)).rejects.toThrow();
    expect(other.writes).toEqual([]);
  });

  it('rejects a foreign account proof even if its enclosing record was retagged', async () => {
    const original = fixture();
    const box = await DelegatedOutbox.open(original.options);
    await box.enqueue(params);
    box.close();
    original.rows[0].venue = 'binance';
    original.rows[0].accountId = 'account-B';
    const other = fixture('binance', 'account-B', original.rows);
    const restored = await DelegatedOutbox.open(other.options);
    await restored.flush(restored.connect(other.connection));
    expect(other.writes).toEqual([]);
    expect(restored.entries()[0].state).toBe('unknown');
    restored.close();
  });

  it('retires deferred handoff when a replacement account is refused', async () => {
    const original = fixture();
    let release: () => void = () => undefined;
    const paused = new Promise<void>((resolve) => {
      release = resolve;
    });
    const put = original.options.storage.put;
    original.options.storage.put = async (scope, row) => {
      await paused;
      await put(scope, row);
    };
    const box = await DelegatedOutbox.open(original.options);
    box.connect(original.connection);
    const queued = box.enqueue(params);
    const other = fixture('binance', 'account-B');
    expect(() => box.connect(other.connection)).toThrow();
    release();
    await queued;
    expect(original.writes).toEqual([]);
    expect(other.writes).toEqual([]);
    box.close();
  });
});

describe('DELEGATED failed restore lifecycle', () => {
  for (const failure of ['binding', 'validation', 'storage', 'compaction', 'schedule']) {
    for (const retry of [false, true]) {
      it(`retires ${failure} failure callbacks (retry writer=${retry})`, async () => {
        const original = fixture();
        const seed = await DelegatedOutbox.open(original.options);
        await seed.enqueue(params);
        await seed.enqueue(params);
        if (failure === 'compaction') {
          for (let i = 0; i < 33; i++) await seed.enqueue(params);
        }
        seed.close();
        const loaded = structuredClone(original.rows);
        if (failure === 'compaction') {
          for (const row of loaded.slice(2)) row.state = 'canceled';
        }
        if (failure === 'validation') loaded[1].seq = 0;
        let now = 0;
        let attempts = 0;
        let writes = 0;
        let schedules = 0;
        const tasks: Array<{ callback: () => void; cancelled: boolean }> = [];
        const options: OutboxOptions = {
          ...original.options,
          clock: { ...original.options.clock, now: () => now },
          schedule: (callback) => {
            if (++schedules === 2 && failure === 'schedule') throw new Error('synthetic schedule failure');
            const task = { callback, cancelled: false };
            tasks.push(task);
            return () => {
              task.cancelled = true;
            };
          },
          storage: {
            load: async () => loaded,
            put: async (scope, row) => {
              if (++attempts === 2 && failure === 'storage') throw new Error('synthetic storage failure');
              writes++;
              await original.options.storage.put(scope, row);
              if (failure === 'binding') loaded[1].accountId = 'replaced-account';
            },
            compact: async () => {
              if (failure === 'compaction') throw new Error('synthetic compaction failure');
            },
          },
        };
        await expect(DelegatedOutbox.open(options)).rejects.toThrow();
        let writer: DelegatedOutbox | undefined;
        if (retry) {
          writer = await DelegatedOutbox.open({
            ...options,
            schedule: () => () => undefined,
            storage: {
              ...options.storage,
              load: async () => structuredClone(original.rows.slice(0, 1)),
              compact: async () => undefined,
            },
          });
          const ticket = writer.connect(original.connection);
          await writer.flush(ticket);
          expect(original.writes).toHaveLength(1);
          writer.disconnect(ticket);
          await writer.flush(ticket);
          expect(original.rows[0].state).toBe('unknown');
        }
        const before = structuredClone(original.rows);
        const writesBefore = writes;
        now = 10_000;
        // Deliver even canceled callbacks, like an already queued host callback.
        for (const task of tasks) task.callback();
        await setImmediate();
        expect(writes).toBe(writesBefore);
        expect(original.rows).toEqual(before);
        expect(tasks.every((task) => task.cancelled)).toBe(true);
        writer?.close();
      });
    }
  }

  it('does not schedule expiry while a later restore write is pending', async () => {
    const original = fixture();
    const seed = await DelegatedOutbox.open(original.options);
    await seed.enqueue(params);
    await seed.enqueue(params);
    seed.close();
    const callbacks: Array<() => void> = [];
    let now = 0;
    let attempts = 0;
    let entered: () => void = () => undefined;
    let rejectWrite: (error: Error) => void = () => undefined;
    const waiting = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const blocked = new Promise<void>((_resolve, reject) => {
      rejectWrite = reject;
    });
    const options: OutboxOptions = {
      ...original.options,
      clock: { ...original.options.clock, now: () => now },
      schedule: (callback) => {
        callbacks.push(callback);
        return () => undefined;
      },
      storage: {
        ...original.options.storage,
        put: async (scope, row) => {
          if (++attempts === 2) {
            entered();
            await blocked;
          }
          await original.options.storage.put(scope, row);
        },
      },
    };
    const rejected = expect(DelegatedOutbox.open(options)).rejects.toThrow('synthetic deferred failure');
    await waiting;
    const before = structuredClone(original.rows);
    now = 10_000;
    for (const callback of callbacks) callback();
    await setImmediate();
    const during = structuredClone(original.rows);
    rejectWrite(new Error('synthetic deferred failure'));
    await rejected;
    expect(during).toEqual(before);
    expect(callbacks).toHaveLength(0);
  });
});
