import assert from 'node:assert/strict';
import { test } from 'node:test';
import { probeCapabilityCommand, requireCapabilityEngine } from './capability-conformance.mjs';

const command = { op: 'fetchOrderBook', command: 17, symbol: 'BTC/USDT' };
const engine = (actions) => ({ command: () => JSON.stringify(actions) });

test('same-family construction errors cannot silently remove coverage', () => {
  assert.throws(() => requireCapabilityEngine(() => { throw new Error('testnet required'); }, 'txflow'), /txflow/);
  assert.throws(() => requireCapabilityEngine(() => undefined, 'missing instance'), /missing instance/);
});

test('a returned unsupported command failure cannot pass as decoder success', () => {
  assert.throws(() => probeCapabilityCommand(engine([
    { op: 'done', command: 17, error: 'not implemented', detail: { kind: 'unsupported' } },
  ]), command, 'advertised book'), /advertised book/);
});

test('a coded unsupported event cannot pass as decoder success', () => {
  assert.throws(() => probeCapabilityCommand(engine([
    { op: 'event', code: 'command.unsupported', message: 'unsupported command' },
  ]), command, 'advertised book'), /advertised book/);
});
