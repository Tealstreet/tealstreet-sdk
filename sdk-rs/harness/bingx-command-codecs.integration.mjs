// Offline production-family wire probes: returned actions are inspected, never sent.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { loadEngine } from './engine.mjs';
import { EngineHost, CommandTimeoutError, DEFAULT_COMMAND_TIMEOUT_MS } from '../hosts/wasm/js/host.mjs';
import { checkAdvertisedCommands } from './capability-conformance.mjs';
const module = await loadEngine();

function probe(command) {
  const engine = new module.WasmEngine('bingx', JSON.stringify({ publicDataOnly: true }));
  try {
    const capabilities = JSON.parse(engine.capabilities());
    assert.equal(capabilities.cancelWithSymbols, false);
    assert.equal(capabilities.orderBookQuery, false);
    assert.equal(capabilities.amendWithSymbol, false);
    let actions;
    assert.doesNotThrow(() => {
      actions = JSON.parse(engine.command(JSON.stringify({ ...command, command: 73 }), 1000));
    }, `${command.op} must decode before the existing venue guards`);
    assert.ok(Array.isArray(actions));
    assert.ok(actions.every(action => !['http', 'managedHttp', 'sign', 'wsSend', 'wsConnect'].includes(action.op)),
      'public probes must not prepare or send private writes');
    return actions;
  } finally { engine.free(); }
}

test('ordinary BingX cancellation keeps the public-account credential guard', () => {
  const actions = probe({ op: 'cancelOrders', ids: ['offline-codec-probe'] });
  assert.equal(actions.length, 1);
  const [result] = actions;
  assert.equal(result.op, 'done');
  assert.equal(result.command, 73);
  assert.equal(result.detail.kind, 'authInvalid');
  assert.equal(result.detail.outcome, 'notSent');
});
for (const command of [
  { op: 'cancelOrdersWithSymbols', orders: [{ id: 'offline-codec-probe', symbol: '__codec_probe__' }] },
  { op: 'fetchOrderBook', symbol: '__codec_probe__', limit: 1 },
  // The existing shared symbol-command feature also includes amend ingress.
  { op: 'amendOrderWithSymbol', id: 'offline-codec-probe', symbol: '__codec_probe__', price: 1 },
]) {
  test(`ordinary BingX decodes ${command.op} without dispatching an unadvertised command`, () => {
    const actions = probe(command);
    if (command.op === 'amendOrderWithSymbol') {
      assert.deepEqual(actions, [{ op: 'done', command: 73,
        error: 'symbol-aware amendment is unsupported by this venue',
        detail: { message: 'symbol-aware amendment is unsupported by this venue' } }]);
    } else {
      assert.deepEqual(actions, [{ op: 'event', level: 'warn', code: 'command.unsupported',
        message: 'unsupported BingX command' }]);
    }
  });
}

// Nonterminal native warnings still use the unchanged host deadline, never resend.
for (const command of [
  { op: 'cancelOrdersWithSymbols', orders: [{ id: 'offline-codec-probe', symbol: '__codec_probe__' }] },
  { op: 'fetchOrderBook', symbol: '__codec_probe__', limit: 1 },
]) {
  test(`host bounds unadvertised ${command.op} with its normal deadline`, async () => {
    let now = 1000, calls = 0, ioCalls = 0;
    const timers = new Map();
    const noIo = () => { ioCalls++; throw new Error('offline probe attempted IO'); };
    const transport = { now: () => now, fetch: noIo, request: noIo, connect: noIo,
      setTimeout(callback, delay) { const id = {}; timers.set(id, { callback, delay }); return id; },
      clearTimeout(id) { timers.delete(id); } };
    const engine = new module.WasmEngine('bingx', JSON.stringify({ publicDataOnly: true }));
    const invoke = engine.command.bind(engine);
    engine.command = (...args) => { calls++; return invoke(...args); };
    const host = new EngineHost(engine, transport, {}, { ownsEngine: true });
    try {
      const pending = host.command(command);
      const rejected = assert.rejects(pending, error => error instanceof CommandTimeoutError
        && error.timeoutMs === DEFAULT_COMMAND_TIMEOUT_MS);
      assert.equal(host.commands.size, 1);
      assert.equal(timers.size, 1);
      const [deadline] = timers.values();
      assert.equal(deadline.delay, 120_000);
      now += deadline.delay;
      deadline.callback();
      await rejected;
      assert.equal(host.commands.size, 0);
      assert.equal(timers.size, 0);
      assert.equal(calls, 1);
      assert.equal(ioCalls, 0);
    } finally { host.dispose(); }
  });
}

// The existing optional fixture phase supplies real Spot handlers, not admission.
if (process.env.BINGX_SPOT_FIXTURE === '1') {
  test('Spot fixture exercises every advertised command through the shared decoder', () => {
    assert.equal(typeof module.bingxSpotReplayFixture, 'function');
    const proofs = checkAdvertisedCommands(() => module.bingxSpotReplayFixture(true), 'BingX/Spot shared codec');
    assert.ok(proofs.some(({ op }) => op === 'cancelOrdersWithSymbols'));
    assert.ok(proofs.some(({ op }) => op === 'fetchOrderBook'));
  });
}
