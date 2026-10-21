import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const diagnostic = (command, message = 'Venue refused one member') => ({ op: 'commandError', command, message });
const done = (command, extra = {}) => ({ op: 'done', command, ...extra });
function rig(actions = () => [], handlers = {}) {
  const logs = [];
  const timers = new Map();
  const writes = [];
  let next = 0;
  const engine = { command(json) { return JSON.stringify(actions(JSON.parse(json))); } };
  const host = new EngineHost(engine, {
    now: () => 0,
    setTimeout(fn) { timers.set(++next, fn); return next; },
    clearTimeout(id) { timers.delete(id); },
  }, { onLog(level, message) { logs.push({ level, message }); }, ...handlers }, { commandTimeoutMs: 20 });
  host.sockets.set(7, { send(text) { writes.push(text); }, close() {} });
  return { host, engine, logs, timers, writes,
    run: (rows) => host.run(JSON.stringify(rows)),
    expire: () => { for (const callback of [...timers.values()]) callback(); },
  };
}

test('same-frame associated error plus native rejection keeps one promise failure and a warning diagnostic', async () => {
  const details = { message: 'Second order rejected', partialOrderIds: ['accepted-first'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1 };
  const r = rig(({ command }) => [diagnostic(command, details.message), done(command, { error: JSON.stringify(details) })]);
  await assert.rejects(r.host.command({ op: 'placeOrders' }), (error) => {
    assert.equal(error.message, details.message);
    assert.deepEqual(error.partialOrderIds, details.partialOrderIds);
    assert.equal(error.partialOrderCount, 1);
    assert.equal(error.failedOrderIndex, 1);
    assert.equal(error.outcome, undefined);
    return true;
  });
  assert.deepEqual(r.logs, [{ level: 'warning', message: details.message }]);
  assert.equal(r.host.commands.size, 0);
});

test('deferred successful partial result promotes only its first attributed error after settlement', async () => {
  const levels = [];
  const r = rig(() => [], { onLog(level, message) {
    if (level === 'error') assert.equal(r.host.commands.size, 0, 'known outcome must settle before the error callback');
    levels.push({ level, message });
  } });
  const pending = r.host.command({ op: 'placeOrders' });
  r.run([diagnostic(1, 'First member rejected')]);
  assert.equal(r.host.commands.get(1).firstCommandError, 'First member rejected');
  r.run([diagnostic(1, 'Another member rejected'), done(1, { result: '["accepted-other-member"]' })]);
  assert.deepEqual(await pending, ['accepted-other-member']);
  assert.deepEqual(levels, [
    { level: 'warning', message: 'First member rejected' },
    { level: 'warning', message: 'Another member rejected' },
    { level: 'error', message: 'First member rejected' },
  ]);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
});

test('pure validation rejection creates no fabricated diagnostic or extra error log', async () => {
  const r = rig(({ command }) => [done(command, { error: 'Invalid quantity' })]);
  await assert.rejects(r.host.command({ op: 'placeOrders' }), /Invalid quantity/);
  assert.deepEqual(r.logs, []);
});

test('warning-only success and background errors retain their ordinary levels', async () => {
  const r = rig(({ command }) => [
    { op: 'event', code: 'test.diagnostic', level: 'warn', message: 'Read repair scheduled' },
    { op: 'event', code: 'test.diagnostic', level: 'error', message: 'Unrelated background failure' },
    done(command, { result: '[]' }),
  ]);
  assert.deepEqual(await r.host.command({ op: 'getAccount' }), []);
  assert.deepEqual(r.logs, [
    { level: 'warning', message: 'Read repair scheduled' },
    { level: 'error', message: 'Unrelated background failure' },
  ]);
});

test('identical text on independent commands is never deduplicated by time or message', async () => {
  const r = rig();
  const first = r.host.command({ op: 'placeOrders' });
  const second = r.host.command({ op: 'placeOrders' });
  r.run([diagnostic(1, 'Same error'), diagnostic(2, 'Same error'), done(2, { result: '[]' }), done(1, { result: '[]' })]);
  assert.deepEqual(await Promise.all([first, second]), [[], []]);
  assert.deepEqual(r.logs, [
    { level: 'warning', message: 'Same error' }, { level: 'warning', message: 'Same error' },
    { level: 'error', message: 'Same error' }, { level: 'error', message: 'Same error' },
  ]);
});

test('a timeout remains unknown and late diagnostics/done cannot emit a second command error', async () => {
  const r = rig(({ command }) => [diagnostic(command, 'Before deadline')]);
  const pending = r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'caller' }] });
  r.expire();
  await assert.rejects(pending, (error) => error.code === 'COMMAND_TIMEOUT' && error.outcome === 'unknown' && error.clientOrderId === 'caller');
  r.run([diagnostic(1, 'After deadline'), done(1, { result: '["accepted-late"]' }), done(1)]);
  assert.deepEqual(r.logs, [
    { level: 'warning', message: 'Before deadline' }, { level: 'warning', message: 'After deadline' },
  ]);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
});

test('duplicate done and diagnostics after known success do not retain a reporting ledger', async () => {
  const r = rig(({ command }) => [diagnostic(command, 'Partial failure'), done(command, { result: '[]' })]);
  assert.deepEqual(await r.host.command({ op: 'placeOrders' }), []);
  r.run([done(1), diagnostic(1, 'Later diagnostic'), done(1)]);
  assert.deepEqual(r.logs, [
    { level: 'warning', message: 'Partial failure' }, { level: 'error', message: 'Partial failure' },
    { level: 'warning', message: 'Later diagnostic' },
  ]);
  assert.equal(r.host.commands.size, 0);
});

test('throwing diagnostic and fallback reporters preserve one outgoing write and unrelated known completion', async (t) => {
  t.mock.method(console, 'error', () => { throw new Error('console unavailable'); });
  t.mock.method(console, 'warn', () => { throw new Error('console unavailable'); });
  for (const acknowledged of [false, true]) {
    const r = rig(() => [], { onLog() { throw new Error('diagnostic consumer unavailable'); } });
    const sibling = r.host.command({ op: 'getAccount' });
    r.engine.command = (json) => {
      const { command } = JSON.parse(json);
      return JSON.stringify([diagnostic(command), { op: 'wsSend', socket: 7, text: 'single-write' },
        done(1, { result: '42' }), ...(acknowledged ? [done(command, { result: '["accepted"]' })] : [])]);
    };
    const pending = r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'caller' }] });
    assert.equal(await sibling, 42);
    assert.deepEqual(r.writes, ['single-write']);
    assert.equal(r.host.dead, false);
    if (acknowledged) assert.deepEqual(await pending, ['accepted']);
    else {
      assert.equal(r.host.commands.size, 1);
      assert.equal(r.timers.size, 1);
      r.expire();
      await assert.rejects(pending, (error) => error.code === 'COMMAND_TIMEOUT' && error.outcome === 'unknown' && error.clientOrderId === 'caller');
    }
    assert.equal(r.host.commands.size, 0);
    assert.equal(r.timers.size, 0);
    assert.deepEqual(r.writes, ['single-write']);
    r.host.dispose();
  }
});

test('throwing completion progress cannot skip final diagnostic delivery or batch-tail results', async () => {
  const logs = [];
  const r = rig(() => [], {
    onCommandProgress() { throw new Error('progress unavailable'); },
    onLog(level, message) { logs.push({ level, message }); },
  });
  const first = r.host.command({ op: 'placeOrders' });
  const second = r.host.command({ op: 'getAccount' });
  r.run([diagnostic(1, 'Partial'), done(1, { result: '[]' }), done(2, { result: '42' })]);
  assert.deepEqual(await first, []);
  assert.equal(await second, 42);
  assert.deepEqual(logs, [
    { level: 'warning', message: 'Partial' }, { level: 'error', message: 'Partial' },
    { level: 'error', message: 'Command progress callback failed' },
  ]);
});


test('a throwing interim diagnostic listener cannot produce a second error at the SDK rejection boundary', async (t) => {
  t.mock.method(console, 'warn', () => {});
  const errorMessages = [];
  const r = rig(({ command }) => [diagnostic(command, 'Original native failure'), done(command, { error: 'Original native failure' })], {
    onLog(level, message) {
      if (level === 'warning') throw new Error('warning listener unavailable');
      if (level === 'error') errorMessages.push(message);
    },
  });
  const error = await r.host.command({ op: 'placeOrders' }).catch((failure) => failure);
  // Mirrors the SDK promise catch. The host must not emit an earlier error
  // merely because the warning consumer failed.
  r.host.handlers.onLog('error', error.message);
  assert.deepEqual(errorMessages, ['Original native failure']);
  assert.equal(error.outcome, undefined);
  assert.equal(r.host.commands.size, 0);
});

test('a partial-success error listener throwing after display cannot trigger another error-level invocation', async () => {
  const errorMessages = [];
  const r = rig(() => [], { onLog(level, message) {
    if (level === 'error') { errorMessages.push(message); throw new Error('toast listener failed after display'); }
  } });
  const partial = r.host.command({ op: 'placeOrders' });
  const sibling = r.host.command({ op: 'getAccount' });
  r.run([diagnostic(1, 'One rejected member'), done(1, { result: '["accepted-member"]' }), done(2, { result: '42' })]);
  assert.deepEqual(await partial, ['accepted-member']);
  assert.equal(await sibling, 42);
  assert.deepEqual(errorMessages, ['One rejected member']);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
});

for (const builderAuthorization of [
  { code: 'GRVT_BUILDER_UNAUTHORIZED', reason: 'unauthorized', requiredRaw: 300 },
  { code: 'GRVT_BUILDER_UNAUTHORIZED', reason: 'cap-too-low', requiredRaw: 300, authorizedRaw: 100 },
]) {
  test(`native builder ${builderAuthorization.reason} survives host command rejection`, async () => {
    const detail = { message: 'Builder approval required', kind: 'invalidInput', outcome: 'notSent', builderAuthorization };
    const r = rig(({ command }) => [done(command, { error: detail.message, detail })]);
    await assert.rejects(r.host.command({ op: 'placeOrders' }), (error) => {
      for (const [key, value] of Object.entries(builderAuthorization)) assert.equal(error[key], value);
      assert.equal(error.kind, 'invalidInput');
      assert.equal(error.outcome, 'notSent');
      return true;
    });
    assert.deepEqual(r.logs, []);
  });
}


test('scheduleCancel interruptions stay unknown and cannot be mistaken for a cleared switch', async () => {
  for (const interrupt of ['timeout', 'dispose']) {
    const r = rig();
    const pending = r.host.command({ op: 'scheduleCancel', time: 1_800_000_030_000 });
    if (interrupt === 'timeout') r.expire();
    else r.host.dispose();
    await assert.rejects(pending, (error) => error.outcome === 'unknown');
    r.run([done(1, { result: '[]' })]);
    assert.equal(r.host.commands.size, 0);
  }
});
