import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createWooSocketAudit } from './woo-protocol.mjs';
import { captureReplayState, finalizeReplay } from './replay-finalize.mjs';
const pub = 'wss://wss.woox.io/ws/stream/undefined';
const priv = 'wss://wss.woox.io/v2/ws/private/stream/undefined';
const send = (audit, url, value) => audit.sent(url, JSON.stringify(value));
function beforePrivateAck(impl) {
  const audit = createWooSocketAudit(impl, { isPrivate: true });
  audit.connected(pub); audit.connected(priv);
  for (const topic of ['tickers', 'bbos', 'markprices']) send(audit, pub, { event: 'subscribe', topic });
  send(audit, pub, { event: 'ping' });
  const timestamp = Date.now();
  const sign = createHmac('sha256', 'replay-secret').update(`|${timestamp}`).digest('hex');
  send(audit, priv, { event: 'auth', params: { apikey: 'replay-key', timestamp, sign } });
  assert.equal(audit.ready(), true, 'actual startup readiness permits the first checkpoint before the ACK');
  return audit;
}
function complete(audit) {
  audit.received(priv, JSON.stringify({ event: 'auth', success: true }));
  for (const topic of ['executionreport', 'algoexecutionreportv2']) send(audit, priv, { event: 'subscribe', topic });
  send(audit, priv, { event: 'ping' });
}
for (const impl of ['ts', 'rust']) {
  test(`${impl}: a complete protocol lifecycle has no diagnostics`, () => {
    const audit = beforePrivateAck(impl); complete(audit); assert.deepEqual(captureReplayState({}, audit, { final: true }).protocolErrors, []);
  });
  test(`${impl}: discarded checkpoint diagnostics must not poison a complete final lifecycle`, () => {
    const audit = beforePrivateAck(impl);
    captureReplayState({ checkpoint: 0 }, audit);
    complete(audit);
    assert.deepEqual(captureReplayState({}, audit, { final: true }).protocolErrors, [], 'the only difference from the working control is an intermediate observation');
  });
  test(`${impl}: real data-before-auth violations remain failures after recovery`, () => {
    const audit = beforePrivateAck(impl);
    audit.received(priv, JSON.stringify({ topic: 'executionreport', data: {} }));
    complete(audit);
    assert.deepEqual(captureReplayState({}, audit, { final: true }).protocolErrors.sort(), ['WOO frame arrived without subscription executionreport', 'WOO private data preceded authentication'].sort());
  });
  test(`${impl}: a genuine late private error survives finalization`, async () => {
    const audit = beforePrivateAck(impl);
    captureReplayState({ checkpoint: 0 }, audit);
    complete(audit);
    audit.received(priv, JSON.stringify({ topic: 'unsubscribed-private-topic', data: {} }));
    const result = await finalizeReplay(
      () => captureReplayState({ frames: 0 }, audit, { final: true }),
      () => {
        audit.received(priv, JSON.stringify({ topic: 'after-logical-end', data: {} }));
        return { cpuMs: 1 };
      },
    );
    assert.deepEqual(result.protocolErrors, ['WOO frame arrived without subscription unsubscribed-private-topic']);
    assert.ok(audit.errors().includes('WOO frame arrived without subscription after-logical-end'));
  });
}

test('checkpoint capture never invokes terminal audit; final audit failures propagate', () => {
  let reads = 0;
  const audit = { errors() { reads++; throw new Error('terminal audit failed'); } };
  assert.deepEqual(captureReplayState({ frames: 0 }, audit), { frames: 0, protocolErrors: [] });
  assert.equal(reads, 0);
  assert.throws(() => captureReplayState({}, audit, { final: true }), /terminal audit failed/);
  assert.equal(reads, 1);
  assert.deepEqual(captureReplayState({}, undefined, { final: true }), { protocolErrors: [] });
});

function verifyReplayWiring(source) {
  assert.equal((source.match(/const capture = \(frames = 0, final = false\)/g) ?? []).length, 2);
  assert.equal((source.match(/return captureReplayState\(/g) ?? []).length, 2);
  assert.equal((source.match(/\}, tape\.audit, \{ final \}\)/g) ?? []).length, 2);
  assert.equal((source.match(/\.\.\.capture\(frames, true\)/g) ?? []).length, 2);
  assert.equal((source.match(/comparisonSnapshot\(capture\(point\.ordinal\)\)/g) ?? []).length, 2);
  assert.equal(source.includes('tape.audit?.errors()'), false);
}

test('both actual replay callers finalize explicitly; forgotten final flags are rejected', () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8');
  verifyReplayWiring(source);
  const flags = [...source.matchAll(/\.\.\.capture\(frames, true\)/g)];
  for (const flag of flags) {
    const broken = source.slice(0, flag.index) + flag[0].replace('true', 'false') + source.slice(flag.index + flag[0].length);
    assert.throws(() => verifyReplayWiring(broken));
  }
  assert.throws(() => verifyReplayWiring(source.replace('frames = 0, final = false', 'frames = 0, final = true')));
});
