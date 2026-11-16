import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { EngineHost } from './host.mjs';

const manifest = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8');
for (const family of ['asterdex', 'hyperliquid', 'orderly', 'paradex', 'polymarket', 'txflow']) {
  test(`${family} keeps admission dispatch opt-in until its own activation slice`, () => {
    const closure = manifest.split('\n').find(line => line.startsWith(`${family} = [`));
    assert(closure);
    assert(!closure.includes('"hosted-join-proof"'));
  });
}
// SDK #3576 and #3578 activate Lighter and GRVT alongside Backpack.
for (const family of ['backpack', 'grvt', 'lighter']) {
  test(`${family} carries admission dispatch in its standalone family`, () => {
    const closure = manifest.split('\n').find(line => line.startsWith(`${family} = [`));
    assert(closure.includes('"hosted-join-proof"'));
  });
}

// Transport/capture lifecycle only; native and real WASM tests verify cryptography.
function rig(signer) {
  const calls = [];
  const captures = new Map();
  let next = 0;
  let now = 1000;
  const timers = new Map();
  const result = { id: 'attempt', joinProof: { version: 1, signature: 'captured-signature' } };
  const engine = {
    useExternalSigner() {},
    prepareJoinProof(raw) {
      calls.push(raw);
      const id = ++next;
      captures.set(id, true);
      return JSON.stringify({ id, deadlineMs: now + 5000, work: { receipt: {}, context: {}, operations: [] } });
    },
    localHostedSigning(id) { return this.finishHostedSigning(id); },
    finishHostedSigning(id) {
      assert(captures.delete(id), 'completion must own its original capture');
      return JSON.stringify(result);
    },
    cancelHostedSigning(id) { captures.delete(id); },
  };
  const transport = {
    now: () => now,
    setTimeout(fn) { const id = Symbol(); timers.set(id, fn); return id; },
    clearTimeout(id) { timers.delete(id); },
    fetch() { throw new Error('join proof must not perform HTTP'); },
    openSocket() { throw new Error('join proof must not open a socket'); },
  };
  return { host: new EngineHost(engine, transport, {}, signer ? { signer } : {}), calls, captures, timers, result,
    expire() { now = 6000; for (const run of [...timers.values()]) run(); } };
}

const tick = async () => { for (let n = 0; n < 10; n++) await Promise.resolve(); };

const raw = '{"id":"attempt","joinProof":{"version":1,"version":1}}';

test('join proof keeps original JSON bytes and uses the shared local capture runner without I/O', async () => {
  const r = rig();
  assert.deepEqual(await r.host.hostedJoinProof(raw), r.result);
  assert.deepEqual(r.calls, [raw], 'raw request must reach the Rust closed decoder without normalization');
  assert.equal(r.captures.size, 0);
  assert.equal(r.host.hostedSignings.size, 0);
  assert.equal(r.host.started, false);
  r.host.dispose();
});

test('asynchronous join proof uses existing signing jobs and cannot complete after disposal', async () => {
  let finish;
  const r = rig(() => new Promise(resolve => { finish = resolve; }));
  const pending = r.host.hostedJoinProof(raw);
  assert.equal(r.host.hostedSignings.size, 1);
  await tick();
  r.host.dispose();
  await assert.rejects(pending);
  finish({ receipt: {}, values: [[1]] });
  await Promise.resolve();
  assert.equal(r.captures.size, 0);
  assert.equal(r.host.hostedSignings.size, 0);
});

test('an unavailable proof entry refuses without calling an ordinary signing path', async () => {
  const r = rig();
  r.host.engine.prepareJoinProof = undefined;
  await assert.rejects(r.host.hostedJoinProof(raw), /unsupported/i);
  assert.deepEqual(r.calls, []);
  r.host.dispose();
});

test('join proof timeout retires the existing capture and drops delayed output', async () => {
  let finish;
  const r = rig(() => new Promise(resolve => { finish = resolve; }));
  const pending = r.host.hostedJoinProof(raw);
  await tick();
  r.expire();
  await assert.rejects(pending, /timed out/i);
  finish({ receipt: {}, values: [[1]] });
  await Promise.resolve();
  assert.equal(r.captures.size, 0);
  assert.equal(r.host.hostedSignings.size, 0);
  r.host.dispose();
});

test('capture handoff guard runs before looking up the engine proof entry', () => {
  const r = rig();
  const guard = r.host.assertHttpHandoffIdle;
  const prepare = r.host.engine.prepareJoinProof;
  const refused = new Error('handoff guard');
  r.host.assertHttpHandoffIdle = () => { throw refused; };
  Object.defineProperty(r.host.engine, 'prepareJoinProof', { configurable: true, get() { throw new Error('entry getter ran before guard'); } });
  try { assert.throws(() => r.host.hostedJoinProof(raw), error => error === refused); }
  finally {
    Object.defineProperty(r.host.engine, 'prepareJoinProof', { configurable: true, value: prepare });
    r.host.assertHttpHandoffIdle = guard;
    r.host.dispose();
  }
});
