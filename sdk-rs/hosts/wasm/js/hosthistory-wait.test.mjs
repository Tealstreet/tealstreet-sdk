import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost, DEFAULT_COMMAND_TIMEOUT_MS } from './host.mjs';

const tick = async () => { for (let n = 0; n < 40; n++) await Promise.resolve(); };

// Protocol fixture only: real EngineHost/ManagedHttp, no native Plan authority.
function fixture() {
  let now = 0, nextTimer = 0, nextRequest = 0;
  const timers = new Map(), active = new Map(), pending = new Map(), trace = [];
  const completed = [], failed = [];
  const json = (actions = []) => JSON.stringify(actions);
  const prepare = (command, pages, page = 1) => {
    const request = ++nextRequest;
    const token = `p1:${request}:1:1`, permit = `r1:${request}:1:1:1`;
    const packet = { request, method: 'GET', url: `https://history.test/${command}/${page}`, headers: [], timeoutMs: 20_000 };
    active.set(token, { command, pages, page, token, permit, packet, issued: now });
    return { op: 'managedHttp', phase: 'prepare', preparation: token, intent: 'public', packet };
  };
  const current = (request) => [...active.values()].find((item) => item.packet.request === request);
  const engine = {
    command(text) {
      const command = JSON.parse(text);
      return json([prepare(command.command, command.pages)]);
    },
    httpPreparationPending(token) { return active.has(token); },
    httpPrepared(token) {
      const item = active.get(token); assert.ok(item);
      trace.push(['prepared', item.packet.request]);
      return json([{ op: 'managedHttp', phase: 'bind', preparation: token, permit: item.permit, packet: item.packet }]);
    },
    httpClaim(permit) {
      const item = [...active.values()].find((item) => item.permit === permit);
      assert.ok(item); assert.ok(now < item.issued + 20_000);
      trace.push(['claim', item.packet.request]); return JSON.stringify({ status: 'released' });
    },
    prepareDispatch() { return json(); },
    httpPreparationFailed() { assert.fail('the live managed route must prepare'); },
    httpStart(request) { assert.ok(current(request)); return json(); },
    httpChunk(request) { assert.ok(current(request)); return json(); },
    httpEnd(request) {
      const item = current(request); assert.ok(item);
      assert.ok(now < item.issued + 20_000, 'every physical response stays inside its call window');
      active.delete(item.token); trace.push(['end', request]);
      const actions = [{ op: 'managedHttp', phase: 'cancel', preparation: item.token }];
      if (item.page < item.pages) actions.push(prepare(item.command, item.pages, item.page + 1));
      else {
        completed.push(item.command);
        actions.push({ op: 'done', command: item.command, result: JSON.stringify(Array.from({ length: item.pages }, (_, index) => index + 1)) });
      }
      return json(actions);
    },
    httpFailed(request) {
      const item = current(request); assert.ok(item); active.delete(item.token);
      failed.push(item.command);
      return json([{ op: 'managedHttp', phase: 'cancel', preparation: item.token }, { op: 'done', command: item.command, error: 'transport failed' }]);
    },
  };
  const transport = {
    now: () => now,
    setTimeout(fn, delay) { const id = ++nextTimer; timers.set(id, { fn, at: now + delay }); return id; },
    clearTimeout(id) { timers.delete(id); },
    request() { assert.fail('no legacy request fallback'); },
    prepareHttp(packet) {
      trace.push(['prepare', packet.request]);
      return { bind(bound) {
        assert.equal(bound.request, packet.request); assert.equal(bound.signal.aborted, false);
        trace.push(['bind', packet.request]);
        return { ready: () => !bound.signal.aborted, dispatch() {
          assert.deepEqual(trace.at(-1), ['claim', packet.request]);
          trace.push(['dispatch', packet.request]);
          return new Promise((resolve, reject) => pending.set(packet.request, { resolve, reject, signal: bound.signal }));
        } };
      } };
    },
  };
  const host = new EngineHost(engine, transport);
  return {
    host, trace, pending, completed, failed, now: () => now,
    advance(by) {
      now += by;
      for (;;) {
        const due = [...timers].find(([, value]) => value.at <= now);
        if (!due) return;
        timers.delete(due[0]); due[1].fn();
      }
    },
    finish(request, failure = false) {
      const item = pending.get(request); assert.ok(item); pending.delete(request);
      if (failure) item.reject(new Error('deferred transport loss'));
      else item.resolve({ status: 200, headers: '', body: new TextEncoder().encode('{}') });
    },
  };
}

// Models the inventoried unbounded await and serial chain, not NX implementation.
function ingestion(host, now) {
  let tail = Promise.resolve();
  const windows = [], starts = [];
  return {
    windows, starts,
    enqueue(name, pages) {
      const state = { name, outcome: 'waiting', completions: 0 };
      windows.push(state);
      const task = tail.then(async () => {
        if (starts.length) assert.ok(now() - starts.at(-1) >= 2_000, 'the exchange pacing floor already elapsed');
        starts.push(now()); state.outcome = 'running';
        try {
          state.rows = await host.command({ op: 'fetchIncome', pages });
          state.outcome = 'complete'; state.completions++;
        } catch (error) {
          state.outcome = 'failed'; state.error = error;
        }
        return state;
      });
      tail = task.then(() => {});
      return task;
    },
  };
}

for (const lateFailure of [false, true]) {
  test(`managed paged caller timeout releases ingestion wait before late ${lateFailure ? 'failure' : 'success'}`, async () => {
    const r = fixture(); const queue = ingestion(r.host, r.now);
    const original = queue.enqueue('old', 7);
    const next = queue.enqueue('next', 1);
    await tick();
    for (let page = 1; page <= 6; page++) {
      assert.equal(r.pending.size, 1);
      r.advance(19_000); r.finish([...r.pending.keys()][0]); await tick();
      assert.equal(queue.windows[0].outcome, 'running');
    }
    assert.equal(r.now(), 114_000);
    const oldRequest = [...r.pending.keys()][0]; const oldSignal = r.pending.get(oldRequest).signal;
    r.advance(6_000); await tick();
    const abandoned = await original;
    assert.equal(abandoned.outcome, 'failed'); assert.equal(abandoned.error.code, 'COMMAND_TIMEOUT');
    assert.equal(abandoned.error.timeoutMs, DEFAULT_COMMAND_TIMEOUT_MS);
    assert.equal(abandoned.error.outcome, undefined);
    assert.equal(abandoned.completions, 0); assert.equal(abandoned.rows, undefined);
    assert.equal(oldSignal.aborted, false, 'caller expiry cannot cancel the managed resource');
    assert.deepEqual(queue.starts, [0, 120_000]);
    assert.equal(queue.windows[1].outcome, 'running');
    assert.equal(r.host.managedHttp.resources.size, 2, 'old work can overlap a new caller wait');
    const newRequest = [...r.pending.keys()].find((id) => id !== oldRequest);
    r.advance(1); r.finish(newRequest); await tick();
    assert.equal((await next).outcome, 'complete'); assert.deepEqual(queue.windows[1].rows, [1]);
    assert.equal(r.host.managedHttp.resources.size, 1);
    r.advance(1_000); r.finish(oldRequest, lateFailure); await tick();
    assert.equal(abandoned.outcome, 'failed'); assert.equal(abandoned.completions, 0);
    assert.equal(abandoned.rows, undefined); assert.equal(queue.windows[1].completions, 1);
    assert.equal(r.host.commands.size, 0); assert.equal(r.host.managedHttp.resources.size, 0);
    assert.deepEqual(r.completed, lateFailure ? [2] : [2, 1]);
    assert.deepEqual(r.failed, lateFailure ? [1] : []);
    for (const phase of ['prepare', 'prepared', 'bind', 'claim', 'dispatch']) assert.equal(r.trace.filter(([name]) => name === phase).length, 8);
    r.host.dispose();
  });
}

test('managed history completed before the host bound completes ingestion once', async () => {
  const r = fixture(); const queue = ingestion(r.host, r.now);
  const completed = queue.enqueue('live', 2); await tick();
  for (let page = 0; page < 2; page++) { r.advance(19_000); r.finish([...r.pending.keys()][0]); await tick(); }
  const result = await completed;
  assert.equal(result.outcome, 'complete'); assert.equal(result.completions, 1); assert.deepEqual(result.rows, [1, 2]);
  r.advance(120_000); await tick();
  assert.equal(result.outcome, 'complete'); assert.equal(result.completions, 1);
  assert.equal(r.host.commands.size, 0); assert.equal(r.host.managedHttp.resources.size, 0);
  r.host.dispose();
});
