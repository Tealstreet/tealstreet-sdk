import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const receipt = (identity = 'native-1', kind = 'clientOrderId', symbol = 'BTCUSDT') => ({ symbol, kind, identity });
const progress = (command, receipts) => ({ op: 'commandProgress', command, receipts });
const done = (command, extra = {}) => ({ op: 'done', command, ...extra });

function rig(actions = () => [], handlers = {}, options = {}) {
  const events = [];
  const timers = new Map();
  const io = [];
  const logs = [];
  let next = 0;
  const engine = {
    setEntropy(source) { this.entropy = source; },
    command(json) { return JSON.stringify(actions(JSON.parse(json))); },
    start() { throw new WebAssembly.RuntimeError('unreachable'); },
  };
  const transport = {
    now: () => 0,
    setTimeout(fn) { timers.set(++next, fn); return next; },
    clearTimeout(id) { timers.delete(id); },
    request() { io.push('http'); return new Promise(() => {}); },
  };
  const host = new EngineHost(engine, transport, {
    onCommandProgress(event) { events.push(event); },
    onLog(level, message) { logs.push({ level, message }); },
    ...handlers,
  }, { commandTimeoutMs: 20, ...options });
  host.sockets.set(7, { send() { io.push('ws'); }, close() {} });
  return { host, engine, transport, events, timers, io, logs,
    run: (actions) => host.run(JSON.stringify(actions)),
    expire: () => { for (const fn of [...timers.values()]) fn(); },
  };
}

test('native receipts are accumulated and delivered before HTTP or WebSocket I/O', async () => {
  for (const type of ['http', 'ws']) {
    const writes = { http: { op: 'http', request: 1, method: 'POST', url: 'https://example.test/orders', headers: [] },
      ws: { op: 'wsSend', socket: 7, text: 'order' } };
    const observed = [];
    const r = rig(({ command }) => [progress(command, [receipt()]), writes[type]], {
      onCommandProgress(event) {
        observed.push(event);
        if (!event.complete) {
          assert.deepEqual(r.io, []);
          assert.deepEqual(r.host.commands.get(event.commandId).identity.orderIdentities, [receipt()]);
        }
      },
    });
    const pending = r.host.command({ op: 'placeOrders', orders: [] });
    await new Promise(setImmediate);
    assert.deepEqual(r.io, [type]);
    assert.equal(observed[0].late, false);
    assert.equal(observed[0].complete, false);
    assert.ok(Object.isFrozen(observed[0]));
    assert.ok(Object.isFrozen(observed[0].receipts));
    assert.ok(Object.isFrozen(observed[0].receipts[0]));
    r.run([done(1, { result: '["venue-order"]' })]);
    assert.deepEqual(await pending, ['venue-order']);
    assert.equal(observed[1].complete, true);
    assert.equal(observed[1].late, false);
    assert.equal(observed[1].receiptKey, observed[0].receiptKey);
    r.host.dispose();
  }
});

test('caller IDs stay separate from tuple-deduped actual client IDs, numeric indices and order codes', async () => {
  const rows = [receipt('actual'), receipt('actual'), receipt('actual', 'clientOrderId', 'ETHUSDT'),
    receipt('900719925474099312345', 'clientOrderIndex'), receipt('000123', 'orderCode')];
  const r = rig(({ command }) => [progress(command, rows), progress(command, [receipt('actual'), receipt('next')])]);
  const pending = r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'requested' }] });
  r.expire();
  const { reason: error } = (await Promise.allSettled([pending]))[0];
  assert.equal(error.code, 'COMMAND_TIMEOUT');
  assert.equal(error.clientOrderId, 'requested');
  assert.deepEqual(error.clientOrderIds, ['requested']);
  assert.deepEqual(error.actualClientOrderIds, ['actual', 'next']);
  assert.deepEqual(error.orderIdentities, [rows[0], rows[2], rows[3], rows[4], receipt('next')]);
  assert.equal(error.commandId, 1);
  assert.equal(error.receiptKey, r.events[0].receiptKey);
  assert.equal(r.events[0].receipts.length, 4);
  assert.equal(r.events[1].receipts.length, 2);
  assert.ok(Object.isFrozen(error.orderIdentities));
  assert.ok(Object.isFrozen(error.actualClientOrderIds));
  assert.throws(() => { error.orderIdentities[0].identity = 'changed'; }, TypeError);
  r.run([progress(1, [receipt('late')])]);
  assert.deepEqual(error.actualClientOrderIds, ['actual', 'next']);
  assert.equal(error.orderIdentities.length, 5);
  r.host.dispose();
});

test('dead, disposed and timed-out commands take immutable receipt snapshots', async () => {
  for (const terminal of ['dead', 'dispose', 'timeout']) {
    const r = rig(({ command }) => [progress(command, [receipt('actual')])]);
    const pending = r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'requested' }] });
    if (terminal === 'dead') r.host.start();
    else if (terminal === 'dispose') r.host.dispose();
    else r.expire();
    const { reason: error } = (await Promise.allSettled([pending]))[0];
    assert.equal(error.code, { dead: 'ENGINE_DEAD', dispose: 'ENGINE_DISPOSED', timeout: 'COMMAND_TIMEOUT' }[terminal]);
    assert.equal(error.outcome, 'unknown');
    assert.equal(error.receiptKey, r.events[0].receiptKey);
    assert.equal(error.commandId, 1);
    assert.deepEqual(error.orderIdentities, [receipt('actual')]);
    assert.deepEqual(error.actualClientOrderIds, ['actual']);
    assert.ok(Object.isFrozen(error.orderIdentities));
    assert.ok(Object.isFrozen(error.orderIdentities[0]));
    assert.ok(Object.isFrozen(error.actualClientOrderIds));
    assert.equal(r.host.commands.size, 0);
    r.host.dispose();
  }
});

test('an interruption before native ID generation still carries a stable receipt key', async () => {
  const r = rig();
  const pending = r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'requested' }] });
  r.expire();
  const { reason: error } = (await Promise.allSettled([pending]))[0];
  assert.equal(typeof error.receiptKey, 'string');
  assert.equal(error.commandId, 1);
  assert.deepEqual(error.orderIdentities, []);
  assert.deepEqual(error.actualClientOrderIds, []);
  assert.equal(error.clientOrderId, 'requested');
  r.run([progress(1, [receipt('generated-later')]), done(1, { result: '["accepted"]' }), done(1)]);
  assert.deepEqual(r.events.map(({ receiptKey, complete, late, receipts }) => ({ receiptKey, complete, late, receipts })), [
    { receiptKey: error.receiptKey, complete: false, late: true, receipts: [receipt('generated-later')] },
    { receiptKey: error.receiptKey, complete: true, late: true, receipts: [] },
    { receiptKey: error.receiptKey, complete: true, late: true, receipts: [] },
  ]);
  assert.deepEqual(error.orderIdentities, []);
  assert.deepEqual(error.actualClientOrderIds, []);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
  r.host.dispose();
});

test('known native partial rejection retains its metadata and closes receipts without an unknown outcome', async () => {
  const details = { message: 'second rejected', partialOrderIds: ['first-accepted'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1 };
  const r = rig(({ command }) => [progress(command, [receipt('actual')]), done(command, { error: JSON.stringify(details) })]);
  await assert.rejects(r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'requested' }] }), (error) => {
    assert.equal(error.message, details.message);
    assert.deepEqual(error.partialOrderIds, details.partialOrderIds);
    assert.equal(error.failedOrderIndex, 1);
    assert.equal(error.outcome, undefined);
    assert.deepEqual(error.actualClientOrderIds, ['actual']);
    assert.equal(error.receiptKey, r.events[0].receiptKey);
    return true;
  });
  assert.equal(r.events[1].complete, true);
  assert.equal(r.events[1].late, false);
});

test('progress consumer failure cannot skip an outgoing write or unrelated and own known completions', async () => {
  const logs = [];
  let callbacks = 0;
  const r = rig(() => [], {
    onCommandProgress() { callbacks++; throw new Error('consumer unavailable'); },
    onLog(level, message) { logs.push({ level, message }); },
  });
  const sibling = r.host.command({ op: 'getAccount' });
  r.engine.command = (json) => {
    const { command } = JSON.parse(json);
    return JSON.stringify([progress(command, [receipt()]), { op: 'wsSend', socket: 7, text: 'order' },
      done(1, { result: '1' }), done(command, { result: '2' })]);
  };
  assert.equal(await r.host.command({ op: 'placeOrders', orders: [] }), 2);
  assert.equal(await sibling, 1);
  assert.equal(callbacks, 3);
  assert.deepEqual(r.io, ['ws']);
  assert.deepEqual(logs, [{ level: 'error', message: 'Command progress callback failed' }]);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
  assert.equal(r.host.dead, false);
  r.host.dispose();
});

test('a failed progress consumer does not falsely complete a still-running write', async (t) => {
  const report = t.mock.method(console, 'error', () => {});
  const r = rig(({ command }) => [progress(command, [receipt()]), { op: 'wsSend', socket: 7, text: 'order' }], {
    onCommandProgress() { throw new WebAssembly.RuntimeError('a consumer failure is not an engine trap'); },
  });
  delete r.host.handlers.onLog;
  const pending = r.host.command({ op: 'placeOrders', orders: [] });
  assert.equal(r.host.commands.size, 1);
  assert.deepEqual(r.io, ['ws']);
  assert.equal(report.mock.calls.length, 1);
  assert.equal(r.host.dead, false);
  r.expire();
  await assert.rejects(pending, (error) => error.code === 'COMMAND_TIMEOUT' && error.outcome === 'unknown' && error.actualClientOrderIds[0] === 'native-1');
  assert.deepEqual(r.io, ['ws']);
  r.host.dispose();
});

test('throwing progress and both diagnostic reporters cannot change a dispatched write outcome', async (t) => {
  const report = t.mock.method(console, 'error', () => { throw new Error('console unavailable'); });
  for (const acknowledged of [false, true]) {
    const r = rig(({ command }) => [progress(command, [receipt('actual')]), { op: 'wsSend', socket: 7, text: 'order' },
      ...(acknowledged ? [done(command, { result: '"accepted"' })] : [])], {
      onCommandProgress() { throw new Error('progress consumer unavailable'); },
      onLog() { throw new Error('log consumer unavailable'); },
    });
    const pending = r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'requested' }] });
    assert.deepEqual(r.io, ['ws']);
    assert.equal(r.host.dead, false);
    if (acknowledged) assert.equal(await pending, 'accepted');
    else {
      assert.equal(r.host.commands.size, 1, 'diagnostic failures must leave the actual write pending');
      assert.equal(r.timers.size, 1);
      r.expire();
      await assert.rejects(pending, (error) => {
        assert.equal(error.code, 'COMMAND_TIMEOUT');
        assert.equal(error.outcome, 'unknown');
        assert.equal(error.clientOrderId, 'requested');
        assert.deepEqual(error.actualClientOrderIds, ['actual']);
        assert.deepEqual(error.orderIdentities, [receipt('actual')]);
        return true;
      });
    }
    assert.deepEqual(r.io, ['ws']);
    assert.equal(r.host.commands.size, 0);
    assert.equal(r.timers.size, 0);
    r.host.dispose();
  }
  assert.equal(report.mock.calls.length, 2);
});

test('host sessions isolate equal command IDs with explicit entropy without Web Crypto', async (t) => {
  t.mock.method(Math, 'random', () => assert.fail('no weak entropy fallback'));
  const cryptoDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'crypto');
  Object.defineProperty(globalThis, 'crypto', { configurable: true, value: undefined });
  try {
    assert.throws(() => rig(), (error) => error.kind === 'invalidInput');
    const hosts = [1, 2].map((seed) => rig(undefined, undefined, {
      entropy: (length) => Uint8Array.from({ length }, () => seed++ & 255),
    }));
    const errors = [];
    for (const r of hosts) {
      const pending = r.host.command({ op: 'placeOrders', orders: [] });
      r.expire();
      errors.push((await Promise.allSettled([pending]))[0].reason);
    }
    assert.equal(errors[0].commandId, errors[1].commandId);
    assert.notEqual(errors[0].receiptKey, errors[1].receiptKey);
    hosts[0].run([progress(1, [receipt('first-host')])]);
    assert.equal(hosts[0].events[0].receiptKey, errors[0].receiptKey);
    assert.deepEqual(hosts[1].events, []);
    for (const r of hosts) r.host.dispose();
  } finally {
    if (cryptoDescriptor) Object.defineProperty(globalThis, 'crypto', cryptoDescriptor);
    else delete globalThis.crypto;
  }
});
