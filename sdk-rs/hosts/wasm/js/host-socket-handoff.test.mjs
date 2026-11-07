import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

for (const inline of [false, true]) {
  test(`subscription handoff precedes an engine timer continuation (inline=${inline})`, () => {
    const effects = [];
    let wake;
    const engine = {
      timer(id, now) {
        assert.equal(id, 19);
        assert.equal(now, 100);
        effects.push('engine continuation');
        return JSON.stringify([{ op: 'http', request: 23, method: 'GET',
          url: 'https://fixture.invalid/account', headers: [] }]);
      },
    };
    const host = new EngineHost(engine, {
      now: () => 100,
      setTimeout(callback, delay) {
        assert.equal(delay, 0);
        effects.push('arm continuation');
        wake = callback;
        if (inline) callback();
        return 1;
      },
      clearTimeout() {},
      request() {
        effects.push('HTTP handoff');
        return new Promise(() => {});
      },
    });
    host.sockets.set(7, {
      send(text) {
        assert.equal(text, 'subscribe');
        effects.push('subscription handoff');
      },
      close() {},
    });
    host.run(JSON.stringify([
      { op: 'wsSend', socket: 7, text: 'subscribe' },
      { op: 'setTimer', timer: 19, at: 100 },
    ]));
    if (!inline) {
      assert.deepEqual(effects, ['subscription handoff', 'arm continuation']);
      wake();
    }
    assert.deepEqual(effects, ['subscription handoff', 'arm continuation',
      'engine continuation', 'HTTP handoff']);
    assert.equal(host.timers.size, 0);
    wake();
    assert.equal(effects.filter((value) => value === 'engine continuation').length, 1);
    assert.equal(host.timers.size, 0);
    host.dispose();
  });
}

test('cleared and superseded timer callbacks cannot consume a new arm', () => {
  const callbacks = [];
  const cleared = [];
  let calls = 0;
  const host = new EngineHost({ timer() { calls++; return '[]'; } }, {
    now: () => 100,
    setTimeout(callback) { callbacks.push(callback); return callbacks.length; },
    clearTimeout(handle) { cleared.push(handle); },
  });
  const arm = () => host.execute({ op: 'setTimer', timer: 19, at: 100 });
  arm();
  arm();
  assert.deepEqual(cleared, [1]);
  callbacks[0]();
  assert.equal(calls, 0);
  assert.equal(host.timers.get(19), 2);
  callbacks[1]();
  callbacks[1]();
  assert.equal(calls, 1);
  assert.equal(host.timers.size, 0);
  arm();
  host.execute({ op: 'clearTimer', timer: 19 });
  callbacks[2]();
  assert.equal(calls, 1);
  assert.equal(host.timers.size, 0);
  assert.deepEqual(cleared, [1, 3]);
  host.dispose();
});

test('an early inline wakeup can rearm the same timer without losing its later callback', () => {
  let now = 100;
  let wake;
  let arms = 0;
  let ingress = 0;
  let continuations = 0;
  const host = new EngineHost({
    timer(id, at) {
      assert.equal(id, 19);
      ingress++;
      if (at < 100) return JSON.stringify([{ op: 'setTimer', timer: 19, at: 100 }]);
      continuations++;
      return '[]';
    },
  }, {
    now: () => now,
    setTimeout(callback) {
      const handle = ++arms;
      if (handle === 1) {
        now = 99;
        callback();
      } else wake = callback;
      return handle;
    },
    clearTimeout() {},
  });
  host.run(JSON.stringify([{ op: 'setTimer', timer: 19, at: 100 }]));
  assert.equal(continuations, 0);
  assert.equal(ingress, 1);
  assert.equal(arms, 2);
  assert.equal(host.timers.get(19), 2);
  now = 100;
  wake();
  assert.equal(continuations, 1);
  assert.equal(ingress, 2);
  assert.equal(host.timers.size, 0);
  wake();
  assert.equal(continuations, 1);
  assert.equal(ingress, 2);
  host.dispose();
});
