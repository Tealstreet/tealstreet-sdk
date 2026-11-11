import type { OutboxConnection, OutboxOptions } from '../delegatedOutbox';

import { describe, expect, it } from 'vitest';

import { DelegatedOutbox } from '../delegatedOutbox';

const params = { symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 0.001 };

async function fixture(owner = 'alice') {
  const writes: string[] = [];
  let entropy = 0;
  const options: OutboxOptions = {
    owner,
    accountName: 'main',
    clock: { epoch: 'synthetic-boot', now: () => 0, unix: () => 1_700_000_000_000 },
    random: (bytes) => bytes.fill(++entropy),
    schedule: () => () => undefined,
    storage: { load: async () => [], put: async () => undefined, compact: async () => undefined },
  };
  const actions = {
    place: true,
    close: 'reduceOnly' as const,
    amend: false,
    cancel: false,
    cancelAll: false,
    nuke: false,
  };
  const capabilities = {
    delegatedOrderFramesV1: true,
    delegatedOrderLedgerV1: true,
    delegatedOrderActionsV1: actions,
  };
  const connection = {
    owner,
    accountName: 'main',
    venue: 'bybit' as const,
    signingMode: 'delegated' as const,
    ready: true,
    capabilities,
    send(frame: string) {
      writes.push(frame);
      return true;
    },
  } satisfies OutboxConnection;
  return { box: await DelegatedOutbox.open(options), connection, options, writes };
}

describe('DELEGATED place and reduce-only close activation', () => {
  it('activates with DELEGATED_BYBIT and the two supported actions while later actions are absent', async () => {
    const f = await fixture();
    f.box.activate(f.connection, { DELEGATED_BYBIT: true });
    await f.box.enqueueAction('place', params);
    await f.box.enqueueAction('close', { ...params, reduceOnly: true });
    expect(f.writes.map((text) => JSON.parse(text).t)).toEqual(['order.submit', 'order.submit']);
    expect(JSON.parse(f.writes[1]).d.params.reduceOnly).toBe(true);
    f.box.close();
  });

  it.each([
    { flag: false, place: true, reason: 'delegatedBybitDisabled' },
    { flag: true, place: false, reason: 'delegatedPlaceUnavailable' },
    { flag: true, place: undefined, reason: 'delegatedPlaceUnavailable' },
  ])('visibly refuses activation before sending ($reason, place=$place)', async ({ flag, place, reason }) => {
    const f = await fixture();
    const connection = {
      ...f.connection,
      capabilities: {
        ...f.connection.capabilities,
        delegatedOrderActionsV1: { ...f.connection.capabilities.delegatedOrderActionsV1, place },
      },
    };
    expect(() => f.box.activate(connection, { DELEGATED_BYBIT: flag })).toThrow(
      expect.objectContaining({ code: reason }),
    );
    expect(f.writes).toEqual([]);
    f.box.close();
  });

  it.each(['amend', 'cancel', 'cancelAll', 'nuke'] as const)(
    '%s is named and refused before any frame',
    async (action) => {
      const f = await fixture();
      f.box.activate(f.connection, { DELEGATED_BYBIT: true });
      await expect(f.box.enqueueAction(action, params)).rejects.toMatchObject({
        name: 'DelegatedActionUnavailable',
        code: 'delegatedActionUnavailable',
      });
      expect(f.writes).toEqual([]);
      expect(f.box.entries()).toEqual([]);
      // A positive control proves that the refusal did not disable all writes.
      await f.box.enqueueAction('place', params);
      expect(f.writes).toHaveLength(1);
      f.box.close();
    },
  );

  it.each(['unsupported', undefined] as const)('requires reduce-only close support (%s)', async (close) => {
    const f = await fixture();
    const connection = {
      ...f.connection,
      capabilities: {
        ...f.connection.capabilities,
        delegatedOrderActionsV1: { ...f.connection.capabilities.delegatedOrderActionsV1, close },
      },
    };
    expect(() => f.box.activate(connection, { DELEGATED_BYBIT: true })).toThrow(
      expect.objectContaining({
        code: 'delegatedCloseUnavailable',
      }),
    );
    expect(f.writes).toEqual([]);
    f.box.close();
  });

  it.each([false, undefined, null, 'true', 1])(
    'refuses a close without literal reduceOnly=true (%s)',
    async (reduceOnly) => {
      const f = await fixture();
      f.box.activate(f.connection, { DELEGATED_BYBIT: true });
      await expect(f.box.enqueueAction('close', { ...params, reduceOnly })).rejects.toMatchObject({
        name: 'DelegatedReduceOnlyRequired',
        code: 'delegatedReduceOnlyRequired',
      });
      expect(f.writes).toEqual([]);
      expect(f.box.entries()).toEqual([]);
      f.box.close();
    },
  );

  it.each(['delegatedOrderFramesV1', 'delegatedOrderLedgerV1'] as const)(
    'keeps %s required per connection',
    async (capability) => {
      const f = await fixture();
      f.connection.capabilities[capability] = false;
      expect(() => f.box.activate(f.connection, { DELEGATED_BYBIT: true })).toThrow(
        expect.objectContaining({
          code:
            capability === 'delegatedOrderFramesV1'
              ? 'delegatedOrderFramesUnavailable'
              : 'delegatedOrderLedgerUnavailable',
        }),
      );
      expect(f.writes).toEqual([]);
      f.box.close();
    },
  );

  it('retires an admitted connection when the host disables DELEGATED_BYBIT during persistence', async () => {
    const f = await fixture();
    let release!: () => void;
    let entered!: () => void;
    const started = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    f.options.storage.put = async () => {
      entered();
      await gate;
    };
    f.box.activate(f.connection, { DELEGATED_BYBIT: true });
    const pending = f.box.enqueueAction('place', params);
    await started;
    expect(() => f.box.activate(f.connection, { DELEGATED_BYBIT: false })).toThrow(
      expect.objectContaining({
        code: 'delegatedBybitDisabled',
      }),
    );
    release();
    await pending;
    expect(f.writes).toEqual([]);
    f.box.close();
  });

  it('keeps the creating owner when login changes during activated action persistence', async () => {
    const alice = await fixture('alice');
    let release!: () => void;
    let entered!: () => void;
    const started = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    alice.options.storage.put = async () => {
      entered();
      await gate;
    };
    alice.box.activate(alice.connection, { DELEGATED_BYBIT: true });
    const stale = alice.box.enqueueAction('place', params);
    await started;
    alice.box.close();
    const bob = await fixture('bob');
    bob.box.activate(bob.connection, { DELEGATED_BYBIT: true });
    await bob.box.enqueueAction('close', { ...params, reduceOnly: true });
    release();
    await stale;
    expect(alice.writes).toEqual([]);
    expect(bob.writes).toHaveLength(1);
    expect(alice.box.entries()[0].owner).toBe('alice');
    expect(bob.box.entries()[0].owner).toBe('bob');
    bob.box.close();
  });

  it('captures action capabilities and requires new admission after discovery changes', async () => {
    const f = await fixture();
    const ticket = f.box.activate(f.connection, { DELEGATED_BYBIT: true });
    f.connection.capabilities.delegatedOrderActionsV1.place = false;
    expect(ticket.capabilities.delegatedOrderActionsV1?.place).toBe(true);
    expect(Object.isFrozen(ticket.capabilities.delegatedOrderActionsV1)).toBe(true);
    await f.box.enqueueAction('close', { ...params, reduceOnly: true });
    expect(f.writes).toHaveLength(1);
    expect(() => f.box.activate(f.connection, { DELEGATED_BYBIT: true })).toThrow(
      expect.objectContaining({
        code: 'delegatedPlaceUnavailable',
      }),
    );
    await expect(f.box.enqueueAction('place', params)).rejects.toMatchObject({
      code: 'delegatedConnectionNotAdmitted',
    });
    expect(f.writes).toHaveLength(1);
    f.box.close();
  });
});
