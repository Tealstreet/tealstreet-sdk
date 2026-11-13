import type { DelegatedAction } from '../delegatedActions';
import type { OutboxConnection, OutboxOptions } from '../delegatedOutbox';
import type { ExchangeName } from '../types';

import { describe, expect, it } from 'vitest';

import { delegatedIntentFingerprint } from '../delegatedOrderFrames';
import { DelegatedOutbox } from '../delegatedOutbox';

const params = { symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 0.001 };

async function fixture(owner = 'alice', venue: ExchangeName = 'bybit') {
  const writes: string[] = [];
  let entropy = 0;
  const options: OutboxOptions = {
    owner,
    accountName: 'main',
    venue,
    accountId: 'account-A',
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
    delegatedActivationScopeV1: actions,
    delegatedVenueV1: 'bybit',
  };
  const connection = {
    owner,
    accountName: 'main',
    accountId: 'account-A',
    venue,
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
  it('activates an admitted non-Bybit hosted venue without changing order bytes', async () => {
    const f = await fixture('alice', 'binance');
    const control = await fixture();
    f.box.activate(
      {
        ...f.connection,
        venue: 'binance',
        capabilities: {
          ...f.connection.capabilities,
          delegatedVenueV1: 'binance',
        },
      },
      { DELEGATED: true },
    );
    control.box.activate(control.connection, { DELEGATED: true });
    await f.box.enqueueAction('place', params);
    await control.box.enqueueAction('place', params);
    expect(f.writes).toEqual(control.writes);
    expect(f.writes).toHaveLength(1);
    f.box.close();
    control.box.close();
  });

  it.each([undefined, null, 'binance'])('refuses missing or different-venue capabilities (%s)', async (venue) => {
    const f = await fixture();
    expect(() =>
      f.box.activate(
        {
          ...f.connection,
          capabilities: {
            ...f.connection.capabilities,
            delegatedVenueV1: venue,
          },
        },
        { DELEGATED: true },
      ),
    ).toThrow(expect.objectContaining({ code: 'delegatedVenueUnavailable' }));
    expect(f.writes).toEqual([]);
    f.box.close();
  });
  it('activates with DELEGATED and the two supported actions while later actions are absent', async () => {
    const f = await fixture();
    f.box.activate(f.connection, { DELEGATED: true });
    await f.box.enqueueAction('place', params);
    await f.box.enqueueAction('close', { ...params, reduceOnly: true });
    expect(f.writes.map((text) => JSON.parse(text).t)).toEqual(['order.submit', 'order.submit']);
    expect(JSON.parse(f.writes[1]).d.params.reduceOnly).toBe(true);
    f.box.close();
  });

  it('does not mistake dedicated-action support for place/close activation scope', async () => {
    const f = await fixture();
    const connection = {
      ...f.connection,
      capabilities: {
        ...f.connection.capabilities,
        delegatedActivationScopeV1: undefined,
        delegatedOrderActionsV1: true,
        delegatedOrderActionLedgerV1: true,
      },
    };
    expect(() => f.box.activate(connection, { DELEGATED: true })).toThrow(
      expect.objectContaining({ code: 'delegatedPlaceUnavailable' }),
    );
    expect(f.box.entries()).toEqual([]);
    expect(f.writes).toEqual([]);
    f.box.close();
  });

  it('keeps placement and every dedicated action available on the same admitted ticket', async () => {
    const f = await fixture();
    const connection = {
      ...f.connection,
      capabilities: {
        ...f.connection.capabilities,
        delegatedOrderActionsV1: true,
        delegatedOrderActionLedgerV1: true,
      },
    };
    const ticket = f.box.activate(connection, { DELEGATED: true });
    expect(ticket.capabilities.delegatedOrderActionsV1).toBe(true);
    expect(ticket.capabilities.delegatedActivationScopeV1?.close).toBe('reduceOnly');
    await f.box.enqueueAction('place', params);
    await f.box.enqueueAction('close', { ...params, reduceOnly: true });
    const actions: DelegatedAction[] = [
      { kind: 'amend', order: { id: 'existing', symbol: 'BTCUSDT' }, update: { price: 50001 } },
      { kind: 'cancel', order: { id: 'existing', symbol: 'BTCUSDT' } },
      { kind: 'cancelAll' },
      { kind: 'nuke' },
    ];
    for (const action of actions) {
      await expect(f.box.enqueueAction(action.kind, params)).rejects.toMatchObject({
        code: 'delegatedActionUnavailable',
      });
      await f.box.enqueueAction(action);
    }
    expect(f.writes.map((frame) => JSON.parse(frame).t)).toEqual([
      'order.submit',
      'order.submit',
      ...actions.map(() => 'order.action'),
    ]);
    expect(f.writes.slice(2).map((frame) => JSON.parse(frame).d.action)).toEqual(actions);
    f.box.close();
  });

  it.each(['delegatedOrderActionsV1', 'delegatedOrderActionLedgerV1'] as const)(
    'activation scope does not override the dedicated-action %s gate',
    async (capability) => {
      const f = await fixture();
      const connection = {
        ...f.connection,
        capabilities: {
          ...f.connection.capabilities,
          delegatedOrderActionsV1: true,
          delegatedOrderActionLedgerV1: true,
          [capability]: false,
        },
      };
      f.box.activate(connection, { DELEGATED: true });
      await f.box.enqueueAction({ kind: 'cancelAll' });
      expect(f.writes).toEqual([]);
      await f.box.enqueueAction('close', { ...params, reduceOnly: true });
      expect(f.writes.map((frame) => JSON.parse(frame).t)).toEqual(['order.submit']);
      f.box.close();
    },
  );

  it.each([
    { flag: false, place: true, reason: 'delegatedDisabled' },
    { flag: true, place: false, reason: 'delegatedPlaceUnavailable' },
    { flag: true, place: undefined, reason: 'delegatedPlaceUnavailable' },
  ])('visibly refuses activation before sending ($reason, place=$place)', async ({ flag, place, reason }) => {
    const f = await fixture();
    const connection = {
      ...f.connection,
      capabilities: {
        ...f.connection.capabilities,
        delegatedActivationScopeV1: { ...f.connection.capabilities.delegatedActivationScopeV1, place },
      },
    };
    expect(() => f.box.activate(connection, { DELEGATED: flag })).toThrow(expect.objectContaining({ code: reason }));
    expect(f.writes).toEqual([]);
    f.box.close();
  });

  it.each(['amend', 'cancel', 'cancelAll', 'nuke'] as const)(
    '%s is named and refused before any frame',
    async (action) => {
      const f = await fixture();
      f.box.activate(f.connection, { DELEGATED: true });
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
        delegatedActivationScopeV1: { ...f.connection.capabilities.delegatedActivationScopeV1, close },
      },
    };
    expect(() => f.box.activate(connection, { DELEGATED: true })).toThrow(
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
      f.box.activate(f.connection, { DELEGATED: true });
      await expect(f.box.enqueueAction('close', { ...params, reduceOnly })).rejects.toMatchObject({
        name: 'DelegatedReduceOnlyRequired',
        code: 'delegatedReduceOnlyRequired',
      });
      expect(f.writes).toEqual([]);
      expect(f.box.entries()).toEqual([]);
      f.box.close();
    },
  );

  it.each(['inherited', 'non-enumerable'] as const)(
    'refuses a serialized close whose %s reduce-only flag disappears',
    async (kind) => {
      const f = await fixture();
      const stored: unknown[] = [];
      f.options.storage.put = async (_scope, row) => {
        stored.push(structuredClone(row));
      };
      const close =
        kind === 'inherited'
          ? Object.assign(Object.create({ reduceOnly: true }), params)
          : Object.defineProperty({ ...params }, 'reduceOnly', { value: true, enumerable: false });
      f.box.activate(f.connection, { DELEGATED: true });
      await expect(f.box.enqueueAction('close', close)).rejects.toMatchObject({
        code: 'delegatedReduceOnlyRequired',
      });
      expect(stored).toEqual([]);
      expect(f.box.entries()).toEqual([]);
      expect(f.writes).toEqual([]);
      f.box.close();
    },
  );

  it('captures a changing close getter once before validation, fingerprinting and persistence', async () => {
    const f = await fixture();
    const stored: Record<string, unknown>[] = [];
    f.options.storage.put = async (_scope, row) => {
      stored.push(structuredClone(row.params));
    };
    let reads = 0;
    const close = {
      ...params,
      get reduceOnly() {
        return ++reads === 1;
      },
    };
    f.box.activate(f.connection, { DELEGATED: true });
    await f.box.enqueueAction('close', close);
    expect(f.writes).toHaveLength(1);
    const { d } = JSON.parse(f.writes[0]);
    expect(d.params.reduceOnly).toBe(true);
    expect(reads).toBe(1);
    expect(stored.every((value) => JSON.stringify(value) === JSON.stringify(d.params))).toBe(true);
    expect(d.fingerprint).toBe(delegatedIntentFingerprint(d.accountName, d.intentId, d.params, d.seq));
    f.box.close();
  });

  it.each([undefined, false])('validates the JSON replacement close flag (%s)', async (reduceOnly) => {
    const f = await fixture();
    let captures = 0;
    const close = {
      ...params,
      reduceOnly: true,
      toJSON() {
        captures++;
        return { ...params, reduceOnly };
      },
    };
    f.box.activate(f.connection, { DELEGATED: true });
    await expect(f.box.enqueueAction('close', close)).rejects.toMatchObject({
      code: 'delegatedReduceOnlyRequired',
    });
    expect(captures).toBe(1);
    expect(f.box.entries()).toEqual([]);
    expect(f.writes).toEqual([]);
    f.box.close();
  });

  it('keeps the captured close stable while persistence is deferred', async () => {
    const f = await fixture();
    let release!: () => void;
    let entered!: () => void;
    const started = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const stored: Record<string, unknown>[] = [];
    f.options.storage.put = async (_scope, row) => {
      stored.push(structuredClone(row.params));
      entered();
      await gate;
    };
    f.box.activate(f.connection, { DELEGATED: true });
    const close = { ...params, reduceOnly: true };
    const pending = f.box.enqueueAction('close', close);
    await started;
    close.reduceOnly = false;
    close.amount = 10;
    release();
    await pending;
    const { d } = JSON.parse(f.writes[0]);
    expect(d.params).toEqual({ ...params, reduceOnly: true });
    expect(stored.length).toBeGreaterThan(0);
    expect(stored.every((value) => JSON.stringify(value) === JSON.stringify(d.params))).toBe(true);
    expect(d.fingerprint).toBe(delegatedIntentFingerprint(d.accountName, d.intentId, d.params, d.seq));
    f.box.close();
  });

  it.each(['delegatedOrderFramesV1', 'delegatedOrderLedgerV1'] as const)(
    'keeps %s required per connection',
    async (capability) => {
      const f = await fixture();
      f.connection.capabilities[capability] = false;
      expect(() => f.box.activate(f.connection, { DELEGATED: true })).toThrow(
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

  it('retires an admitted connection when the host disables DELEGATED during persistence', async () => {
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
    f.box.activate(f.connection, { DELEGATED: true });
    const pending = f.box.enqueueAction('place', params);
    await started;
    expect(() => f.box.activate(f.connection, { DELEGATED: false })).toThrow(
      expect.objectContaining({
        code: 'delegatedDisabled',
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
    alice.box.activate(alice.connection, { DELEGATED: true });
    const stale = alice.box.enqueueAction('place', params);
    await started;
    alice.box.close();
    const bob = await fixture('bob');
    bob.box.activate(bob.connection, { DELEGATED: true });
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
    const ticket = f.box.activate(f.connection, { DELEGATED: true });
    f.connection.capabilities.delegatedActivationScopeV1.place = false;
    expect(ticket.capabilities.delegatedActivationScopeV1?.place).toBe(true);
    expect(Object.isFrozen(ticket.capabilities.delegatedActivationScopeV1)).toBe(true);
    await f.box.enqueueAction('close', { ...params, reduceOnly: true });
    expect(f.writes).toHaveLength(1);
    expect(() => f.box.activate(f.connection, { DELEGATED: true })).toThrow(
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
