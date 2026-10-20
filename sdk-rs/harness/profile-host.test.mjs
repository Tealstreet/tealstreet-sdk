import assert from 'node:assert/strict';
import test from 'node:test';
import { profileEngine } from './profile-engine.mjs';
import { profileHost } from './profile-host.mjs';

const clocks = (thread = true) => {
  let cpu = 0;
  let wall = 0;
  let reads = 0;
  const cpuSource = {};
  const clock = function (previous) {
    assert.equal(this, cpuSource);
    reads++;
    return { user: cpu - (previous?.user ?? 0), system: 0 };
  };
  cpuSource[thread ? 'threadCpuUsage' : 'cpuUsage'] = clock;
  return {
    cpuSource,
    wallNow: () => { reads++; return wall; },
    advance: (cpuMs, wallMs = cpuMs) => { cpu += cpuMs * 1000; wall += wallMs; },
    reads: () => reads,
  };
};
const messages = (count) => Array.from({ length: count }, (_, index) => [index, index + 1000, `text${index}`]).flat();

test('host diagnostics preserve receiver, arguments and result while subtracting nested engine CPU', () => {
  const clock = clocks();
  const engineProfile = profileEngine({
    consume() { clock.advance(2, 3); return 'engine-result'; },
  }, clock);
  const result = {};
  const args = [{ marker: true }, 2];
  const receiver = {
    messages: messages(2),
    drainMessages(...actual) {
      assert.equal(this, receiver);
      assert.deepEqual(actual, args);
      clock.advance(1, 2);
      assert.equal(engineProfile.engine.consume(), 'engine-result');
      clock.advance(3, 4);
      this.messages = [];
      return result;
    },
  };
  const profile = profileHost(receiver, engineProfile, clock);
  assert.equal(receiver.drainMessages(...args), result);
  assert.deepEqual(profile.snapshot(), {
    outercalls: 1, queuedMessages: 2, singletonDrains: 0, maxMessages: 2,
    wallMs: 9, cpuMs: 6, engineCpuMs: 2, outsideEngineCpuMs: 4, cpuClock: 'threadCpuUsage',
  });
});

test('empty and disposed drains run the original without reading diagnostic clocks or engine counters', () => {
  const clock = clocks();
  const engineProfile = { get methods() { assert.fail('untimed drain inspected engine counters'); } };
  const seen = [];
  const host = { messages: [], drainMessages(value) { seen.push([this, value]); return value; } };
  const profile = profileHost(host, engineProfile, clock);
  assert.equal(host.drainMessages('empty'), 'empty');
  host.messages = messages(1);
  host.disposed = true;
  assert.equal(host.drainMessages('disposed'), 'disposed');
  assert.equal(clock.reads(), 0);
  assert.deepEqual(seen, [[host, 'empty'], [host, 'disposed']]);
  assert.equal(profile.snapshot().outercalls, 0);
});

test('a borrowed wrapper keeps the invocation receiver instead of binding the profiled host', () => {
  const clock = clocks();
  const host = { messages: [], value: 1, drainMessages(increment) {
    this.value += increment;
    this.messages = [];
    clock.advance(1);
    return this.value;
  } };
  const profile = profileHost(host, { methods: {} }, clock);
  const other = { messages: messages(1), value: 10 };
  assert.equal(host.drainMessages.call(other, 3), 13);
  assert.equal(host.value, 1);
  assert.equal(other.value, 13);
  assert.equal(profile.snapshot().singletonDrains, 1);
});

test('reentrant nonempty drains contribute once to outer timing and engine totals', () => {
  const clock = clocks();
  const engineProfile = profileEngine({ consume() { clock.advance(1); } }, clock);
  const seen = [];
  const host = {
    messages: messages(2),
    drainMessages(label) {
      seen.push(label);
      clock.advance(1);
      this.messages = [];
      engineProfile.engine.consume();
      if (label === 'outer') {
        this.messages = messages(1);
        this.drainMessages('inner');
      }
    },
  };
  const profile = profileHost(host, engineProfile, clock);
  host.drainMessages('outer');
  assert.deepEqual(seen, ['outer', 'inner']);
  assert.deepEqual(profile.snapshot(), {
    outercalls: 1, queuedMessages: 2, singletonDrains: 0, maxMessages: 2,
    wallMs: 4, cpuMs: 4, engineCpuMs: 2, outsideEngineCpuMs: 2, cpuClock: 'threadCpuUsage',
  });
  // Four outer readings plus two engine calls' four readings apiece.
  assert.equal(clock.reads(), 12);
});

test('original thrown values are retained and the outer guard resets for the next drain', () => {
  const clock = clocks();
  const failure = {};
  let throws = true;
  const host = {
    messages: messages(1),
    drainMessages() {
      clock.advance(1);
      if (throws) throw failure;
      this.messages = [];
      return 7;
    },
  };
  const profile = profileHost(host, { methods: {} }, clock);
  assert.throws(() => host.drainMessages(), (actual) => actual === failure);
  throws = false;
  host.messages = messages(3);
  assert.equal(host.drainMessages(), 7);
  assert.deepEqual(profile.snapshot(), {
    outercalls: 2, queuedMessages: 4, singletonDrains: 1, maxMessages: 3,
    wallMs: 2, cpuMs: 2, engineCpuMs: 0, outsideEngineCpuMs: 2, cpuClock: 'threadCpuUsage',
  });
});

test('snapshots remain immutable and retain a raw signed residual', () => {
  const clock = clocks();
  const engineProfile = { methods: { wsMessages: { cpuMs: 10 } } };
  const host = { messages: messages(1), drainMessages() {
    clock.advance(1);
    engineProfile.methods.wsMessages.cpuMs += 2;
    this.messages = [];
  } };
  const profile = profileHost(host, engineProfile, clock);
  host.drainMessages();
  const first = profile.snapshot();
  assert.equal(first.outsideEngineCpuMs, -1);
  assert(Object.isFrozen(first));
  assert.throws(() => { first.outercalls = 0; }, TypeError);
  host.messages = messages(1);
  host.drainMessages();
  assert.equal(profile.snapshot().outercalls, 2);
  assert.equal(first.outercalls, 1);
  assert.equal(first.outsideEngineCpuMs, -1);
});

test('process fallback uses the same explicit CPU clock label as the engine profiler', () => {
  const clock = clocks(false);
  const engineProfile = profileEngine({ consume() { clock.advance(1); } }, clock);
  const host = { messages: messages(1), drainMessages() {
    engineProfile.engine.consume();
    clock.advance(2);
    this.messages = [];
  } };
  const profile = profileHost(host, engineProfile, clock);
  host.drainMessages();
  assert.equal(profile.cpuClock, engineProfile.cpuClock);
  assert.equal(profile.snapshot().cpuClock, 'cpuUsage (process fallback; includes other threads)');
  assert.equal(profile.snapshot().cpuMs, 3);
  assert.equal(profile.snapshot().engineCpuMs, 1);
});
