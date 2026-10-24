import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

// NEW-CONTRACT boundary controls. The spy declares the proposed teardown API;
// it does not implement or prove a Rust snapshot, subject or binding.
for (const ownsEngine of [false, true]) {
  test(`NEW-CONTRACT: ${ownsEngine ? 'owned' : 'borrowed'} host retires identity after fencing and before cleanup`, () => {
    const calls = [];
    let host;
    const engine = {
      retireIdentitySnapshot() {
        assert.equal(this, engine);
        assert.equal(host.disposed, true, 'public ingress is fenced before retirement');
        assert.throws(() => host.health(), /disposed/);
        calls.push('retire identity');
      },
      health() { assert.fail('disposed host entered normal engine validation'); },
      free() { calls.push('free engine'); },
    };
    host = new EngineHost(engine, {
      now: () => 0,
      clearTimeout: () => calls.push('clear timer'),
      dispose: () => calls.push('dispose transport'),
    }, {}, { ownsEngine });
    host.requests.set(1, { abort: () => calls.push('abort HTTP') });
    host.primitiveSignings.set(2, { cancel: () => calls.push('cancel primitive signing') });
    host.hostedSignings.set(3, { cancel: () => calls.push('cancel hosted signing') });
    host.sockets.set(4, { close: () => calls.push('close socket') });
    host.timers.set(5, 11);

    host.dispose();
    host.dispose();

    const expected = [
      'retire identity', 'abort HTTP', 'cancel primitive signing',
      'cancel hosted signing', 'close socket', 'clear timer', 'dispose transport',
    ];
    if (ownsEngine) expected.push('free engine');
    assert.deepEqual(calls, expected, 'retirement happens once before every release');
    assert.equal(host.requests.size, 0);
    assert.equal(host.primitiveSignings.size, 0);
    assert.equal(host.hostedSignings.size, 0);
    assert.equal(host.sockets.size, 0);
    assert.equal(host.timers.size, 0);
  });
}

test('working control: public ingress is already fenced during resource cleanup', () => {
  let reads = 0;
  let checks = 0;
  let host;
  host = new EngineHost({ health() { reads++; return '{}'; } }, {
    now: () => 0,
    dispose() {
      assert.equal(host.disposed, true);
      assert.throws(() => host.health(), /disposed/);
      checks++;
    },
  });
  host.dispose();
  assert.equal(checks, 1);
  assert.equal(reads, 0);
});

for (const ownsEngine of [false, true]) {
  test(`working control: ${ownsEngine ? 'owned' : 'borrowed'} engine retains existing free semantics`, () => {
    let freed = 0;
    const engine = { free() { freed++; } };
    const host = new EngineHost(engine, { now: () => 0 }, {}, { ownsEngine });
    host.dispose();
    host.dispose();
    assert.equal(freed, ownsEngine ? 1 : 0);
    assert.equal(host.ownsEngine, false);
    assert.equal(host.engine, engine);
  });
}

test('working control: a throwing resource cannot strand later cleanup', () => {
  const calls = [];
  const host = new EngineHost({ free: () => calls.push('free engine') }, {
    now: () => 0,
    dispose: () => calls.push('dispose transport'),
  }, {}, { ownsEngine: true });
  host.requests.set(1, { abort() { calls.push('abort HTTP'); throw new Error('transport failed'); } });
  host.sockets.set(2, { close: () => calls.push('close socket') });
  assert.doesNotThrow(() => host.dispose());
  assert.deepEqual(calls, ['abort HTTP', 'close socket', 'dispose transport', 'free engine']);
  assert.equal(host.requests.size, 0);
  assert.equal(host.sockets.size, 0);
});
