import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { startCpuProfile } from './cpu-profile.mjs';

test('disabled sampling never creates an inspector session or output', async () => {
  const unexpected = () => { throw new Error('sampling was disabled'); };
  assert.equal(await startCpuProfile(undefined, { createSession: unexpected, writeProfile: unexpected }), undefined);
});

test('local sampling uses 1000us interval and stops/disconnects before writing once', async () => {
  const events = [];
  const profile = { nodes: [], samples: [], timeDeltas: [] };
  const sampler = await startCpuProfile('/tmp/test.cpuprofile', {
    createSession: () => ({
      connect() { events.push('connect'); },
      async post(method, params) { events.push([method, params]); return { profile }; },
      disconnect() { events.push('disconnect'); },
    }),
    async writeProfile(path, actual) {
      events.push('write');
      assert.equal(path, '/tmp/test.cpuprofile');
      assert.equal(actual, profile);
    },
  });
  const stopped = sampler.stop();
  assert.equal(sampler.stop(), stopped);
  await stopped;
  assert.equal(sampler.stop(), stopped);
  assert.deepEqual(events, ['connect', ['Profiler.enable', undefined], ['Profiler.setSamplingInterval', { interval: 1000 }],
    ['Profiler.start', undefined], ['Profiler.stop', undefined], 'disconnect', 'write']);
});

test('setup and stop failures disconnect and preserve the original exception', async () => {
  for (const failure of ['Profiler.enable', 'Profiler.setSamplingInterval', 'Profiler.start', 'Profiler.stop']) {
    const error = new Error(failure);
    let disconnected = 0;
    let written = 0;
    const createSession = () => ({
      connect() {},
      async post(method) { if (method === failure) throw error; return {}; },
      disconnect() { disconnected++; },
    });
    const pending = startCpuProfile('/tmp/test.cpuprofile', { createSession, writeProfile: () => written++ });
    if (failure === 'Profiler.stop') {
      const sampler = await pending;
      const stopped = sampler.stop();
      assert.equal(sampler.stop(), stopped);
      await assert.rejects(stopped, (actual) => actual === error);
    } else await assert.rejects(pending, (actual) => actual === error);
    assert.equal(disconnected, 1, failure);
    assert.equal(written, 0, failure);
  }
});

test('output failure happens after disconnect and cannot restart profiling', async () => {
  let disconnected = false;
  const error = new Error('output failure');
  const sampler = await startCpuProfile('/tmp/test.cpuprofile', {
    createSession: () => ({ connect() {}, post: async () => ({ profile: {} }), disconnect() { disconnected = true; } }),
    writeProfile: async () => { assert.equal(disconnected, true); throw error; },
  });
  const stopped = sampler.stop();
  await assert.rejects(stopped, (actual) => actual === error);
  assert.equal(sampler.stop(), stopped);
});

test('CPU profile flag rejects TS before starting a replay', () => {
  const result = spawnSync(process.execPath, [new URL('./replay.mjs', import.meta.url).pathname,
    '--impl', 'ts', '--cpu-profile', '/tmp/unused-test.cpuprofile'], { encoding: 'utf8' });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /cpu-profile is supported only for impl rust/);
});
