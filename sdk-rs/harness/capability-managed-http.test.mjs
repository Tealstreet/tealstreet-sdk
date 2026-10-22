import assert from 'node:assert/strict';
import { test } from 'node:test';
import { probeCapabilityCommand } from './capability-conformance.mjs';
const command = { op: 'fetchAccount', command: 17 };
const packet = { request: 7, method: 'POST', url: 'https://venue.test/read', headers: [], timeoutMs: 1000, body: '{}' };
const preparation = 'p1:7:18446744073709551615:18446744073709551615';
const permit = 'r1:7:18446744073709551615:0:18446744073709551615';
const probe = (action) => probeCapabilityCommand({ command: () => JSON.stringify([action]) }, command, 'managed schema');
test('wide managed prepare and bind are ingress request evidence without executing', () => {
  for (const action of [
    { op: 'managedHttp', phase: 'prepare', preparation, intent: 'primary', packet },
    { op: 'managedHttp', phase: 'bind', preparation, permit, packet },
  ]) assert.equal(probe(action).evidence, 'request');
});
test('managed cancel is deferred and never physical request evidence', () => {
  assert.equal(probe({ op: 'managedHttp', phase: 'cancel', preparation, signing: 7 }).evidence, 'deferred');
  assert.equal(probe({ op: 'managedHttp', phase: 'cancel', preparation: 'p1:7:1:0' }).evidence, 'deferred');
});

const prepare = () => ({ op: 'managedHttp', phase: 'prepare', preparation, intent: 'primary', packet: { ...packet } });
const bind = () => ({ op: 'managedHttp', phase: 'bind', preparation, permit, packet: { ...packet } });
test('managed token parsing rejects aliases, lossy numbers, arity and counter overflow', () => {
  for (const invalid of [
    7, '', 'p1:7:1', 'p1:7:1:0:0', 'p2:7:1:0',
    'p1:0:1:0', 'p1:4294967296:1:0', 'p1:7:0:0',
    'p1:07:1:0', 'p1:7:01:0', 'p1:7:1:00', 'p1:+7:1:0',
    'p1:7:1: 0', 'p1:7:1:0\n', 'p1:7:1:1e2', 'p1:7:1:١',
    'p1:7:18446744073709551616:0', 'p1:7:1:18446744073709551616',
  ]) assert.throws(() => probe({ ...prepare(), preparation: invalid }), /managed schema/);
  for (const invalid of [
    'r1:7:1:0:0', 'r1:7:1:0', 'r1:7:1:0:1:1',
    'r1:7:1:0:18446744073709551616', 'r1:7:1:00:1',
    'r1:7:18446744073709551614:0:1', 'r1:8:18446744073709551615:0:1',
  ]) assert.throws(() => probe({ ...bind(), permit: invalid }), /managed schema/);
});
test('managed schemas are closed per phase and packets retain exact request correlation', () => {
  for (const action of [
    { ...prepare(), phase: 'send' }, { ...prepare(), permit },
    { ...prepare(), signing: 7 }, { ...prepare(), intent: 'secret' },
    { ...bind(), intent: 'primary' }, { ...bind(), signing: 7 },
    { op: 'managedHttp', phase: 'cancel', preparation, packet },
    { op: 'managedHttp', phase: 'cancel', preparation, signing: null },
    { op: 'managedHttp', phase: 'cancel', preparation, signing: 4294967296 },
    { ...prepare(), extra: true },
  ]) assert.throws(() => probe(action), /managed schema/);
  for (const patch of [
    { request: 8 }, { request: 7.1 }, { request: '7' }, { request: 0 },
    { method: 'TRACE' }, { url: null }, { headers: [['x']] },
    { body: null }, { route: 'unrecognized' }, { route: null },
    { timeoutMs: 1.5 }, { timeoutMs: 4294967296 }, { timeoutMs: null },
    { secret: 'not an outbound field' },
  ]) assert.throws(() => probe({ ...prepare(), packet: { ...packet, ...patch } }), /managed schema/);
});
test('optional fields match the encoder and valid route/intent declarations stay passive', () => {
  assert.equal(probe({ ...prepare(), preparation: 'p1:7:1:0', packet: { ...packet, timeoutMs: 0, body: '' } }).evidence, 'request');
  assert.equal(probe({ ...prepare(), preparation: 'p1:4294967295:1:0', packet: { ...packet, request: 4294967295 } }).evidence, 'request');
  for (const intent of ['public', 'primary', 'secondary']) {
    for (const route of ['primary', 'direct', 'publicCors']) {
      const action = prepare(); action.intent = intent; action.packet.route = route;
      delete action.packet.body; delete action.packet.timeoutMs;
      assert.equal(probe(action).evidence, 'request');
    }
  }
  const output = [prepare(), { op: 'done', command: 17, error: 'not implemented', detail: { kind: 'unsupported' } }];
  assert.throws(() => probeCapabilityCommand({ command: () => JSON.stringify(output) }, command, 'managed unsupported'), /managed unsupported/);
});
