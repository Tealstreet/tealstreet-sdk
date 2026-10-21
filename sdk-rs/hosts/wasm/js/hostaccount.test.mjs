import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

test('account snapshots use the native private API without flushing or emitting callbacks', () => {
  const frame = new Uint8Array([1, 2, 3]);
  const calls = [];
  const engine = {
    accountSnapshotFrame(now) { calls.push(now); return frame; },
    snapshotFrame() { throw new Error('full snapshots replay public state'); },
    takeFrame() { throw new Error('ordinary deltas must not be consumed'); },
  };
  const host = new EngineHost(engine, { now: () => 123 }, {
    onFrame() { throw new Error('the caller owns application and forwarding'); },
  });
  assert.deepEqual(new Uint8Array(host.accountSnapshot()), frame);
  assert.deepEqual(new Uint8Array(host.accountSnapshot()), frame);
  assert.deepEqual(calls, [123, 123]);
  assert.equal(host.dead, false);
  host.dispose();
});

test('an unsupported older engine fails the read visibly without poisoning it or using cached state', () => {
  const host = new EngineHost({}, { now: () => 1 });
  assert.throws(() => host.accountSnapshot(), /does not support private account snapshots/);
  assert.equal(host.dead, false);
  host.dispose();
});

test('disposed account reads never reenter native code', () => {
  let calls = 0;
  const host = new EngineHost({ accountSnapshotFrame() { calls++; return new Uint8Array(); } }, { now: () => 1 });
  host.dispose();
  assert.throws(() => host.accountSnapshot(), (error) => error.code === 'ENGINE_DISPOSED');
  assert.equal(calls, 0);
});

test('a trap during account snapshot kills the engine and preserves unknown writes and their identities', async () => {
  let reads = 0;
  let deaths = 0;
  const timers = new Map();
  const engine = {
    command(json) {
      const { command } = JSON.parse(json);
      return JSON.stringify([{ op: 'commandProgress', command, receipts: [
        { symbol: 'BTCUSDT', kind: 'clientOrderId', identity: 'actual-native' },
      ] }]);
    },
    accountSnapshotFrame() { reads++; throw new WebAssembly.RuntimeError('unreachable'); },
  };
  const host = new EngineHost(engine, {
    now: () => 1,
    setTimeout(callback) { timers.set(1, callback); return 1; },
    clearTimeout(id) { timers.delete(id); },
  }, { onDead() { deaths++; } });
  const write = host.command({ op: 'placeOrders', orders: [{ clientOrderId: 'requested' }] });
  assert.throws(() => host.accountSnapshot(), (error) => error.code === 'ENGINE_DEAD');
  await assert.rejects(write, (error) => {
    assert.equal(error.code, 'ENGINE_DEAD');
    assert.equal(error.outcome, 'unknown');
    assert.equal(error.clientOrderId, 'requested');
    assert.deepEqual(error.actualClientOrderIds, ['actual-native']);
    return true;
  });
  assert.throws(() => host.accountSnapshot(), (error) => error.code === 'ENGINE_DEAD');
  assert.equal(reads, 1);
  assert.equal(deaths, 1);
  assert.equal(timers.size, 0);
});
