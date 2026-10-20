import assert from 'node:assert/strict';
import test from 'node:test';
import { profileEngine } from './profile-engine.mjs';

const clocks = (thread = true) => {
  let cpu = 0;
  let wall = 0;
  const clock = (previous) => {
    cpu += 1000;
    return { user: cpu - (previous?.user ?? 0), system: 0 };
  };
  return { cpuSource: thread ? { threadCpuUsage: clock } : { cpuUsage: clock }, wallNow: () => ++wall };
};

test('optional profiler preserves receiver and results while tallying gross thread CPU', () => {
  const target = { value: 4, add(n) { this.value += n; return this.value; } };
  const profile = profileEngine(target, clocks());
  assert.equal(profile.engine.add(2), 6);
  assert.equal(profile.engine.add(3), 9);
  assert.equal(profile.engine.value, 9);
  assert.deepEqual(profile.methods.add, { calls: 2, wallMs: 2, cpuMs: 2, cpuClock: 'threadCpuUsage' });
});

test('optional profiler records failures and preserves the original thrown value', () => {
  const error = new Error('original');
  const profile = profileEngine({ fail() { throw error; } }, clocks());
  assert.throws(() => profile.engine.fail(), (actual) => actual === error);
  assert.equal(profile.methods.fail.calls, 1);
  assert.equal(profile.methods.fail.cpuMs, 1);
});

test('older Node fallback explicitly labels process-wide CPU', () => {
  const profile = profileEngine({ noop() {} }, clocks(false));
  profile.engine.noop();
  assert.match(profile.cpuClock, /process fallback; includes other threads/);
  assert.equal(profile.methods.noop.cpuClock, profile.cpuClock);
  assert.equal(profile.methods.noop.cpuMs, 1);
});
