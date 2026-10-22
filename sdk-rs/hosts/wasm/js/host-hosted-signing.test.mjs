import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const tick = async () => { for (let n = 0; n < 10; n++) await Promise.resolve(); };
const request = { kind: 'http', payload: 'note=a%2Bb&qty=0.0100', stampMs: 1000, revision: '7' };
const work = {
  receipt: { stampMs: 1000 },
  context: { venue: 'bybit', method: 'HOSTED', host: '', path: '' },
  operations: [
    { kind: 'present', key: 'owned.primary.credential' },
    { kind: 'sign', key: 'owned.primary.signing', algorithm: 'hmacSha256', payload: [{ kind: 'bytes', bytes: [43, 37, 0, 195, 169] }] },
  ],
};
const result = () => ({ values: [[107, 101, 121], Array(32).fill(55)], receipt: { stampMs: 1000 } });

// The fake models the native one-shot capture boundary, not cryptography. Actual
// WASM tests separately verify family canonical work and provider execution.
function rig(signer) {
  let now = 1000;
  let nextTimer = 1;
  let nextCapture = 1;
  let revision = '7';
  const timers = new Map();
  const captures = new Map();
  const calls = [];
  const engine = {
    useExternalSigner() { calls.push(['external']); },
    start() { throw new Error('signing must not start the venue'); },
    hostedSigningClock(at) { calls.push(['clock', at]); return JSON.stringify({ stampMs: at, revision }); },
    prepareHostedSigning(input, at) {
      calls.push(['prepare', JSON.parse(input), at]);
      const id = nextCapture++;
      captures.set(id, { revision, deadline: at + 5000 });
      return JSON.stringify({ id, work: structuredClone(work), deadlineMs: at + 5000 });
    },
    localHostedSigning(id, at) {
      assert.equal(captures.has(id), true);
      calls.push(['local', id]);
      return this.finishHostedSigning(id, JSON.stringify(result()), at);
    },
    finishHostedSigning(id, encoded, at) {
      const pending = captures.get(id);
      captures.delete(id);
      assert.ok(pending);
      calls.push(['finish', id, JSON.parse(encoded), at]);
      if (pending.revision !== revision || at >= pending.deadline) {
        const error = new Error('Signing request is invalid');
        error.kind = 'invalidInput'; error.outcome = 'notSent';
        throw error;
      }
      return JSON.stringify({ credential: 'key', signature: '37'.repeat(32) });
    },
    cancelHostedSigning(id) { calls.push(['cancel', id]); captures.delete(id); },
    signed(...args) { calls.push(['signed', ...args]); return ''; },
    signFailed(...args) { calls.push(['signFailed', ...args]); return ''; },
  };
  const transport = {
    now: () => now,
    setTimeout(fn, ms) { const id = nextTimer++; timers.set(id, { fn, ms }); return id; },
    clearTimeout(id) { timers.delete(id); },
    fetch() { throw new Error('signing must not perform HTTP'); },
    openSocket() { throw new Error('signing must not open sockets'); },
  };
  const host = new EngineHost(engine, transport, {}, signer === undefined ? {} : { signer });
  return { host, engine, calls, captures, timers, setNow: (value) => { now = value; }, setRevision: (value) => { revision = value; } };
}

test('hosted clock uses the transport clock without starting or emitting I/O', () => {
  const r = rig();
  r.setNow(1234);
  assert.deepEqual(r.host.hostedSigningClock(), { stampMs: 1234, revision: '7' });
  assert.deepEqual(r.calls, [['clock', 1234]]);
  assert.equal(r.host.started, false);
  assert.equal(r.timers.size, 0);
  r.host.dispose();
});

test('local and asynchronous injected providers complete the same one-shot work without I/O', async () => {
  for (const injected of [false, true]) {
    let seen;
    const r = rig(injected ? async (input, { signal }) => {
      assert.equal(signal.aborted, false);
      seen = structuredClone(input);
      await Promise.resolve();
      return result();
    } : undefined);
    assert.deepEqual(await r.host.hostedSigning(request), { credential: 'key', signature: '37'.repeat(32) });
    if (injected) assert.deepEqual(seen, work);
    assert.deepEqual(r.calls.find(([kind]) => kind === 'prepare'), ['prepare', request, 1000]);
    assert.equal(r.calls.filter(([kind]) => kind === 'local').length, injected ? 0 : 1);
    assert.equal(r.calls.filter(([kind]) => kind === 'finish').length, 1);
    assert.equal(r.host.started, false);
    assert.equal(r.captures.size, 0);
    assert.equal(r.host.hostedSignings.size, 0);
    assert.equal(r.timers.size, 0);
    r.host.dispose();
  }
});

test('hosted and ordinary dispatch signing use the same injected provider and receipt validation', async () => {
  const seen = [];
  const r = rig(async (input) => { seen.push(structuredClone(input)); return result(); });
  r.host.execute({ op: 'sign', attempt: 91, work: structuredClone(work), timeoutMs: 1000 });
  await r.host.hostedSigning(request);
  await tick();
  assert.deepEqual(seen, [work, work]);
  assert.equal(r.calls.filter(([kind]) => kind === 'signed').length, 1);
  assert.equal(r.calls.filter(([kind]) => kind === 'finish').length, 1);
  assert.equal(r.host.primitiveSignings.size, 0);
  r.host.dispose();
});

test('concurrent hosted captures complete in reverse order without crossing identities', async () => {
  const completions = [];
  const r = rig(() => new Promise((resolve) => { completions.push(resolve); }));
  const first = r.host.hostedSigning(request);
  const second = r.host.hostedSigning({ ...request, payload: 'second' });
  await tick();
  completions[1](result());
  await second;
  assert.deepEqual([...r.captures.keys()], [1]);
  completions[0](result());
  await first;
  assert.deepEqual(r.calls.filter(([kind]) => kind === 'finish').map((call) => call[1]), [2, 1]);
  assert.equal(r.captures.size, 0);
  r.host.dispose();
});

test('wrong receipts, counts, bytes and provider failures cancel captures with closed errors', async () => {
  const invalid = [
    () => ({ ...result(), receipt: undefined }),
    () => ({ ...result(), receipt: {} }),
    () => ({ ...result(), receipt: { stampMs: 1001 } }),
    () => ({ ...result(), receipt: { stampMs: 1000, expiryMs: 2000 } }),
    () => ({ ...result(), receipt: { stampMs: 1000, private: 'payload-do-not-leak' } }),
    () => ({ ...result(), values: [[1]] }),
    () => ({ ...result(), values: [[1], [256]] }),
    () => ({ ...result(), values: [[1], [0.5]] }),
    () => ({ ...result(), values: [[1], Array(1048577).fill(1)] }),
    () => { throw new Error('payload-do-not-leak'); },
  ];
  for (const [index, signer] of invalid.entries()) {
    const r = rig(signer);
    await assert.rejects(r.host.hostedSigning(request), (error) => {
      assert.equal(error.kind, index === invalid.length - 1 ? 'invalidInput' : 'internal');
      assert.equal(error.outcome, 'notSent');
      assert.doesNotMatch(String(error), /payload-do-not-leak/);
      return true;
    });
    assert.equal(r.calls.some(([kind]) => kind === 'finish'), false);
    assert.equal(r.captures.size, 0);
    assert.equal(r.timers.size, 0);
    r.host.dispose();
  }
});

test('provider mutation cannot rewrite the capture receipt expected by the host', async () => {
  const r = rig((input) => {
    input.receipt.stampMs = 1999;
    return { ...result(), receipt: input.receipt };
  });
  await assert.rejects(r.host.hostedSigning(request), { kind: 'internal', outcome: 'notSent' });
  assert.equal(r.captures.size, 0);
  assert.equal(r.calls.some(([kind]) => kind === 'finish'), false);
  r.host.dispose();
});

test('timeout and disposal abort, remove native captures and ignore late provider completion', async () => {
  for (const dispose of [false, true]) {
    let complete;
    let signal;
    const r = rig((_, context) => { signal = context.signal; return new Promise((resolve) => { complete = resolve; }); });
    const promise = r.host.hostedSigning(request);
    const rejected = assert.rejects(promise, (error) => error.outcome === 'notSent' && (dispose || error.kind === 'timeout'));
    await tick();
    if (dispose) r.host.dispose();
    else { r.setNow(6000); [...r.timers.values()][0].fn(); }
    await rejected;
    assert.equal(signal.aborted, true);
    assert.equal(r.captures.size, 0);
    assert.equal(r.host.hostedSignings.size, 0);
    assert.equal(r.timers.size, 0);
    complete(result());
    await tick();
    assert.equal(r.calls.some(([kind]) => kind === 'finish'), false);
    assert.equal(r.calls.filter(([kind]) => kind === 'cancel').length, 1);
    r.host.dispose();
  }
});

test('disposal before provider dispatch cancels capture without invoking the provider', async () => {
  let calls = 0;
  const r = rig(() => { calls++; return result(); });
  const pending = r.host.hostedSigning(request);
  r.host.dispose();
  await assert.rejects(pending, { outcome: 'notSent' });
  await tick();
  assert.equal(calls, 0);
  assert.equal(r.captures.size, 0);
});

test('native calibration revision rejection retires the capture and preserves the typed failure', async () => {
  let complete;
  const r = rig(() => new Promise((resolve) => { complete = resolve; }));
  const pending = r.host.hostedSigning(request);
  await tick();
  r.setRevision('8');
  complete(result());
  await assert.rejects(pending, { kind: 'invalidInput', outcome: 'notSent' });
  assert.equal(r.captures.size, 0);
  assert.equal(r.timers.size, 0);
  r.host.dispose();
});

test('unsupported and disposed engines never reach a provider', async () => {
  let calls = 0;
  const host = new EngineHost({ useExternalSigner() {} }, { now: () => 1000 }, {}, { signer: () => { calls++; return result(); } });
  assert.throws(() => host.hostedSigningClock(), { kind: 'unsupported', outcome: 'notSent' });
  await assert.rejects(host.hostedSigning(request), { kind: 'unsupported', outcome: 'notSent' });
  host.dispose();
  assert.throws(() => host.hostedSigningClock(), { code: 'ENGINE_DISPOSED' });
  await assert.rejects(host.hostedSigning(request), { code: 'ENGINE_DISPOSED' });
  assert.equal(calls, 0);
});

test('invalid native capture deadlines cancel admission without provider execution', async () => {
  for (const deadlineMs of [1000, 999, 6001, null]) {
    let calls = 0;
    const r = rig(() => { calls++; return result(); });
    r.engine.prepareHostedSigning = () => {
      r.captures.set(1, {});
      return JSON.stringify({ id: 1, work, deadlineMs });
    };
    await assert.rejects(r.host.hostedSigning(request), { kind: 'invalidInput', outcome: 'notSent' });
    assert.equal(calls, 0);
    assert.equal(r.captures.size, 0);
    r.host.dispose();
  }
});

test('fatal WASM traps during hosted signing retire the host instead of allowing reuse', async () => {
  for (const method of ['hostedSigningClock', 'prepareHostedSigning', 'localHostedSigning', 'finishHostedSigning']) {
    const r = rig();
    r.engine[method] = () => { throw new WebAssembly.RuntimeError('unreachable'); };
    if (method === 'hostedSigningClock') assert.throws(() => r.host.hostedSigningClock());
    else await assert.rejects(r.host.hostedSigning(request));
    assert.equal(r.host.dead, true, method);
    assert.equal(r.host.disposed, true, method);
    assert.equal(r.host.hostedSignings.size, 0);
    assert.equal(r.timers.size, 0);
    r.host.dispose();
  }
});

test('timer arming failure settles and cancels the native capture before provider execution', async () => {
  let providerCalls = 0;
  const r = rig(() => { providerCalls++; return result(); });
  r.host.transport.setTimeout = () => { throw new Error('private-timer-arm-text'); };
  let settled;
  r.host.hostedSigning(request).then(
    (value) => { settled = { value }; },
    (error) => { settled = { error }; },
  );
  await tick();
  assert.equal(r.captures.size, 0);
  assert.equal(r.host.hostedSignings.size, 0);
  assert.equal(r.timers.size, 0);
  assert.equal(providerCalls, 0);
  assert.equal(settled?.error?.outcome, 'notSent');
  assert.doesNotMatch(String(settled.error), /private-timer-arm-text/);
  r.host.dispose();
});

test('throwing timer cleanup cannot strand successful, timed-out or disposed hosted promises', async () => {
  for (const mode of ['success', 'timeout', 'dispose']) {
    let complete;
    let signal;
    const r = rig((_, context) => {
      signal = context.signal;
      return new Promise((resolve) => { complete = resolve; });
    });
    r.host.transport.clearTimeout = (id) => {
      r.timers.delete(id);
      throw new Error('private-timer-clear-text');
    };
    let settled;
    r.host.hostedSigning(request).then(
      (value) => { settled = { value }; },
      (error) => { settled = { error }; },
    );
    await tick();
    if (mode === 'timeout') {
      r.setNow(6000);
      assert.doesNotThrow(() => [...r.timers.values()][0].fn());
    } else if (mode === 'dispose') assert.doesNotThrow(() => r.host.dispose());
    else complete(result());
    await tick();
    assert.ok(settled, 'timer cleanup must not leave the promise pending');
    assert.equal(r.captures.size, 0);
    assert.equal(r.host.hostedSignings.size, 0);
    assert.equal(r.timers.size, 0);
    if (mode !== 'success') {
      if (mode === 'timeout') assert.equal(settled.error?.kind, 'timeout');
      assert.equal(settled.error?.outcome, 'notSent');
      assert.equal(signal.aborted, true);
      complete(result());
      await tick();
      assert.equal(r.calls.some(([kind]) => kind === 'finish'), false);
    } else assert.deepEqual(settled.value, { credential: 'key', signature: '37'.repeat(32) });
    assert.doesNotMatch(String(settled.error), /private-timer-clear-text/);
    assert.doesNotThrow(() => r.host.dispose());
  }
});

test('synchronous deadline callbacks settle after registration without invoking the provider', async () => {
  let providerCalls = 0;
  const r = rig(() => { providerCalls++; return result(); });
  r.host.transport.setTimeout = (fn, ms) => {
    const id = 100;
    r.timers.set(id, { fn, ms });
    fn();
    return id;
  };
  let settled;
  r.host.hostedSigning(request).then(
    (value) => { settled = { value }; },
    (error) => { settled = { error }; },
  );
  await tick();
  assert.equal(r.captures.size, 0);
  assert.equal(r.host.hostedSignings.size, 0);
  assert.equal(r.timers.size, 0);
  assert.equal(providerCalls, 0);
  assert.equal(settled?.error?.kind, 'timeout');
  assert.equal(settled.error?.outcome, 'notSent');
  r.host.dispose();
});


test('dispatch liveness guard leaves hosted HTTP and WS captures independently usable', async () => {
  for (const injected of [false, true]) {
    let providerCalls = 0, livenessChecks = 0;
    const r = rig(injected ? () => { providerCalls++; return result(); } : undefined);
    r.engine.signingPending = () => { livenessChecks++; return false; };
    r.host.execute({ op: 'sign', attempt: 91, work: structuredClone(work), timeoutMs: 1000 });
    await tick();
    assert.equal(providerCalls, 0);
    assert.equal(livenessChecks, 1);
    for (const input of [request, { kind: 'wsAuth', toSign: 'GET/realtime1700999999999', outputFormat: 'hex' }]) {
      assert.deepEqual(await r.host.hostedSigning(input), { credential: 'key', signature: '37'.repeat(32) });
    }
    assert.equal(livenessChecks, 1, 'hosted capture tokens must not consult dispatch attempt liveness');
    assert.equal(providerCalls, injected ? 2 : 0);
    assert.equal(r.calls.filter(([kind]) => kind === 'finish').length, 2);
    assert.equal(r.calls.some(([kind]) => kind === 'signed' || kind === 'signFailed'), false);
    assert.equal(r.captures.size, 0);
    assert.equal(r.timers.size, 0);
    r.host.dispose();
  }
});
