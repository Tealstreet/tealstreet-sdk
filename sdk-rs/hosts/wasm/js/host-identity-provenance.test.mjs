import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

// NEW-CONTRACT boundary controls. The spy declares the proposed teardown API;
// it does not implement or prove a Rust snapshot, subject or binding.
for (const ownsEngine of [false, true]) {
  test(`NEW-CONTRACT: dispose retires ${ownsEngine ? 'owned' : 'borrowed'} identity before every cleanup boundary`, () => {
    const calls = [];
    let actionDrains = 0;
    let transports = 0;
    let host;
    const engine = {
      retireIdentitySnapshot() {
        assert.equal(this, engine);
        assert.equal(host.disposed, true, 'public ingress is fenced before retirement');
        assert.throws(() => host.health(), /disposed|stopped unexpectedly/);
        calls.push('retire identity');
        return JSON.stringify([{ op: 'wsConnect', socket: 99, url: 'wss://retired.invalid' }]);
      },
      health() { assert.fail('disposed host entered normal engine validation'); },
      free() { calls.push('free engine'); },
    };
    host = new EngineHost(engine, {
      now: () => 0,
      clearTimeout(handle) { calls.push(`clear ${handle}`); },
      connect() { transports++; return { close() {} }; },
      dispose: () => calls.push('dispose transport'),
    }, {}, { ownsEngine });
    const run = host.run.bind(host);
    host.run = (...args) => { actionDrains++; return run(...args); };
    host.commands.set(7, {
      identity: { op: 'placeOrders', clientOrderIds: ['keep-order-receipt'] },
      clearDeadline: () => calls.push('clear command deadline'),
      reject(error) {
        assert.equal(error.outcome, 'unknown', 'interrupted write evidence stays conservative');
        assert.deepEqual(error.clientOrderIds, ['keep-order-receipt']);
        calls.push('settle command');
      },
      resolve() { assert.fail('teardown must not report write success'); },
    });
    host.socketFailures.set(8, { key: 81, handle: 'socket-failure timer' });
    host.timers.set(81, 'socket-failure timer');
    host.requests.set(1, { abort: () => calls.push('abort HTTP') });
    host.signings.set(9, {
      controller: { abort: () => calls.push('abort signing') },
      timer: 'signing timer',
    });
    host.primitiveSignings.set(2, { cancel: () => calls.push('cancel primitive signing') });
    host.hostedSignings.set(3, { cancel: () => calls.push('cancel hosted signing') });
    host.sockets.set(4, { close: () => calls.push('close socket') });
    host.timers.set(5, 'ordinary timer');

    host.dispose();
    host.dispose();

    const expected = [
      'retire identity', 'clear command deadline', 'settle command',
      'clear socket-failure timer', 'abort HTTP', 'abort signing', 'clear signing timer',
      'cancel primitive signing', 'cancel hosted signing', 'close socket',
      'clear ordinary timer', 'dispose transport',
    ];
    if (ownsEngine) expected.push('free engine');
    assert.deepEqual(calls, expected, 'retirement happens once before every release');
    assert.equal(actionDrains, 0, 'private retirement cannot drain returned actions');
    assert.equal(transports, 0, 'private retirement cannot dispatch transport');
    assert.equal(host.commands.size, 0);
    assert.equal(host.socketFailures.size, 0);
    assert.equal(host.requests.size, 0);
    assert.equal(host.signings.size, 0);
    assert.equal(host.primitiveSignings.size, 0);
    assert.equal(host.hostedSignings.size, 0);
    assert.equal(host.sockets.size, 0);
    assert.equal(host.timers.size, 0);
  });
}

for (const ownsEngine of [false, true]) {
  test(`regression: a trapped ${ownsEngine ? 'owned' : 'borrowed'} engine has no export, getter or free access during cleanup`, () => {
    const calls = [];
    const engineAccess = [];
    let poisoned = false;
    let host;
    const engine = new Proxy({
      failed() { return false; },
      trap() { poisoned = true; throw new WebAssembly.RuntimeError('identity trap'); },
      retireIdentitySnapshot() { engineAccess.push('retire call'); },
      free() { engineAccess.push('free call'); },
    }, {
      get(target, property, receiver) {
        if (poisoned) engineAccess.push(`get ${String(property)}`);
        return Reflect.get(target, property, receiver);
      },
    });
    host = new EngineHost(engine, {
      now: () => 0,
      clearTimeout: (handle) => calls.push(`clear ${handle}`),
      dispose() {
        assert.equal(host.disposed, true);
        assert.equal(host.dead, true);
        calls.push('dispose transport');
      },
    }, {}, { ownsEngine });
    host.commands.set(7, {
      identity: { op: 'placeOrders', clientOrderIds: ['keep-trapped-receipt'] },
      clearDeadline: () => calls.push('clear command deadline'),
      reject(error) {
        assert.equal(error.outcome, 'unknown');
        assert.deepEqual(error.clientOrderIds, ['keep-trapped-receipt']);
        calls.push('settle command');
      },
      resolve() { assert.fail('trap cannot report write success'); },
    });
    host.requests.set(1, { abort: () => calls.push('abort HTTP') });
    host.socketFailures.set(8, { key: 81, handle: 'socket failure' });
    host.timers.set(81, 'socket failure');
    host.signings.set(2, { controller: { abort: () => calls.push('abort signing') }, timer: 'signing' });
    host.primitiveSignings.set(3, { cancel: () => calls.push('cancel primitive') });
    host.hostedSignings.set(4, { cancel: () => calls.push('cancel hosted') });
    host.sockets.set(5, { close: () => calls.push('close socket') });
    host.timers.set(6, 'ordinary');
    const trap = engine.trap;
    assert.doesNotThrow(() => host.invoke(trap));
    host.dispose();
    assert.deepEqual(calls, [
      'clear command deadline', 'settle command', 'clear socket failure', 'abort HTTP',
      'abort signing', 'clear signing', 'cancel primitive', 'cancel hosted',
      'close socket', 'clear ordinary', 'dispose transport',
    ]);
    for (const map of [host.commands, host.requests, host.socketFailures, host.signings,
      host.primitiveSignings, host.hostedSignings, host.sockets, host.timers]) assert.equal(map.size, 0);
    assert.equal(host.ownsEngine, false);
    assert.deepEqual(engineAccess, [], 'poisoned teardown must not inspect or enter the engine');
  });
}

test('NEW-CONTRACT: a throwing healthy retirement cannot strand command or resource cleanup', () => {
  const calls = [];
  const host = new EngineHost({
    retireIdentitySnapshot() { calls.push('retire identity'); throw new Error('retirement failed'); },
    free() { calls.push('free engine'); },
  }, { now: () => 0, dispose: () => calls.push('dispose transport') }, {}, { ownsEngine: true });
  host.commands.set(7, {
    identity: { op: 'placeOrders', clientOrderIds: ['keep-throwing-receipt'] },
    clearDeadline: () => calls.push('clear command deadline'),
    reject(error) {
      assert.equal(error.outcome, 'unknown');
      assert.deepEqual(error.clientOrderIds, ['keep-throwing-receipt']);
      calls.push('settle command');
    },
    resolve() { assert.fail('retirement failure cannot report write success'); },
  });
  host.requests.set(1, { abort: () => calls.push('abort HTTP') });
  host.primitiveSignings.set(2, { cancel: () => calls.push('cancel primitive') });
  host.hostedSignings.set(3, { cancel: () => calls.push('cancel hosted') });
  host.sockets.set(4, { close: () => calls.push('close socket') });
  assert.doesNotThrow(() => host.dispose());
  host.dispose();
  assert.deepEqual(calls.filter(call => call !== 'retire identity'), [
    'clear command deadline', 'settle command', 'abort HTTP', 'cancel primitive',
    'cancel hosted', 'close socket', 'dispose transport', 'free engine',
  ]);
  for (const map of [host.commands, host.requests, host.primitiveSignings,
    host.hostedSignings, host.sockets]) assert.equal(map.size, 0);
  assert.deepEqual(calls, [
    'retire identity', 'clear command deadline', 'settle command', 'abort HTTP',
    'cancel primitive', 'cancel hosted', 'close socket', 'dispose transport', 'free engine',
  ]);
});

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

function injectedConstruction() {
  const calls = [];
  const sources = {};
  let sealed = false;
  const open = () => { assert.equal(sealed, false, 'configuration is frozen after seal'); };
  const engine = {
    identitySnapshotRequired: () => true,
    setEntropy(source) { open(); sources.identity = source; calls.push('identity entropy'); },
    setSigningEntropy(source) { open(); sources.signing = source; calls.push('signing entropy'); },
    requiresExternalSigner: () => true,
    useExternalSigner(algorithms) { open(); calls.push(['external signer', JSON.parse(algorithms)]); },
    sealIdentitySnapshot() { open(); sealed = true; calls.push('seal identity'); },
    retireIdentitySnapshot() {},
    start() { calls.push('start engine'); return ''; },
  };
  const host = new EngineHost(engine, { now: () => 0 }, {}, {
    entropy: (length) => new Uint8Array(length).fill(17),
    signingEntropy: (length) => new Uint8Array(length).fill(29),
    signer: async () => { assert.fail('construction does not start a signing job'); },
    signerAlgorithms: ['hmac-sha256'],
  });
  return { host, engine, calls, sources };
}

test('working control: construction preserves both entropy sources and external signer injection', () => {
  const { host, calls, sources } = injectedConstruction();
  assert.deepEqual(calls.slice(0, 3), [
    'identity entropy', 'signing entropy', ['external signer', ['hmac-sha256']],
  ]);
  const identity = sources.identity(4);
  const signing = sources.signing(4);
  assert.deepEqual([...identity], [17, 17, 17, 17]);
  assert.deepEqual([...signing], [29, 29, 29, 29]);
  host.dispose();
});

test('NEW-CONTRACT: final seal follows host injection and precedes startup', () => {
  const { host, calls } = injectedConstruction();
  try {
    host.start();
    assert.deepEqual(calls, [
      'identity entropy', 'signing entropy', ['external signer', ['hmac-sha256']],
      'seal identity', 'start engine',
    ]);
  } finally { host.dispose(); }
});
