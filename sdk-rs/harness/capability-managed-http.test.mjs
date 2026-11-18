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

test('compact browser class metadata is closed and only belongs to preparation', () => {
  for (const requestClass of [0, 1, 2, 3, 4]) {
    assert.equal(probe({ ...prepare(), requestClass }).evidence, 'request');
  }
  for (const requestClass of [-1, 5, 255, 1.5, 'history', '0', null, false]) {
    assert.throws(() => probe({ ...prepare(), requestClass }), /managed schema/);
  }
  assert.throws(() => probe({ ...bind(), requestClass: 3 }), /managed schema/);
});

const wide = '18446744073709551615';
const preparationV2 = `p2:${wide}:7:${wide}:${wide}`;
const permitV2 = `r2:${wide}:7:${wide}:0:${wide}`;
const prepareV2 = () => ({ ...prepare(), preparation: preparationV2 });
const bindV2 = () => ({ ...bind(), preparation: preparationV2, permit: permitV2 });

test('v2 managed prepare, bind and cancel retain v1 capability evidence', () => {
  for (const action of [prepare(), bind(), prepareV2(), bindV2()]) {
    assert.equal(probe(action).evidence, 'request');
  }
  for (const preparation of [preparationV2, 'p2:1:7:1:0']) {
    assert.equal(probe({ op: 'managedHttp', phase: 'cancel', preparation, signing: 7 }).evidence, 'deferred');
  }
  assert.equal(probe({ ...prepareV2(), requestClass: 4 }).evidence, 'request');
  assert.equal(probe({ ...bindV2(), permit: `r2:${wide}:7:${wide}:1:${wide}` }).evidence, 'request');
});

test('the widest v2 tokens preserve every u64 counter and the u32 request boundary', () => {
  const preparation = `p2:${wide}:4294967295:${wide}:${wide}`;
  const permit = `r2:${wide}:4294967295:${wide}:${wide}:${wide}`;
  assert.equal(permit.length, 97);
  for (const action of [
    { ...prepareV2(), preparation },
    { ...bindV2(), preparation, permit },
  ]) assert.equal(probe({ ...action, packet: { ...packet, request: 4294967295 } }).evidence, 'request');
});

test('v2 issuer, request, generation and permit sequence remain canonical and bounded', () => {
  for (const preparation of [
    'p2:0:7:1:0', 'p2:01:7:1:0', 'p2:+1:7:1:0', 'p2:1:7:0:0',
    'p2:1:0:1:0', 'p2:1:4294967296:1:0', 'p2:1:07:1:0', 'p2:1:7:01:0',
    'p2:1:7:1:00', 'p2:1:7:1:-1', 'p2:1:7:1:0\n', 'p2:١:7:1:0',
    'p2:18446744073709551616:7:1:0', 'p2:1:7:18446744073709551616:0',
    'p2:1:7:1:18446744073709551616', 'p2:1:7:1', 'p2:1:7:1:0:0', 'p3:1:7:1:0',
  ]) assert.throws(() => probe({ ...prepareV2(), preparation }), /managed schema/);
  for (const permit of [
    `r2:0:7:${wide}:0:1`, `r2:01:7:${wide}:0:1`, `r2:${wide}:7:${wide}:0:0`,
    `r2:${wide}:7:${wide}:00:1`, `r2:${wide}:7:${wide}:0:01`,
    `r2:${wide}:7:${wide}:18446744073709551616:1`,
    `r2:${wide}:7:${wide}:0:18446744073709551616`,
    `r2:${wide}:7:${wide}:0`, `r2:${wide}:7:${wide}:0:1:1`,
  ]) assert.throws(() => probe({ ...bindV2(), permit }), /managed schema/);
});

test('managed bind rejects mixed versions and mismatched v2 owners before accepting evidence', () => {
  for (const action of [
    { ...bindV2(), permit }, { ...bind(), permit: permitV2 },
    { ...bindV2(), permit: `r2:1:7:${wide}:0:1` },
    { ...bindV2(), permit: `r2:${wide}:8:${wide}:0:1` },
    { ...bindV2(), permit: `r2:${wide}:7:1:0:1` },
    { ...prepareV2(), packet: { ...packet, request: 8 } },
    { ...prepareV2(), permit: permitV2 }, { ...bindV2(), requestClass: 4 },
    { ...prepareV2(), intent: 'secret' }, { ...prepareV2(), extra: true },
  ]) assert.throws(() => probe(action), /managed schema/);
});
