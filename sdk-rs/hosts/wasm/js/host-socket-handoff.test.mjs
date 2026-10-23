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
    host.dispose();
  });
}
