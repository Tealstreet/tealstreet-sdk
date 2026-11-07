import assert from 'node:assert/strict';
import test from 'node:test';
import { EngineHost, ExchangeError } from './host.mjs';

const invalidInput = (error) => error instanceof ExchangeError && error.kind === 'invalidInput';
const seeded = (seed = 0) => (length) => Uint8Array.from({ length }, () => seed++ & 255);

function rig(options = {}) {
  const progress = [];
  const installed = [];
  let calls = 0;
  const engine = {
    setEntropy(source) { installed.push('identity'); this.identity = source; },
    setSigningEntropy(source) { installed.push('signing'); this.signing = source; },
    command(json) {
      calls++;
      const { command } = JSON.parse(json);
      const identity = this.identity ? Array.from(this.identity(8)).join('-') : 'native-id';
      return JSON.stringify([
        { op: 'commandProgress', command, receipts: [{ symbol: 'BTCUSDT', kind: 'clientOrderId', identity }] },
        { op: 'done', command, result: JSON.stringify(identity) },
      ]);
    },
  };
  const host = new EngineHost(engine, { now: () => 0 }, {
    onCommandProgress(event) { progress.push(event); },
  }, options);
  return { host, engine, progress, installed, calls: () => calls,
    command: () => host.command({ op: 'placeOrders', orders: [] }) };
}

function cryptoBinding(t, value) {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'crypto');
  Object.defineProperty(globalThis, 'crypto', { configurable: true, value });
  t.after(() => {
    if (previous) Object.defineProperty(globalThis, 'crypto', previous);
    else delete globalThis.crypto;
  });
}

test('receipt identity defaults to Web Crypto without replacing either engine provider', async (t) => {
  const draws = [];
  const crypto = { getRandomValues(bytes) {
    assert.equal(this, crypto);
    draws.push(bytes.length);
    return bytes.fill(0xab);
  } };
  cryptoBinding(t, crypto);
  t.mock.method(Math, 'random', () => assert.fail('weak randomness must not be used'));
  const r = rig();
  assert.deepEqual(r.installed, []);
  assert.equal(await r.command(), 'native-id');
  assert.match(r.progress[0].receiptKey, /^abababab-abab-4bab-abab-abababababab:\d+:1$/);
  assert.deepEqual(draws, [16]);
  await r.command();
  assert.deepEqual(draws, [16], 'one session identity per host');
  r.host.dispose();
});

test('independently seeded hosts repeat native identities without sharing draw state', async () => {
  const left = rig({ entropy: seeded(7) });
  const right = rig({ entropy: seeded(7) });
  assert.deepEqual(left.installed, ['identity']);
  assert.deepEqual(right.installed, ['identity']);
  const first = await left.command();
  const second = await left.command();
  assert.equal(await right.command(), first);
  assert.equal(await right.command(), second);
  assert.notEqual(first, second);
  const session = (r) => r.progress[0].receiptKey.split(':')[0];
  assert.equal(session(left), session(right), 'receipt UUID draws repeat too');
  assert.notEqual(left.progress[0].receiptKey, right.progress[0].receiptKey,
    'host sequence retains cross-host receipt ownership');
  left.host.dispose();
  right.host.dispose();
});

test('identity and explicit signing overrides are independent and copy provider buffers', async () => {
  const shared = new Uint8Array([1, 2, 3]);
  const identity = rig({ entropy: (length) => length === 3 ? shared : new Uint8Array(length) });
  const copy = identity.engine.identity(3);
  assert.notEqual(copy, shared);
  copy.fill(99);
  assert.deepEqual(shared, new Uint8Array([1, 2, 3]));
  assert.equal(identity.engine.signing, undefined);
  const signing = rig({ signingEntropy: () => shared });
  assert.deepEqual(signing.installed, ['signing']);
  assert.equal(signing.engine.identity, undefined);
  signing.engine.signing(3).fill(88);
  assert.deepEqual(shared, new Uint8Array([1, 2, 3]));
  const both = rig({ entropy: seeded(2), signingEntropy: seeded(2) });
  assert.deepEqual(both.installed, ['identity', 'signing']);
  await both.command();
  assert.deepEqual(both.engine.signing(8), seeded(2)(8), 'ID draws never advance signing');
  identity.host.dispose();
  signing.host.dispose();
  both.host.dispose();
});

test('explicit provider options require functions and matching engine injection support', () => {
  for (const key of ['entropy', 'signingEntropy']) {
    for (const value of [null, false, 7, {}]) {
      assert.throws(() => new EngineHost({}, {}, {}, { [key]: value }), invalidInput);
    }
    assert.throws(() => new EngineHost({}, {}, {}, { [key]: seeded() }), invalidInput);
  }
  let installed = false;
  assert.throws(() => new EngineHost({ setEntropy() { installed = true; } }, {}, {}, {
    entropy: seeded(), signingEntropy: seeded(),
  }), invalidInput);
  assert.equal(installed, false, 'validate all available setters before installing any override');
  assert.throws(() => new EngineHost({ setEntropy() { throw new Error('already started'); } }, {}, {}, {
    entropy: seeded(),
  }), invalidInput);
});

test('missing Web Crypto fails construction unless identity entropy is explicit', (t) => {
  cryptoBinding(t, undefined);
  t.mock.method(Math, 'random', () => assert.fail('no weak fallback'));
  assert.throws(() => new EngineHost({}, {}), invalidInput);
  assert.throws(() => rig({ signingEntropy: seeded() }), invalidInput);
  const r = rig({ entropy: seeded() });
  assert.deepEqual(r.installed, ['identity']);
  r.host.dispose();
});

test('provider exceptions and invalid results fail before command dispatch or pending state', async () => {
  for (const source of [
    () => { throw new Error('PRIVATE_PROVIDER_DETAIL'); },
    () => [],
    () => new Uint8Array(15),
    () => Promise.resolve(new Uint8Array(16)),
  ]) {
    const r = rig({ entropy: source });
    await assert.rejects(r.command(), (error) => invalidInput(error)
      && !error.message.includes('PRIVATE_PROVIDER_DETAIL'));
    assert.equal(r.calls(), 0);
    assert.equal(r.host.commands.size, 0);
    assert.equal(r.host.receiptSession, undefined);
    r.host.dispose();
    const signing = rig({ signingEntropy: source });
    assert.throws(() => signing.engine.signing(16), invalidInput);
    signing.host.dispose();
  }
});

test('Web Crypto failure rejects receipt generation without dispatch or fallback', async (t) => {
  cryptoBinding(t, { getRandomValues() { throw new Error('unavailable'); } });
  t.mock.method(Math, 'random', () => assert.fail('no weak fallback'));
  const r = rig();
  await assert.rejects(r.command(), invalidInput);
  assert.equal(r.calls(), 0);
  r.host.dispose();
});
