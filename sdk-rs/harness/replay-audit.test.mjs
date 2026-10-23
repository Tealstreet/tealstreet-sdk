import test from 'node:test';
import assert from 'node:assert/strict';
import { createHmac } from 'node:crypto';
import { createWooSocketAudit } from './woo-protocol.mjs';
import { captureReplayState } from './replay-finalize.mjs';
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
}
