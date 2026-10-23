import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { CommandDeadlineError, CommandProtocolError, CommandTimeoutError, DEFAULT_COMMAND_TIMEOUT_MS, EngineDeadError, EngineDisposedError, EngineHost } from './host.mjs';
import { ReplayClock } from '../../../harness/tape.mjs';

function rig(options = {}, overrides = {}, handlers = {}) {
  let time = 0;
  let next = 0;
  const timers = new Map();
  const calls = [];
  const cleared = [];
  const transport = {
    now: () => time,
    setTimeout(fn, ms) { const id = ++next; timers.set(id, { fn, at: time + ms }); return id; },
    clearTimeout(id) { cleared.push(id); timers.delete(id); },
  };
  const engine = { command(json, now) { calls.push({ ...JSON.parse(json), now }); return '[]'; }, ...overrides };
  const host = new EngineHost(engine, transport, handlers, options);
  const advance = (ms) => {
    time += ms;
    for (;;) {
      const due = [...timers].find(([, timer]) => timer.at <= time);
      if (!due) break;
      timers.delete(due[0]);
      due[1].fn();
    }
  };
  return { host, engine, transport, calls, timers, cleared, advance, elapse: (ms) => { time += ms; } };
}

const done = (host, id, result = '"done"') => host.run(JSON.stringify([{ op: 'done', command: id, result }]));

test('deadlines validate without sending commands; only the effective wait enters native JSON', async () => {
  for (const invalid of [0, -1, NaN, Infinity, 1.5, 2_147_483_648, '100', null]) {
    assert.throws(() => rig({ commandTimeoutMs: invalid }), RangeError);
    const r = rig();
    await assert.rejects(r.host.command({ op: 'getAccount' }, { timeoutMs: invalid }), RangeError);
    assert.equal(r.calls.length, 0);
    assert.equal(r.host.commands.size, 0);
    assert.equal(r.timers.size, 0);
  }
  const r = rig({ commandTimeoutMs: 20 });
  const result = r.host.command({ op: 'getAccount', input: 1 }, { timeoutMs: 7 });
  assert.deepEqual(r.calls, [{ op: 'getAccount', input: 1, command: 1, timeoutMs: 7, now: 0 }]);
  assert.equal([...r.timers.values()][0].at, 7);
  done(r.host, 1);
  assert.equal(await result, 'done');
  assert.equal(r.timers.size, 0);
});

test('default deadline is bounded and long history callers can override it', async () => {
  assert.equal(DEFAULT_COMMAND_TIMEOUT_MS, 120_000);
  const r = rig();
  const normal = r.host.command({ op: 'getAccount' });
  const long = r.host.command({ op: 'fetchHistoricalOrders' }, { timeoutMs: 300_000 });
  const normalRejection = assert.rejects(normal, (error) => error.code === 'COMMAND_TIMEOUT' && error.timeoutMs === 120_000);
  r.advance(120_000);
  await normalRejection;
  assert.equal(r.host.commands.size, 1);
  done(r.host, 2, '[]');
  assert.deepEqual(await long, []);
  assert.equal(r.timers.size, 0);
});

test('synchronous completions, validation errors and no-reply events allocate no deadline', async () => {
  let timerCalls = 0;
  const engine = { command(json) {
    const command = JSON.parse(json);
    if (command.op === 'bad') throw new Error('native validation');
    return command.command === undefined ? '[]' : JSON.stringify([{ op: 'done', command: command.command, result: '42' }]);
  } };
  const host = new EngineHost(engine, { now: () => 0,
    setTimeout() { timerCalls++; throw new Error('unnecessary timer'); }, clearTimeout() {} });
  assert.equal(await host.command({ op: 'getAccount' }), 42);
  await assert.rejects(host.command({ op: 'bad' }), /native validation/);
  for (const op of ['publicFeedUpdate', 'setTickerSymbols', 'setPollScale', 'subscribeOrderBook', 'unsubscribeOrderBook', 'subscribeCandles', 'unsubscribeCandles']) {
    await host.command({ op });
  }
  assert.equal(timerCalls, 0);
  assert.equal(host.commands.size, 0);
  assert.equal(host.dead, false);
});

test('deadline includes admission work and is not reset by intermediate native actions', async () => {
  const r = rig({ commandTimeoutMs: 20 });
  r.engine.command = () => { r.elapse(7); return '[]'; };
  const result = r.host.command({ op: 'fetchHistoricalOrders', get input() { r.elapse(3); return 'history'; } });
  assert.equal([...r.timers.values()][0].at, 20);
  r.elapse(2);
  r.host.run('[{"op":"event","code":"test.diagnostic","level":"info","message":"next history page"}]');
  const rejected = assert.rejects(result, CommandTimeoutError);
  r.advance(8);
  await rejected;
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.host.dead, false);
});

test('minimal transports still bound pending commands using the global timer pair', async (t) => {
  let callback;
  let cleared;
  t.mock.method(globalThis, 'setTimeout', (fn, ms) => { callback = fn; assert.equal(ms, 5); return 42; });
  t.mock.method(globalThis, 'clearTimeout', (handle) => { cleared = handle; });
  const host = new EngineHost({ command: () => '[]' }, { now: () => 0 }, {}, { commandTimeoutMs: 5 });
  const rejected = assert.rejects(host.command({ op: 'getAccount' }), CommandTimeoutError);
  callback();
  await rejected;
  assert.equal(cleared, 42);
  assert.equal(host.commands.size, 0);
});

test('synchronous timer callbacks and disposal during scheduling cannot leak a deadline', async () => {
  for (const dispose of [false, true]) {
    const r = rig();
    let cleared;
    r.transport.setTimeout = (fn) => { if (dispose) r.host.dispose(); else fn(); return 42; };
    r.transport.clearTimeout = (handle) => { cleared = handle; };
    await assert.rejects(r.host.command({ op: 'placeOrders', clientOrderId: 'caller' }), (error) => {
      assert.equal(error.code, dispose ? 'ENGINE_DISPOSED' : 'COMMAND_TIMEOUT');
      assert.equal(error.outcome, 'unknown');
      assert.equal(error.clientOrderId, 'caller');
      return true;
    });
    assert.equal(cleared, 42);
    assert.equal(r.host.commands.size, 0);
  }
});

test('timeouts preserve only caller identity, do not cancel transport I/O or retry writes', async () => {
  const r = rig({ commandTimeoutMs: 10 });
  let requestSignal;
  let finish;
  r.transport.request = ({ signal }) => { requestSignal = signal; return new Promise((resolve) => { finish = resolve; }); };
  r.engine.httpStart = r.engine.httpChunk = r.engine.httpEnd = () => '[]';
  const io = r.host.request({ request: 5, method: 'POST', url: 'https://example.test', headers: [] });
  const orders = [{ clientOrderId: 'caller-1' }, { clientOrderId: 'caller-2' }, {}];
  const batch = r.host.command({ op: 'placeOrders', orders });
  const read = r.host.command({ op: 'getAccount' });
  orders[0].clientOrderId = 'mutated';
  const results = Promise.allSettled([batch, read]);
  r.advance(10);
  const [writeResult, readResult] = await results;
  assert.ok(writeResult.reason instanceof CommandTimeoutError);
  assert.equal(writeResult.reason.command, 'placeOrders');
  assert.equal(writeResult.reason.outcome, 'unknown');
  assert.deepEqual(writeResult.reason.clientOrderIds, ['caller-1', 'caller-2']);
  assert.equal(writeResult.reason.clientOrderId, undefined);
  assert.equal(readResult.reason.outcome, undefined);
  assert.equal(readResult.reason.command, 'getAccount');
  assert.equal(requestSignal.aborted, false);
  assert.equal(r.host.requests.size, 1);
  assert.equal(r.calls.length, 2);
  assert.equal(r.host.dead, false);
  // Expiry ends caller waiting; late native work and frames remain admissible.
  done(r.host, 1, '{bad late JSON');
  finish({ status: 200, body: new Uint8Array() });
  await io;
  assert.equal(r.host.requests.size, 0);
  assert.equal(r.calls.length, 2);
});

test('an interrupted Lighter account-tier change is an unknown write outcome', async () => {
  const r = rig({ commandTimeoutMs: 10 });
  const tier = r.host.command({ op: 'setAccountTier', tier: 'premium' });
  r.advance(10);
  await assert.rejects(tier, (error) => error instanceof CommandTimeoutError
    && error.command === 'setAccountTier' && error.outcome === 'unknown');
});

test('known completion wins once, clears its deadline, and ignores malformed duplicate done', async () => {
  const r = rig({ commandTimeoutMs: 10 });
  const result = r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'one' }] });
  const staleTimer = [...r.timers.values()][0].fn;
  done(r.host, 1, '["accepted"]');
  staleTimer();
  done(r.host, 1, '{malformed duplicate');
  assert.deepEqual(await result, ['accepted']);
  assert.equal(r.cleared.length, 1);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
});

test('malformed success JSON rejects its command and clears the deadline without losing siblings', async () => {
  const r = rig();
  const bad = r.host.command({ op: 'getAccount' });
  const good = r.host.command({ op: 'getAccount' });
  const rejected = assert.rejects(bad, (error) => error instanceof CommandProtocolError && error.cause instanceof SyntaxError && error.outcome === undefined);
  r.host.run(JSON.stringify([{ op: 'done', command: 1, result: '{bad' }, { op: 'done', command: 2, result: '123' }]));
  await rejected;
  assert.equal(await good, 123);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
});

test('malformed action batches and native validation failures settle admission without a deadline leak', async () => {
  for (const output of ['{bad', '{}']) {
    const r = rig({}, { command: () => output });
    await assert.rejects(r.host.command({ op: 'getAccount' }));
    assert.equal(r.host.commands.size, 0);
    assert.equal(r.timers.size, 0);
    assert.equal(r.host.dead, false);
  }
  const r = rig();
  const circular = { op: 'placeOrders' };
  circular.self = circular;
  await assert.rejects(r.host.command(circular), TypeError);
  assert.equal(r.calls.length, 0);
  assert.equal(r.host.commands.size, 0);
});

test('normal disposal settles all callers despite failures in every cleanup adapter', async () => {
  const r = rig();
  let closed = 0;
  let aborted = 0;
  let disposed = 0;
  r.transport.clearTimeout = () => { throw new Error('clear failed'); };
  r.transport.dispose = () => { disposed++; throw new Error('dispose failed'); };
  r.host.requests.set(1, { abort() { aborted++; throw new Error('abort failed'); } });
  r.host.signings.set(2, { controller: { abort() { aborted++; throw new Error('sign abort failed'); } }, timer: 99 });
  r.host.sockets.set(3, { close() { closed++; throw new Error('close failed'); } });
  r.host.timers.set(4, 100);
  const pending = Promise.allSettled([
    r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'one' }] }),
    r.host.command({ op: 'getAccount' }),
  ]);
  r.host.dispose();
  const [write, read] = await pending;
  assert.ok(write.reason instanceof EngineDisposedError);
  assert.equal(write.reason.code, 'ENGINE_DISPOSED');
  assert.equal(write.reason.outcome, 'unknown');
  assert.equal(write.reason.clientOrderId, 'one');
  assert.deepEqual(write.reason.clientOrderIds, ['one']);
  assert.equal(read.reason.outcome, undefined);
  assert.equal(aborted, 2);
  assert.equal(closed, 1);
  assert.equal(disposed, 1);
  for (const key of ['commands', 'requests', 'signings', 'sockets', 'timers']) assert.equal(r.host[key].size, 0);
  r.advance(DEFAULT_COMMAND_TIMEOUT_MS);
  assert.equal(r.host.dead, false);
  await assert.rejects(r.host.command({ op: 'placeOrders' }), (error) => error.code === 'ENGINE_DISPOSED' && error.outcome === undefined);
  assert.throws(() => r.host.snapshot(), EngineDisposedError);
  assert.throws(() => r.host.capabilities(), EngineDisposedError);
  assert.equal(r.calls.length, 2);
  r.host.dispose();
  assert.equal(disposed, 1);
});

test('timer clear failure cannot suppress a known success or known partial rejection', async () => {
  const r = rig();
  r.transport.clearTimeout = () => { throw new Error('clear failed'); };
  const good = r.host.command({ op: 'getAccount' });
  const bad = r.host.command({ op: 'placeOrders' });
  const rejected = assert.rejects(bad, (error) => {
    assert.equal(error.outcome, undefined);
    assert.deepEqual(error.partialOrderIds, ['accepted']);
    assert.equal(error.failedOrderIndex, 1);
    return true;
  });
  r.host.run(JSON.stringify([{ op: 'done', command: 1, result: 'true' }, { op: 'done', command: 2,
    error: JSON.stringify({ message: 'rejected', partialOrderIds: ['accepted'], partialOrderCount: 1, totalOrderCount: 2, failedOrderIndex: 1 }) }]));
  assert.equal(await good, true);
  await rejected;
  r.advance(DEFAULT_COMMAND_TIMEOUT_MS);
  assert.equal(r.host.commands.size, 0);
});

test('unknown actions preserve batch tail completions before surfacing the first error', async () => {
  const r = rig();
  const first = r.host.command({ op: 'getAccount' });
  const second = r.host.command({ op: 'getAccount' });
  assert.throws(() => r.host.run(JSON.stringify([
    { op: 'future-action' }, { op: 'done', command: 1, result: '1' },
    { op: 'other-unknown' }, { op: 'done', command: 2, result: '2' },
  ])), /unknown engine action: future-action/);
  assert.equal(await first, 1);
  assert.equal(await second, 2);
  assert.equal(r.timers.size, 0);
  assert.equal(r.host.dead, false);
});

test('unknown actions from command output stay visible when own completion already won', async () => {
  for (const ownFirst of [false, true]) {
    const logs = [];
    const r = rig({}, {}, { onLog: (level, message) => logs.push({ level, message }) });
    const sibling = r.host.command({ op: 'getAccount' });
    r.engine.command = () => JSON.stringify([
      ...(ownFirst ? [{ op: 'done', command: 2, result: '2' }] : []),
      { op: 'new-action' }, { op: 'done', command: 1, result: '1' },
      ...(!ownFirst ? [{ op: 'done', command: 2, result: '2' }] : []),
    ]);
    assert.equal(await r.host.command({ op: 'getAccount' }), 2);
    assert.equal(await sibling, 1);
    assert.deepEqual(logs, [{ level: 'error', message: 'unknown engine action: new-action' }]);
    assert.equal(r.host.dead, false);
  }
});

test('unknown command actions reject still-pending callers and fall back to console after known completion', async (t) => {
  const r = rig({}, { command: () => '[{"op":"new-action"}]' });
  await assert.rejects(r.host.command({ op: 'getAccount' }), (error) => error.code === 'ENGINE_PROTOCOL_ERROR' && /unknown engine action/.test(error.cause.message) && error.outcome === undefined);
  assert.equal(r.timers.size, 0);
  const report = t.mock.method(console, 'error', () => {});
  r.engine.command = (json) => JSON.stringify([{ op: 'done', command: JSON.parse(json).command, result: 'true' }, { op: 'new-action' }]);
  assert.equal(await r.host.command({ op: 'getAccount' }), true);
  assert.equal(report.mock.calls.length, 1);
  assert.match(report.mock.calls[0].arguments[0].message, /unknown engine action/);
});

test('deadline setup failures after HTTP dispatch preserve unknown write outcomes without cancelling or replaying', async () => {
  for (const failure of ['timer', 'clock']) {
    const r = rig();
    const cause = new Error(`${failure} unavailable`);
    let requests = 0;
    let signal;
    let finish;
    let entries = 0;
    r.transport.request = (request) => {
      requests++;
      signal = request.signal;
      return new Promise((resolve) => { finish = resolve; });
    };
    r.engine.command = () => {
      entries++;
      return '[{"op":"http","request":1,"method":"POST","url":"https://example.test/orders","headers":[]}]';
    };
    r.engine.httpStart = r.engine.httpEnd = () => '[]';
    if (failure === 'timer') r.transport.setTimeout = () => { throw cause; };
    else {
      let reads = 0;
      r.transport.now = () => { if (++reads === 2) throw cause; return 0; };
    }
    await assert.rejects(r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'caller-1' }, { clientOrderId: 'caller-2' }] }), (error) => {
      assert.ok(error instanceof CommandDeadlineError);
      assert.equal(error.code, 'COMMAND_DEADLINE_FAILED');
      assert.equal(error.cause, cause);
      assert.equal(error.command, 'placeOrders');
      assert.equal(error.outcome, 'unknown');
      assert.deepEqual(error.clientOrderIds, ['caller-1', 'caller-2']);
      assert.doesNotMatch(error.message, /timed out|expired/);
      return true;
    });
    assert.equal(requests, 1);
    assert.equal(entries, 1);
    assert.equal(signal.aborted, false);
    assert.equal(r.host.dead, false);
    assert.equal(r.host.commands.size, 0);
    finish({ status: 200, body: new Uint8Array() });
    await new Promise(setImmediate);
    assert.equal(r.host.requests.size, 0);
    const sibling = rig({}, { command: (json) => JSON.stringify([{ op: 'done', command: JSON.parse(json).command, result: 'true' }]) });
    assert.equal(await sibling.host.command({ op: 'getAccount' }), true);
    assert.equal(sibling.host.dead, false);
    assert.equal(entries, 1);
  }
});

test('deadline setup failures on reads and failures before native dispatch do not invent unknown writes', async () => {
  const r = rig();
  const cause = new Error('timer unavailable');
  r.transport.setTimeout = () => { throw cause; };
  await assert.rejects(r.host.command({ op: 'getAccount' }), (error) => error instanceof CommandDeadlineError && error.cause === cause && error.outcome === undefined);
  const initialClockError = new Error('initial clock unavailable');
  r.transport.now = () => { throw initialClockError; };
  await assert.rejects(r.host.command({ op: 'placeOrders', clientOrderId: 'unsubmitted' }), (error) => error === initialClockError && error.outcome === undefined);
  assert.equal(r.calls.length, 1);
  assert.equal(r.host.commands.size, 0);
});

test('unsupported actions after a socket write reject with protocol attribution while processing unrelated completions', async () => {
  const r = rig();
  let sends = 0;
  r.host.sockets.set(7, { send() { sends++; }, close() {} });
  const sibling = r.host.command({ op: 'getAccount' });
  r.engine.command = () => '[{"op":"wsSend","socket":7,"text":"place-order"},{"op":"future-action"},{"op":"done","command":1,"result":"123"}]';
  await assert.rejects(r.host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'caller' }] }), (error) => {
    assert.ok(error instanceof CommandProtocolError);
    assert.equal(error.code, 'ENGINE_PROTOCOL_ERROR');
    assert.match(error.cause.message, /unknown engine action: future-action/);
    assert.equal(error.command, 'placeOrders');
    assert.equal(error.outcome, 'unknown');
    assert.equal(error.clientOrderId, 'caller');
    assert.deepEqual(error.clientOrderIds, ['caller']);
    return true;
  });
  assert.equal(await sibling, 123);
  assert.equal(sends, 1);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
  assert.equal(r.host.dead, false);
  r.host.dispose();
});

test('malformed write success retains identity and unknown outcome rather than claiming rejection', async () => {
  const r = rig();
  const rejected = assert.rejects(r.host.command({ op: 'amendOrder', clientOrderId: 'caller' }), (error) => {
    assert.ok(error instanceof CommandProtocolError);
    assert.ok(error.cause instanceof SyntaxError);
    assert.equal(error.command, 'amendOrder');
    assert.equal(error.clientOrderId, 'caller');
    assert.equal(error.outcome, 'unknown');
    return true;
  });
  done(r.host, 1, '{malformed');
  await rejected;
  assert.equal(r.timers.size, 0);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.host.dead, false);
});

test('an ordinary callback throwing undefined still rejects the pending command', async () => {
  const r = rig({}, { command: () => '[{"op":"event","code":"test.diagnostic","level":"info","message":"notice"}]' }, { onLog() { throw undefined; } });
  const result = await Promise.allSettled([r.host.command({ op: 'getAccount' })]);
  assert.equal(result[0].status, 'rejected');
  assert.equal(result[0].reason, undefined);
  assert.equal(r.timers.size, 0);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.host.dead, false);
});

test('ordinary callback failures are not reclassified as unknown actions or engine death', async () => {
  const r = rig({}, {}, { onLog() { throw new WebAssembly.RuntimeError('consumer log failure'); } });
  const result = r.host.command({ op: 'getAccount' });
  assert.throws(() => r.host.run('[{"op":"event","code":"test.diagnostic","level":"error","message":"notice"},{"op":"done","command":1,"result":"1"}]'), /consumer log failure/);
  assert.equal(r.host.commands.size, 1);
  done(r.host, 1);
  assert.equal(await result, 'done');
  assert.equal(r.host.dead, false);
});

test('native trap retains EngineDeadError and clears pending command deadlines', async () => {
  let deaths = 0;
  const r = rig({}, { start() { throw new WebAssembly.RuntimeError('unreachable'); } }, { onDead() { deaths++; } });
  const result = r.host.command({ op: 'placeOrders', clientOrderId: 'caller' });
  const rejected = assert.rejects(result, (error) => error instanceof EngineDeadError && error.outcome === 'unknown' && error.clientOrderId === 'caller');
  r.host.start();
  await rejected;
  assert.equal(r.timers.size, 0);
  assert.equal(deaths, 1);
  assert.equal(r.host.dead, true);
});

test('command deadlines use the replay virtual clock and leave sibling hosts isolated', async () => {
  const clock = new ReplayClock(1_000, true);
  clock.install();
  try {
    const transport = { now: () => Date.now(), setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer) };
    const first = new EngineHost({ command: () => '[]' }, transport, {}, { commandTimeoutMs: 20 });
    const sibling = new EngineHost({ command: () => '[]' }, transport, {}, { commandTimeoutMs: 50 });
    const rejected = assert.rejects(first.command({ op: 'getAccount' }), CommandTimeoutError);
    const result = sibling.command({ op: 'getAccount' });
    clock.start();
    await clock.advance(20);
    await rejected;
    assert.equal(first.commands.size, 0);
    assert.equal(sibling.commands.size, 1);
    done(sibling, 1, 'true');
    assert.equal(await result, true);
    assert.equal(clock.timers.size, 0);
  } finally {
    clock.restore();
  }
});

test('a history.incomplete event naming the command marks its rows incomplete', async () => {
  const r = rig();
  const flagged = r.host.command({ op: 'fetchIncome' });
  const other = r.host.command({ op: 'fetchIncome' });
  r.host.run(JSON.stringify([
    { op: 'event', level: 'warn', code: 'history.incomplete', message: 'truncated', fields: { command: 1 } },
    { op: 'done', command: 1, result: '[{"id":"a"}]' },
    { op: 'done', command: 2, result: '[{"id":"b"}]' },
  ]));
  const rows = await flagged;
  assert.equal(rows.incomplete, true);
  assert.deepEqual(Object.keys(rows), ['0'], 'still a plain array to existing callers');
  assert.equal(JSON.stringify(rows), '[{"id":"a"}]');
  assert.equal((await other).incomplete, undefined);
});


const coverageEvent = (command) => ({
  op: 'event', level: 'warn', code: 'history.incomplete', message: 'partial history', fields: { command },
});

test('native WEEX incomplete fixture flags retained income only for its pending caller', async () => {
  const event = JSON.parse(readFileSync(new URL('../../../venues/weex/tests/fixtures/incomplete-query-event.json', import.meta.url), 'utf8'));
  const events = [[], []];
  const r = rig();
  const first = r.host.command({ op: 'fetchIncome' }, { onEvent: (event) => {
    assert.equal(r.host.commands.size, 2, 'marker arrives before either completion');
    events[0].push(event);
  } });
  const second = r.host.command({ op: 'fetchIncome' }, { onEvent: (event) => events[1].push(event) });
  r.host.run(JSON.stringify([
    event,
    { op: 'done', command: 1, result: '[{"tranId":"kept"}]' },
    { op: 'done', command: 2, result: '[]' },
  ]));
  const retained = await first;
  assert.equal(retained.incomplete, true);
  assert.deepEqual([...retained], [{ tranId: 'kept' }]);
  const { op, ...notice } = event;
  assert.equal(op, 'event');
  assert.deepEqual(events, [[notice], []]);
  assert.equal((await second).incomplete, undefined);
  assert.equal(r.host.commands.size, 0);
});

test('per-command event observer is installed before synchronous dispatch and sees coverage before done', async () => {
  const events = [];
  const r = rig({}, { command(json) {
    const { command, timeoutMs, ...input } = JSON.parse(json);
    assert.deepEqual(input, { op: 'fetchFills' }, 'observer stays outside native JSON');
    assert.equal(timeoutMs, DEFAULT_COMMAND_TIMEOUT_MS);
    return JSON.stringify([coverageEvent(command), { op: 'done', command, result: '[]' }]);
  } });
  const result = await r.host.command({ op: 'fetchFills' }, { onEvent(event) {
    assert.equal(r.host.commands.size, 1);
    events.push(event);
  } });
  assert.equal(events.length, 1);
  assert.equal(events[0].fields.command, 1);
  assert.equal(result.incomplete, true);
  assert.deepEqual(Object.keys(result), []);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
});

test('per-command events stay with pending requests and malformed, unknown and timed-out IDs have no observer', async () => {
  const firstEvents = [];
  const secondEvents = [];
  const r = rig({ commandTimeoutMs: 5 });
  const first = r.host.command({ op: 'fetchFills' }, { onEvent: (event) => firstEvents.push(event) });
  const second = r.host.command({ op: 'fetchFills' }, { onEvent: (event) => secondEvents.push(event) });
  r.host.run(JSON.stringify([coverageEvent(2), ...['1', null, true, 1.5, -1, 0x1_0000_0000, 999].map(coverageEvent)]));
  assert.equal(firstEvents.length, 0);
  assert.equal(secondEvents.length, 1);
  done(r.host, 2, '[]');
  assert.equal((await second).incomplete, true);
  const expired = assert.rejects(first, CommandTimeoutError);
  r.advance(5);
  await expired;
  r.host.run(JSON.stringify([coverageEvent(1), coverageEvent(2)]));
  assert.equal(firstEvents.length, 0);
  assert.equal(secondEvents.length, 1);
  assert.equal(r.host.commands.size, 0);
});

test('a throwing command event observer cannot replace completion or erase the incomplete flag', async () => {
  const logs = [];
  const r = rig({}, { command(json) {
    const { command } = JSON.parse(json);
    return JSON.stringify([coverageEvent(command), { op: 'done', command, result: '[]' }]);
  } }, { onLog: (level, message) => logs.push({ level, message }) });
  const result = await r.host.command({ op: 'fetchFills' }, { onEvent() { throw new Error('observer broke'); } });
  assert.equal(result.incomplete, true);
  assert.equal(r.host.commands.size, 0);
  assert.equal(r.timers.size, 0);
  assert.ok(logs.some(({ message }) => message === 'Command diagnostic callback failed'));
});


test('invalid command observers reject before native dispatch', async () => {
  for (const onEvent of [null, true, 1, {}]) {
    const r = rig();
    await assert.rejects(r.host.command({ op: 'placeOrders' }, { onEvent }), TypeError);
    assert.equal(r.calls.length, 0);
    assert.equal(r.host.commands.size, 0);
  }
  const r = rig();
  await assert.rejects(r.host.command({ op: 'placeOrders' }, {
    get onEvent() { throw new Error('observer getter failed'); },
  }), /observer getter failed/);
  assert.equal(r.calls.length, 0);
});

test('native WEEX empty-fill fixture flags retained rows incomplete only for its caller', async () => {
  const event = JSON.parse(readFileSync(new URL('../../../venues/weex/tests/fixtures/empty-fill-incomplete-query-event.json', import.meta.url), 'utf8'));
  const observed = [[], []];
  const r = rig();
  const first = r.host.command({ op: 'fetchFills' }, { onEvent: (event) => {
    assert.equal(r.host.commands.size, 2, 'owned marker precedes completion');
    observed[0].push(event);
  } });
  const second = r.host.command({ op: 'fetchFills' }, { onEvent: (event) => observed[1].push(event) });
  r.host.run(JSON.stringify([event, { op: 'done', command: 1, result: '[{"id":"good","price":69000,"qty":0.01}]' }, { op: 'done', command: 2, result: '[]' }]));
  const rows = await first;
  assert.equal(rows.incomplete, true);
  assert.deepEqual([...rows], [{ id: 'good', price: 69000, qty: 0.01 }]);
  assert.equal(JSON.stringify(rows), '[{"id":"good","price":69000,"qty":0.01}]');
  assert.equal((await second).incomplete, undefined);
  assert.deepEqual(observed, [[{ level: event.level, code: event.code, message: event.message, fields: event.fields }], []]);
  assert.equal(r.host.commands.size, 0);
});

test('direct schedule ingress refuses numbers that JSON would turn into a clear', async () => {
  const r = rig();
  for (const time of [NaN, Infinity, -Infinity, 1.5, -1, Number.MAX_SAFE_INTEGER + 1, '123', false]) {
    const pending = r.host.command({ op: 'scheduleCancel', time });
    assert.equal(r.calls.length, 0, 'invalid time must not reach JSON/native');
    await assert.rejects(pending, (error) => error.kind === 'invalidInput' && error.outcome === 'notSent');
  }
  assert.deepEqual(r.calls, []);
  for (const time of [null, undefined, 1_800_000_030_000]) {
    const pending = r.host.command({ op: 'scheduleCancel', time });
    const call = r.calls.at(-1);
    done(r.host, call.command, '[]');
    await pending;
    assert.equal(call.time, time);
  }
});

test('reply commands carry the effective caller wait into the engine envelope', () => {
  const r = rig({ commandTimeoutMs: 20_000 });
  r.host.command({ op: 'getAccount' });
  r.host.command({ op: 'fetchHistoricalOrders' }, { timeoutMs: 300_000 });
  r.host.command({ op: 'setPollScale', scale: 2 });
  assert.deepEqual(r.calls.map((call) => [call.op, call.timeoutMs]), [
    ['getAccount', 20_000],
    ['fetchHistoricalOrders', 300_000],
    ['setPollScale', undefined],
  ]);
  const d = rig();
  d.host.command({ op: 'getAccount' });
  assert.equal(d.calls[0].timeoutMs, DEFAULT_COMMAND_TIMEOUT_MS);
});
