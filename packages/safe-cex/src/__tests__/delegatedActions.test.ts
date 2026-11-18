import type { DelegatedAction } from '../delegatedActions';
import type { DelegatedIntent, OutboxConnection, OutboxOptions } from '../delegatedOutbox';

import { describe, expect, it } from 'vitest';

import golden from '../../../../sdk-rs/hosts/server/tests/data/delegated-actions-v1.json';
import fullWrites from '../../../../sdk-rs/hosts/server/tests/data/delegated-full-writes-v1.json';
import { delegatedActionFingerprint, isDelegatedAction, isDelegatedFullWriteAction } from '../delegatedActions';
import { DelegatedOutbox } from '../delegatedOutbox';
import { OrderSide, OrderType } from '../types';

const actions: DelegatedAction[] = [
  { kind: 'amend', order: { id: 'existing', symbol: 'BTCUSDT' }, update: { price: 50001, amount: 0.002 } },
  { kind: 'cancel', order: { id: 'existing', symbol: 'BTCUSDT' } },
  { kind: 'cancelAll' },
  { kind: 'nuke' },
  ...fullWrites.map(({ action }) => action as unknown as DelegatedAction),
];
function setup(owner = 'alice', rows = new Map<string, DelegatedIntent>()) {
  const writes: string[] = [];
  const timers: (() => void)[] = [];
  const options: OutboxOptions = {
    owner,
    accountName: 'main',
    venue: 'bybit',
    accountId: 'account-A',
    storage: {
      async load(scope) {
        return [...rows.values()].filter((r) => r.owner === scope.owner);
      },
      async put(scope, row) {
        rows.set(`${scope.owner}:${row.intentId}`, structuredClone(row));
      },
      async compact(scope, checkpoint) {
        for (const id of checkpoint.retiredIntentIds) rows.delete(`${scope.owner}:${id}`);
      },
    },
    clock: { epoch: 'boot', now: () => 0, unix: () => 1700000000000 },
    random(bytes) {
      bytes.fill(1);
    },
    schedule(run) {
      timers.push(run);
      return () => {};
    },
  };
  const connection: OutboxConnection = {
    owner,
    accountName: 'main',
    accountId: 'account-A',
    venue: 'bybit',
    signingMode: 'delegated',
    ready: true,
    capabilities: {
      delegatedOrderFramesV1: true,
      delegatedOrderLedgerV1: true,
      delegatedOrderActionsV1: true,
      delegatedFullWriteActionsV1: true,
      delegatedFullWriteLedgerV1: true,
      delegatedOrderActionLedgerV1: true,
    },
    send(frame) {
      writes.push(frame);
      return true;
    },
  };
  return { options, connection, writes, rows, timers };
}
const outcome = (id: string, action: DelegatedAction, seq = 1) => ({
  t: 'order.actionOutcome',
  d: {
    version: 1,
    accountName: 'main',
    intentId: id,
    seq,
    outcomeSeq: 41,
    outcome: action.kind === 'algo' ? 'unknown' : 'applied',
    result:
      action.kind === 'algo'
        ? null
        : ['amend', 'place', 'close'].includes(action.kind)
          ? { kind: action.kind, orderIds: ['changed'] }
          : { kind: action.kind },
    error: action.kind === 'algo' ? { message: 'Asynchronous operation', outcome: 'unknown' } : null,
  },
});

describe('DELEGATED versioned actions', () => {
  it('preserves a typed unsupported v1 outcome across restore without replay or a fabricated notSent proof', async () => {
    const f = setup();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection);
    const action: DelegatedAction = { kind: 'setAllLeverage', leverage: 3 };
    const id = await box.enqueueAction(action);
    const sent = JSON.parse(f.writes[0]);
    expect(sent.d.version).toBe(1);
    expect(sent.d.action).not.toHaveProperty('version');
    const frame = outcome(id, action);
    Object.assign(frame.d, {
      outcome: 'unsupported',
      result: null,
      error: { message: 'Unsupported v1 field', outcome: 'unsupported' },
    });
    for (const invalid of [
      { ...frame.d, version: 2 },
      { ...frame.d, seq: 2 },
      { ...frame.d, result: { kind: action.kind } },
      { ...frame.d, error: { message: 'Not a schema rejection', outcome: 'notSent' } },
    ]) {
      await box.receive(c, { ...frame, d: invalid });
      expect(box.entries()[0].state).toBe('sending');
    }
    await box.receive(c, frame);
    expect(box.entries()[0]).toMatchObject({ state: 'unsupported', error: { outcome: 'unsupported' } });
    expect(box.entries()[0].notSentEvidence).toBeUndefined();
    // Later queue uncertainty / stale business results cannot revoke rejection.
    await box.receive(c, {
      ...frame,
      d: { ...frame.d, outcomeSeq: 42, outcome: 'unknown', error: { message: 'uncertain', outcome: 'unknown' } },
    });
    await box.receive(c, { ...outcome(id, action), d: { ...outcome(id, action).d, outcomeSeq: 43 } });
    expect(box.entries()[0].state).toBe('unsupported');
    box.close();
    const saved = structuredClone(f.rows.get(`alice:${id}`)!);
    f.rows.set(`alice:${id}`, { ...saved, result: { kind: 'setAllLeverage' } });
    await expect(DelegatedOutbox.open(f.options)).rejects.toThrow('Invalid persisted unsupported action');
    f.rows.set(`alice:${id}`, saved);
    const restored = await DelegatedOutbox.open(f.options);
    const connection = restored.connect(f.connection);
    expect(restored.entries()[0].state).toBe('unsupported');
    await restored.cancel(id);
    await restored.enqueueAction(action, { intentId: id });
    await restored.receive(connection, frame);
    expect(f.writes).toHaveLength(1);
    restored.close();
  });

  it('accepts an identified native protection effect without inventing order IDs, and refuses it for ordinary placements', async () => {
    for (const protection of [false, true]) {
      const f = setup();
      const box = await DelegatedOutbox.open(f.options);
      const c = box.connect(f.connection);
      const action: DelegatedAction = {
        kind: 'place',
        orders: [
          {
            symbol: 'BTCUSDT',
            type: protection ? OrderType.StopLoss : OrderType.Limit,
            side: OrderSide.Sell,
            amount: 0.001,
            price: 49000,
            reduceOnly: true,
          },
        ],
      };
      const id = await box.enqueueAction(action);
      const reply = outcome(id, action);
      await box.receive(c, {
        ...reply,
        d: { ...reply.d, result: { kind: 'place', orderIds: [], protectionApplied: true } },
      });
      expect(box.entries()[0].state).toBe(protection ? 'applied' : 'sending');
      box.close();
    }
  });

  it.each(fullWrites)('pins full-write canonical hash for $method', ({ action, fingerprint }) => {
    expect(isDelegatedFullWriteAction(action as unknown as DelegatedAction)).toBe(true);
    expect(delegatedActionFingerprint('main', golden.key, action as unknown as DelegatedAction, 1)).toBe(fingerprint);
  });
  it('preserves explicit TWAP generation evidence in the typed action and its fingerprint', () => {
    const action = JSON.parse(JSON.stringify(fullWrites.find((entry) => entry.action.op === 'twap:state')!.action));
    const original = JSON.parse(JSON.stringify(action));
    for (const generation of [0, 7, Number.MAX_SAFE_INTEGER]) {
      action.data[0].ownerGeneration = generation;
      expect(isDelegatedFullWriteAction(action)).toBe(true);
    }
    action.data[0].ownerGeneration = 7;
    const owned = JSON.parse(JSON.stringify(action));
    const hash = delegatedActionFingerprint('main', golden.key, action, 1);
    expect(action).toEqual(owned);
    expect(hash).not.toBe(delegatedActionFingerprint('main', golden.key, original, 1));
    action.data[0].ownerGeneration = 6;
    expect(hash).not.toBe(delegatedActionFingerprint('main', golden.key, action, 1));
    for (const generation of [null, '7', true, -1, 0.5, Number.MAX_SAFE_INTEGER + 1]) {
      action.data[0].ownerGeneration = generation;
      expect(isDelegatedAction(action)).toBe(false);
    }
  });
  it.each(['delegatedFullWriteActionsV1', 'delegatedFullWriteLedgerV1'] as const)(
    'requires the expanded %s attestation',
    async (flag) => {
      const f = setup();
      delete f.connection.capabilities[flag];
      const box = await DelegatedOutbox.open(f.options);
      box.connect(f.connection);
      await box.enqueueAction(fullWrites[0].action as unknown as DelegatedAction);
      expect(f.writes).toEqual([]);
      box.close();
      const legacy = setup();
      delete legacy.connection.capabilities[flag];
      const oldBox = await DelegatedOutbox.open(legacy.options);
      oldBox.connect(legacy.connection);
      await oldBox.enqueueAction(actions[1]);
      expect(legacy.writes).toHaveLength(1);
      oldBox.close();
    },
  );
  it.each([
    {
      kind: 'amend',
      order: { id: 'resting', symbol: 'BTCUSDT', type: 'stop_market' },
      update: { price: 49000 },
    },
    { kind: 'setLeverage', symbol: 'BTCUSDT', leverage: -1 },
    { kind: 'setPositionMode', hedged: 'false' },
    { kind: 'setMarginType', isolated: true, accountName: 'other' },
    {
      kind: 'place',
      orders: [{ symbol: 'BTCUSDT', type: 'market', side: 'buy', amount: 1, secret: 'forbidden-field' }],
    },
    { kind: 'close', order: { symbol: 'BTCUSDT', type: 'market', side: 'sell', amount: 1, reduceOnly: false } },
    { kind: 'cancelOrders', orders: [] },
    { kind: 'algo', op: 'callMethod', data: {} },
    { kind: 'algo', op: 'exec', data: { command: 'buy 1', accountName: 'other' } },
    { kind: 'algo', op: 'twap:state', data: [{ id: 'x', options: {}, state: {} }] },
  ])('refuses malformed/full-authority $kind payloads', (value) => {
    expect(isDelegatedAction(value)).toBe(false);
  });

  it('keeps a physically handed-off action unknown when its admission window expires', async () => {
    const f = setup();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection);
    await box.enqueueAction(actions[1]);
    f.options.clock.now = () => 10_000;
    f.timers[0]();
    await box.flush(c);
    expect(box.entries()[0]).toMatchObject({ state: 'unknown', cancelPending: false });
    expect(f.writes.map((text) => JSON.parse(text).t)).toEqual(['order.action']);
    box.close();
  });
  it('does not label a handed-off action canceled before the router proves it unsent', async () => {
    const f = setup();
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection);
    const id = await box.enqueueAction(actions[1]);
    await box.cancel(id);
    expect(box.entries()[0]).toMatchObject({ state: 'unknown', cancelPending: true });
    box.close();
  });

  it.each(golden.cases)('pins shared Rust/browser fingerprint and shapes for $name', (test) => {
    const action = test.action as DelegatedAction;
    expect(delegatedActionFingerprint('main', golden.key, JSON.parse(JSON.stringify(action)), 1)).toBe(
      test.fingerprint,
    );
    for (const frame of [test.request, test.ack, test.outcome, test.cancelQueued]) {
      expect(frame.d).toMatchObject({ version: 1, accountName: 'main', intentId: golden.key, seq: 1 });
    }
  });
  it('hashes the action kind, target IDs, symbols, updates and account separately', () => {
    const base = actions[1] as Extract<DelegatedAction, { kind: 'cancel' }>;
    const changed: DelegatedAction[] = [
      base,
      { ...base, order: { ...base.order, id: 'other' } },
      { ...base, order: { ...base.order, symbol: 'ETHUSDT' } },
      actions[0],
      actions[2],
      actions[3],
    ];
    const hashes = changed.map((action) => delegatedActionFingerprint('main', golden.key, action, 1));
    expect(new Set(hashes).size).toBe(changed.length);
    expect(delegatedActionFingerprint('other', golden.key, base, 1)).not.toBe(hashes[0]);
    expect(delegatedActionFingerprint('main', golden.key, base, 2)).not.toBe(hashes[0]);
  });

  it.each(actions)('enqueues and settles a durable $kind action', async (action) => {
    const f = setup();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection);
    const id = await box.enqueueAction(action);
    const request = JSON.parse(f.writes[0]);
    expect(request).toMatchObject({ t: 'order.action', d: { version: 1, intentId: id, action } });
    expect(request.d.action).not.toHaveProperty('version');
    expect(request.d.fingerprint).toBe(delegatedActionFingerprint('main', id, action, 1));
    expect([...f.rows.values()][0]).toMatchObject({ version: 2, action });
    await box.receive(c, outcome(id, action));
    expect(box.entries()[0]).toMatchObject({
      state: action.kind === 'algo' ? 'unknown' : 'applied',
      ...(action.kind === 'algo' ? {} : { result: outcome(id, action).d.result }),
    });
    box.close();
  });
  it.each([
    [false, false],
    [false, true],
    [true, false],
    [true, true],
  ])('requires action box=%s AND action ledger=%s', async (boxFlag, routerFlag) => {
    const f = setup();
    f.connection.capabilities.delegatedOrderActionsV1 = boxFlag;
    f.connection.capabilities.delegatedOrderActionLedgerV1 = routerFlag;
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection);
    const id = await box.enqueueAction(actions[1]);
    expect(f.writes).toHaveLength(boxFlag && routerFlag ? 1 : 0);
    await box.receive(c, outcome(id, actions[1]));
    expect(box.entries()[0].state).toBe(boxFlag && routerFlag ? 'applied' : 'queued');
    box.close();
  });
  it('keeps an uncertain action distinct from a proven not-sent refusal', async () => {
    for (const state of ['unknown', 'refused'] as const) {
      const f = setup();
      const box = await DelegatedOutbox.open(f.options);
      const c = box.connect(f.connection);
      const id = await box.enqueueAction(actions[3]);
      const reply = outcome(id, actions[3]);
      await box.receive(c, {
        ...reply,
        d: {
          ...reply.d,
          outcome: state,
          result: null,
          error: { message: 'synthetic', outcome: state === 'unknown' ? 'unknown' : 'notSent' },
        },
      });
      expect(box.entries()[0]).toMatchObject({
        state: state === 'unknown' ? 'unknown' : 'notSent',
        error: { outcome: state === 'unknown' ? 'unknown' : 'notSent' },
      });
      await box.flush(box.connect(f.connection));
      expect(f.writes).toHaveLength(1);
      box.close();
    }
  });
  it('does not accept a placement receipt, wrong version or another action result', async () => {
    const f = setup();
    const box = await DelegatedOutbox.open(f.options);
    const c = box.connect(f.connection);
    const id = await box.enqueueAction(actions[1]);
    const reply = outcome(id, actions[1]);
    for (const bad of [
      { t: 'order.outcome', d: { ...reply.d, outcome: 'placed', clientOrderId: id, venueOrderId: 'oops' } },
      { ...reply, d: { ...reply.d, version: 2 } },
      outcome(id, actions[3]),
    ])
      await box.receive(c, bad);
    expect(box.entries()[0].state).toBe('sending');
    box.close();
  });
  it('cancels only the action queue and restores an ambiguous handoff as unknown', async () => {
    const f = setup();
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection);
    const id = await box.enqueueAction(actions[1]);
    box.close();
    const restored = await DelegatedOutbox.open(f.options);
    expect(restored.entries()[0].state).toBe('unknown');
    restored.close();
    const g = setup();
    const queued = await DelegatedOutbox.open(g.options);
    queued.connect(g.connection);
    const cancel = await queued.enqueueAction(actions[2]);
    await queued.cancel(cancel);
    expect(JSON.parse(g.writes[1])).toMatchObject({
      t: 'order.actionCancelQueued',
      d: { version: 1, intentId: cancel },
    });
    queued.close();
    expect(id).toBe(cancel);
  });
  it('keeps owner A captured while owner B replaces it DURING handoff persistence', async () => {
    const f = setup();
    let release!: () => void;
    let started!: () => void;
    const waiting = new Promise<void>((r) => {
      started = r;
    });
    const gate = new Promise<void>((r) => {
      release = r;
    });
    const put = f.options.storage.put;
    f.options.storage.put = async (scope, row) => {
      if (row.state === 'sending') {
        started();
        await gate;
      }
      await put(scope, row);
    };
    const alice = await DelegatedOutbox.open(f.options);
    alice.connect(f.connection);
    const pending = alice.enqueueAction(actions[1]);
    await waiting;
    alice.close();
    const g = setup('bob', f.rows);
    const bob = await DelegatedOutbox.open(g.options);
    bob.connect(g.connection);
    await bob.enqueueAction(actions[3]);
    release();
    await pending;
    expect(f.writes).toEqual([]);
    expect(g.writes).toHaveLength(1);
    expect([...f.rows.values()].filter((r) => r.owner === 'alice')).toHaveLength(1);
    bob.close();
  });
});

describe('DELEGATED existing v1 lifecycle scope', () => {
  it.each(['chaser', 'twap'])('rejects deferred %s parameter amendment before durable enqueue', async (family) => {
    const start = fullWrites.find(({ action }) => 'op' in action && action.op === `${family}:start`)!.action as {
      data: unknown;
    };
    const payload = { kind: 'algo', op: `${family}:amend`, data: { id: 'resident-job', options: start.data } };
    expect(isDelegatedAction(payload)).toBe(false);
    const f = setup();
    const box = await DelegatedOutbox.open(f.options);
    box.connect(f.connection);
    await expect(box.enqueueAction(payload as unknown as DelegatedAction)).rejects.toThrow('Invalid delegated action');
    expect(box.entries()).toHaveLength(0);
    expect(f.writes).toHaveLength(0);
    box.close();
  });
});
