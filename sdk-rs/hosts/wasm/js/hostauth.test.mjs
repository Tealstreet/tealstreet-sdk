import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const evidence = (credential = 'primary', scope = 'account', outcome = 'rejected') => ({ op: 'accountAuth', credential, scope, outcome });

test('native account evidence is validated, immutable, account scoped and independent of commands', () => {
  const seen = [];
  const logs = [];
  const host = new EngineHost({}, { now: () => 1 }, {
    onAccountAuth(result) { assert.equal(Object.isFrozen(result), true); seen.push(result); },
    onLog(...args) { logs.push(args); },
  });
  const sibling = new EngineHost({}, { now: () => 1 }, { onAccountAuth() { assert.fail('wrong account'); } });
  for (const credential of ['primary', 'secondary']) {
    for (const scope of ['account', 'balance', 'orders', 'positions', 'settings']) {
      for (const outcome of ['accepted', 'rejected']) host.run(JSON.stringify([evidence(credential, scope, outcome)]));
    }
  }
  assert.equal(seen.length, 20);
  assert.deepEqual(seen[0], { credential: 'primary', scope: 'account', outcome: 'accepted' });
  assert.deepEqual(seen.at(-1), { credential: 'secondary', scope: 'settings', outcome: 'rejected' });
  assert.deepEqual(logs, []);
  assert.equal(host.dead, false);
  host.dispose();
  host.run(JSON.stringify([evidence()]));
  assert.equal(seen.length, 20);
  sibling.dispose();
});

test('invalid account evidence stays a visible protocol failure without dropping a sibling completion', async () => {
  for (const action of [
    { ...evidence(), credential: 'unknown' }, { ...evidence(), credential: ['primary'] },
    { ...evidence(), scope: 'public' }, { ...evidence(), scope: null },
    { ...evidence(), outcome: 'success' }, { ...evidence(), outcome: true },
    { op: 'accountAuth', credential: 'primary', scope: 'orders' },
    { ...evidence(), command: 7 }, { ...evidence(), key: 'must-not-forward' },
  ]) {
    const seen = [];
    const engine = { command(json) {
      const { command } = JSON.parse(json);
      return JSON.stringify([action, { op: 'done', command, result: '"known"' }]);
    } };
    const host = new EngineHost(engine, { now: () => 1 }, { onAccountAuth(result) { seen.push(result); }, onLog() {} });
    // Native success remains definitive even when another action is invalid.
    assert.equal(await host.command({ op: 'fetchAccount' }), 'known');
    assert.deepEqual(seen, []);
    assert.throws(() => host.run(JSON.stringify([action])), /Invalid account authentication action/);
    assert.equal(host.dead, false);
    host.dispose();
  }
});

test('throwing auth and diagnostic consumers cannot alter a dispatched write or skip the action tail', async () => {
  const sent = [];
  let warnings = 0;
  const original = console.warn;
  console.warn = () => { warnings++; };
  const engine = { command(json) {
    const { command } = JSON.parse(json);
    return JSON.stringify([
      { op: 'wsConnect', socket: 1, url: 'wss://fixture.invalid' },
      evidence('secondary', 'orders'),
      { op: 'wsSend', socket: 1, text: 'write-once' },
      { op: 'done', command, result: '["accepted"]' },
    ]);
  } };
  const host = new EngineHost(engine, {
    now: () => 1,
    connect: () => ({ send(text) { sent.push(text); }, close() {} }),
  }, {
    onAccountAuth() { throw new Error('consumer failed'); },
    onLog(level) { assert.equal(level, 'warning'); throw new Error('reporter failed'); },
  });
  try {
    assert.deepEqual(await host.command({ op: 'placeOrders', orders: [] }), ['accepted']);
    assert.deepEqual(sent, ['write-once']);
    assert.equal(warnings, 1);
    assert.equal(host.dead, false);
  } finally { host.dispose(); console.warn = original; }
});

test('a terminal host ignores later auth evidence and cannot invoke consumers again', async () => {
  let calls = 0;
  const host = new EngineHost({ command() { throw new WebAssembly.RuntimeError('unreachable'); } }, { now: () => 1 }, {
    onAccountAuth() { calls++; },
  });
  host.run(JSON.stringify([evidence()]));
  await assert.rejects(host.command({ op: 'fetchAccount' }), { code: 'ENGINE_DEAD' });
  host.run(JSON.stringify([evidence()]));
  assert.equal(calls, 1);
  assert.equal(host.dead, true);
});
